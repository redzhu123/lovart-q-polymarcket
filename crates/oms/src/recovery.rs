//! OMS Order Recovery（P2-04 第七节）。
//!
//! 程序启动后：
//! 1. 从 Repository 加载所有订单到内存。
//! 2. 找出所有非终态订单（待恢复）。
//! 3. 通过 Gateway 同步每个订单的最新状态（`sync_order`）。
//! 4. 更新本地 Order 状态，发布事件。
//!
//! 保证重启后 OMS 与 Gateway 状态一致。

use chrono::{DateTime, Local};
use pm_gateway::ExchangeGateway;

use crate::events::{EventBus, OrderEvent};
use crate::order::{Order, OrderStatus};
use crate::repository::OrderRepository;
use crate::state_machine::StateMachine;

// ============================================================================
// RecoveryReport — 恢复报告
// ============================================================================

/// 恢复报告：恢复完成后返回。
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    /// 程序启动时间。
    pub started_at: DateTime<Local>,
    /// 完成时间。
    pub completed_at: DateTime<Local>,
    /// 从仓库加载的总订单数。
    pub total_loaded: usize,
    /// 待恢复的活跃订单数。
    pub pending_recovery: usize,
    /// 成功同步的订单数。
    pub synced_count: usize,
    /// 同步后状态变化的订单数。
    pub status_changed_count: usize,
    /// 同步失败的订单数。
    pub failed_count: usize,
    /// 失败原因（中文）。
    pub failures: Vec<(String, String)>,
}

impl RecoveryReport {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let elapsed = (self.completed_at - self.started_at).num_milliseconds();
        format!(
            "【OMS 恢复报告】\n\
             启动时间: {}\n\
             完成时间: {}\n\
             耗时: {} ms\n\
             ───────────────────────────────\n\
             加载订单: {} 个\n\
             待恢复（活跃）: {} 个\n\
             同步成功: {} 个\n\
             状态变化: {} 个\n\
             同步失败: {} 个",
            self.started_at.format("%Y-%m-%d %H:%M:%S"),
            self.completed_at.format("%Y-%m-%d %H:%M:%S"),
            elapsed,
            self.total_loaded,
            self.pending_recovery,
            self.synced_count,
            self.status_changed_count,
            self.failed_count,
        )
    }

    /// 失败清单。
    pub fn failure_summary(&self) -> String {
        if self.failures.is_empty() {
            return "无失败".to_string();
        }
        let mut s = String::new();
        for (oid, reason) in &self.failures {
            s.push_str(&format!("  {}：{}\n", oid, reason));
        }
        s
    }
}

// ============================================================================
// Recovery
// ============================================================================

/// OMS 启动恢复器。
pub struct Recovery;

impl Recovery {
    /// 执行完整恢复流程。
    ///
    /// 流程：
    /// 1. 加载所有订单。
    /// 2. 过滤活跃订单。
    /// 3. 对每个活跃订单调用 `sync_one`。
    /// 4. 汇总报告。
    pub async fn run(
        repository: &dyn OrderRepository,
        event_bus: &EventBus,
        state_machine: &StateMachine,
        gateway: &dyn ExchangeGateway,
    ) -> RecoveryReport {
        let started_at = Local::now();
        tracing::info!(time = %started_at.format("%H:%M:%S"), "OMS 启动恢复开始");

        let all_orders = match repository.list_all() {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "恢复失败：无法加载订单列表");
                return RecoveryReport {
                    started_at,
                    completed_at: Local::now(),
                    total_loaded: 0,
                    pending_recovery: 0,
                    synced_count: 0,
                    status_changed_count: 0,
                    failed_count: 0,
                    failures: vec![("(加载)".into(), e.to_string())],
                };
            }
        };

        let total_loaded = all_orders.len();
        let pending: Vec<Order> = all_orders
            .iter()
            .filter(|o| o.status.is_active())
            .cloned()
            .collect();
        let pending_recovery = pending.len();

        tracing::info!(
            total = total_loaded,
            pending = pending_recovery,
            "OMS 待恢复订单已识别"
        );

        let mut synced = 0usize;
        let mut status_changed = 0usize;
        let mut failed = 0usize;
        let mut failures: Vec<(String, String)> = Vec::new();

        for order in &pending {
            match Self::sync_one(order, repository, event_bus, state_machine, gateway).await
            {
                SyncOutcome::Synced { status_changed: changed } => {
                    synced += 1;
                    if changed {
                        status_changed += 1;
                    }
                }
                SyncOutcome::Failed { reason } => {
                    failed += 1;
                    failures.push((order.order_id.clone(), reason));
                }
            }
        }

        let completed_at = Local::now();
        event_bus.publish(OrderEvent::RecoveryCompleted {
            recovered_count: synced,
            timestamp: completed_at,
        });

        tracing::info!(
            synced,
            status_changed,
            failed,
            "OMS 启动恢复完成"
        );

        RecoveryReport {
            started_at,
            completed_at,
            total_loaded,
            pending_recovery,
            synced_count: synced,
            status_changed_count: status_changed,
            failed_count: failed,
            failures,
        }
    }

    /// 同步单个订单：通过 Gateway 查询最新状态，更新本地。
    async fn sync_one(
        order: &Order,
        repository: &dyn OrderRepository,
        event_bus: &EventBus,
        state_machine: &StateMachine,
        gateway: &dyn ExchangeGateway,
    ) -> SyncOutcome {
        // 没有 exchange_order_id（未提交到 Gateway）：无需同步
        let Some(eid) = order.exchange_order_id.as_ref() else {
            tracing::debug!(
                order_id = %order.order_id,
                "OMS 订单无 ExchangeOrderId，跳过同步"
            );
            return SyncOutcome::Synced { status_changed: false };
        };

        // 通过 Gateway 查询
        let result = gateway.get_order(eid).await;
        let now = Local::now();
        match Self::apply_gateway_result_to_order(order, &result, state_machine, event_bus, now) {
            Ok(changed) => {
                if changed {
                    let mut updated = order.clone();
                    let new_oms_status = OrderStatus::from_execution(result.status);
                    let reason = format!(
                        "Gateway 同步：{} → {}",
                        order.status.as_zh(),
                        new_oms_status.as_zh()
                    );
                    updated.transition(new_oms_status, &reason, "recovery", now);
                    if result.filled > 0.0
                        || matches!(
                            result.status,
                            pm_execution::order::OrderStatus::Filled
                                | pm_execution::order::OrderStatus::PartiallyFilled
                        )
                    {
                        updated.update_fill(
                            result.filled,
                            result.avg_price.unwrap_or(updated.price),
                            0.0,
                        );
                    }
                    if let Err(e) = repository.save(&updated) {
                        return SyncOutcome::Failed {
                            reason: format!("持久化失败：{}", e),
                        };
                    }
                }
                SyncOutcome::Synced { status_changed: changed }
            }
            Err(e) => SyncOutcome::Failed { reason: e },
        }
    }

    /// 把 GatewayResult 应用到本地 Order（clone + 修改后由 sync_one 重新 save）。
    fn apply_gateway_result_to_order(
        order: &Order,
        result: &pm_gateway::GatewayResult,
        state_machine: &StateMachine,
        event_bus: &EventBus,
        now: DateTime<Local>,
    ) -> Result<bool, String> {
        let new_status = OrderStatus::from_execution(result.status);
        if new_status == order.status {
            return Ok(false);
        }
        if let Err(e) = state_machine.validate_transition(order.status, new_status) {
            return Err(format!(
                "状态机拒绝 {} → {}：{}",
                order.status.as_zh(),
                new_status.as_zh(),
                e
            ));
        }

        // 发布事件（无需修改 Order 自身，sync_one 已持有 &Order）
        let event = match new_status {
            OrderStatus::Filled => OrderEvent::OrderFilled {
                order_id: order.order_id.clone(),
                avg_price: result.avg_price.unwrap_or(order.price),
                slippage: 0.0,
                timestamp: now,
            },
            OrderStatus::PartiallyFilled => OrderEvent::OrderPartiallyFilled {
                order_id: order.order_id.clone(),
                filled: result.filled,
                remaining: result.remaining,
                avg_price: result.avg_price.unwrap_or(order.price),
                timestamp: now,
            },
            OrderStatus::Cancelled => OrderEvent::OrderCancelled {
                order_id: order.order_id.clone(),
                reason: format!("Gateway 同步：{}", result.message),
                timestamp: now,
            },
            OrderStatus::Rejected => OrderEvent::OrderRejected {
                order_id: order.order_id.clone(),
                reason: format!("Gateway 同步：{}", result.message),
                timestamp: now,
            },
            OrderStatus::Expired => OrderEvent::OrderExpired {
                order_id: order.order_id.clone(),
                timestamp: now,
            },
            _ => OrderEvent::OrderAccepted {
                order_id: order.order_id.clone(),
                gateway: result.gateway_order_id.clone(),
                exchange_order_id: result.gateway_order_id.clone(),
                timestamp: now,
            },
        };
        event_bus.publish(event);

        let _ = event_bus;
        Ok(true)
    }
}

/// 单个订单同步结果。
enum SyncOutcome {
    Synced { status_changed: bool },
    Failed { reason: String },
}

// ============================================================================
// 简化方案：sync_order 也作为 OMS API 暴露
// ============================================================================

/// 同步单个订单（公开 API）。
///
/// 调用 Gateway `get_order` 获取最新状态，更新本地订单。
pub async fn sync_order(
    order: &mut Order,
    gateway: &dyn ExchangeGateway,
    repository: &dyn OrderRepository,
    event_bus: &EventBus,
    state_machine: &StateMachine,
) -> Result<SyncReport, String> {
    let Some(eid) = order.exchange_order_id.as_ref() else {
        return Ok(SyncReport {
            order_id: order.order_id.clone(),
            status_changed: false,
            message: "订单尚未提交到 Gateway，无需同步".to_string(),
        });
    };
    let result = gateway.get_order(eid).await;
    let now = Local::now();
    let old_status = order.status;
    let new_status = OrderStatus::from_execution(result.status);

    if new_status == old_status {
        return Ok(SyncReport {
            order_id: order.order_id.clone(),
            status_changed: false,
            message: "状态一致，无需更新".to_string(),
        });
    }

    if let Err(e) = state_machine.validate_transition(old_status, new_status) {
        return Err(format!(
            "状态机拒绝 {} → {}：{}",
            old_status.as_zh(),
            new_status.as_zh(),
            e
        ));
    }

    let reason = format!(
        "Gateway 同步：{} → {}",
        old_status.as_zh(),
        new_status.as_zh()
    );
    order.transition(new_status, &reason, "recovery", now);

    if result.filled > 0.0 {
        order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
    }
    repository.save(order).map_err(|e| e.to_string())?;

    // 发布对应事件
    let event = match new_status {
        OrderStatus::Filled => OrderEvent::OrderFilled {
            order_id: order.order_id.clone(),
            avg_price: result.avg_price.unwrap_or(order.price),
            slippage: 0.0,
            timestamp: now,
        },
        OrderStatus::Cancelled => OrderEvent::OrderCancelled {
            order_id: order.order_id.clone(),
            reason: result.message.clone(),
            timestamp: now,
        },
        OrderStatus::Rejected => OrderEvent::OrderRejected {
            order_id: order.order_id.clone(),
            reason: result.message.clone(),
            timestamp: now,
        },
        _ => OrderEvent::OrderAccepted {
            order_id: order.order_id.clone(),
            gateway: order.gateway_name.clone(),
            exchange_order_id: result.gateway_order_id.clone(),
            timestamp: now,
        },
    };
    event_bus.publish(event);

    Ok(SyncReport {
        order_id: order.order_id.clone(),
        status_changed: true,
        message: format!(
            "{} → {}",
            old_status.as_zh(),
            order.status.as_zh()
        ),
    })
}

/// 单订单同步结果。
#[derive(Debug, Clone)]
pub struct SyncReport {
    pub order_id: String,
    pub status_changed: bool,
    pub message: String,
}

impl SyncReport {
    pub fn summary_zh(&self) -> String {
        format!(
            "订单 {}：{} | {}",
            self.order_id,
            if self.status_changed { "已变化" } else { "无变化" },
            self.message
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventBus;
    use crate::order::{Direction, Order};
    use crate::repository::memory::InMemoryRepository;
    use crate::state_machine::StateMachine;
    use chrono::Local;
    use pm_core::Side;
    use pm_gateway::{create_mock_gateway, OrderType, TimeInForce};

    fn build_order() -> Order {
        let now = Local::now();
        Order::new(
            "CLI-001".into(),
            "mkt-1".into(),
            "mock".into(),
            "MockGateway".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            OrderType::Limit,
            TimeInForce::Gtc,
            "S1".into(),
            "R1".into(),
            "O1".into(),
            now,
        )
    }

    #[test]
    fn report_summary_chinese() {
        let now = Local::now();
        let r = RecoveryReport {
            started_at: now,
            completed_at: now + chrono::Duration::seconds(1),
            total_loaded: 10,
            pending_recovery: 3,
            synced_count: 3,
            status_changed_count: 1,
            failed_count: 0,
            failures: Vec::new(),
        };
        let s = r.summary_zh();
        assert!(s.contains("OMS 恢复报告"));
        assert!(s.contains("10"));
        assert!(s.contains("3"));
    }

    #[test]
    fn report_failure_summary() {
        let mut r = RecoveryReport {
            started_at: Local::now(),
            completed_at: Local::now(),
            total_loaded: 0,
            pending_recovery: 0,
            synced_count: 0,
            status_changed_count: 0,
            failed_count: 1,
            failures: Vec::new(),
        };
        r.failures
            .push(("OMS-001".into(), "Gateway 超时".into()));
        assert!(r.failure_summary().contains("OMS-001"));
    }

    #[tokio::test]
    async fn run_with_empty_repo() {
        let repo = InMemoryRepository::new();
        let bus = EventBus::new();
        let sm = StateMachine::new();
        let gateway = create_mock_gateway();
        let report = Recovery::run(&repo, &bus, &sm, gateway.as_ref()).await;
        assert_eq!(report.total_loaded, 0);
        assert_eq!(report.pending_recovery, 0);
        assert_eq!(report.synced_count, 0);
        assert_eq!(report.failed_count, 0);
    }

    #[tokio::test]
    async fn run_with_terminal_orders_no_sync() {
        let repo = InMemoryRepository::new();
        let bus = EventBus::new();
        let sm = StateMachine::new();
        let mut o = build_order();
        o.transition(OrderStatus::Filled, "测试", "oms", Local::now());
        repo.save(&o).unwrap();
        let gateway = create_mock_gateway();
        let report = Recovery::run(&repo, &bus, &sm, gateway.as_ref()).await;
        assert_eq!(report.total_loaded, 1);
        assert_eq!(report.pending_recovery, 0);
    }

    #[tokio::test]
    async fn sync_order_without_exchange_id_noop() {
        let repo = InMemoryRepository::new();
        let bus = EventBus::new();
        let sm = StateMachine::new();
        let gateway = create_mock_gateway();
        let mut o = build_order();
        let report = sync_order(&mut o, gateway.as_ref(), &repo, &bus, &sm).await.unwrap();
        assert!(!report.status_changed);
        assert!(report.message.contains("未提交"));
    }
}