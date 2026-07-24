//! OMS Order Event Bus（P2-04 第六节）。
//!
//! 所有 OMS 订单状态变化都会发布 OrderEvent。
//! Portfolio / Metrics / Audit 等模块通过订阅事件工作，
//! OMS 不直接调用这些模块。
//!
//! # 设计
//!
//! - **同步发布**：EventBus 维护内存中的订阅者列表 + CSV 追加。
//! - **失败隔离**：订阅者处理失败不影响主流程，仅记日志。
//! - **可序列化**：所有事件实现 Serialize/Deserialize，可写 CSV 用于回放。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::order::OrderStatus;

// ============================================================================
// OrderEvent — 订单事件
// ============================================================================

/// OMS 订单事件。
///
/// 所有 OMS API 触发的事件都会通过本枚举发布。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum OrderEvent {
    /// 订单创建（OMS 内部）。
    OrderCreated {
        order_id: String,
        client_order_id: String,
        market_id: String,
        timestamp: DateTime<Local>,
    },
    /// 订单校验通过。
    OrderValidated {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 待提交（OMS 内部决策完成）。
    OrderPendingSubmit {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 已提交到 Gateway。
    OrderSubmitted {
        order_id: String,
        gateway: String,
        timestamp: DateTime<Local>,
    },
    /// Gateway 接受订单。
    OrderAccepted {
        order_id: String,
        gateway: String,
        exchange_order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 部分成交。
    OrderPartiallyFilled {
        order_id: String,
        filled: f64,
        remaining: f64,
        avg_price: f64,
        timestamp: DateTime<Local>,
    },
    /// 完全成交。
    OrderFilled {
        order_id: String,
        avg_price: f64,
        slippage: f64,
        timestamp: DateTime<Local>,
    },
    /// 订单已取消。
    OrderCancelled {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 订单被拒绝。
    OrderRejected {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 订单过期。
    OrderExpired {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 校验失败（Validator 拒绝，未提交到 Gateway）。
    ValidationFailed {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// Gateway 返回错误（适配失败）。
    GatewayError {
        order_id: String,
        gateway: String,
        error: String,
        timestamp: DateTime<Local>,
    },
    /// 状态机非法转移（被状态机拒绝）。
    StateTransitionRejected {
        order_id: String,
        from: OrderStatus,
        to: OrderStatus,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 恢复完成（程序启动恢复历史订单）。
    RecoveryCompleted {
        recovered_count: usize,
        timestamp: DateTime<Local>,
    },
}

impl OrderEvent {
    /// 事件类型名（CSV 字段）。
    pub fn event_name(&self) -> &'static str {
        match self {
            OrderEvent::OrderCreated { .. } => "OrderCreated",
            OrderEvent::OrderValidated { .. } => "OrderValidated",
            OrderEvent::OrderPendingSubmit { .. } => "OrderPendingSubmit",
            OrderEvent::OrderSubmitted { .. } => "OrderSubmitted",
            OrderEvent::OrderAccepted { .. } => "OrderAccepted",
            OrderEvent::OrderPartiallyFilled { .. } => "OrderPartiallyFilled",
            OrderEvent::OrderFilled { .. } => "OrderFilled",
            OrderEvent::OrderCancelled { .. } => "OrderCancelled",
            OrderEvent::OrderRejected { .. } => "OrderRejected",
            OrderEvent::OrderExpired { .. } => "OrderExpired",
            OrderEvent::ValidationFailed { .. } => "ValidationFailed",
            OrderEvent::GatewayError { .. } => "GatewayError",
            OrderEvent::StateTransitionRejected { .. } => "StateTransitionRejected",
            OrderEvent::RecoveryCompleted { .. } => "RecoveryCompleted",
        }
    }

    /// 中文事件名。
    pub fn event_name_zh(&self) -> &'static str {
        match self {
            OrderEvent::OrderCreated { .. } => "订单创建",
            OrderEvent::OrderValidated { .. } => "校验通过",
            OrderEvent::OrderPendingSubmit { .. } => "待提交",
            OrderEvent::OrderSubmitted { .. } => "已提交",
            OrderEvent::OrderAccepted { .. } => "已接受",
            OrderEvent::OrderPartiallyFilled { .. } => "部分成交",
            OrderEvent::OrderFilled { .. } => "完全成交",
            OrderEvent::OrderCancelled { .. } => "已取消",
            OrderEvent::OrderRejected { .. } => "已拒绝",
            OrderEvent::OrderExpired { .. } => "已过期",
            OrderEvent::ValidationFailed { .. } => "校验失败",
            OrderEvent::GatewayError { .. } => "网关错误",
            OrderEvent::StateTransitionRejected { .. } => "状态机拒绝",
            OrderEvent::RecoveryCompleted { .. } => "恢复完成",
        }
    }

    /// 关联订单 ID（恢复事件返回空）。
    pub fn order_id(&self) -> &str {
        match self {
            OrderEvent::OrderCreated { order_id, .. }
            | OrderEvent::OrderValidated { order_id, .. }
            | OrderEvent::OrderPendingSubmit { order_id, .. }
            | OrderEvent::OrderSubmitted { order_id, .. }
            | OrderEvent::OrderAccepted { order_id, .. }
            | OrderEvent::OrderPartiallyFilled { order_id, .. }
            | OrderEvent::OrderFilled { order_id, .. }
            | OrderEvent::OrderCancelled { order_id, .. }
            | OrderEvent::OrderRejected { order_id, .. }
            | OrderEvent::OrderExpired { order_id, .. }
            | OrderEvent::ValidationFailed { order_id, .. }
            | OrderEvent::GatewayError { order_id, .. }
            | OrderEvent::StateTransitionRejected { order_id, .. } => order_id,
            OrderEvent::RecoveryCompleted { .. } => "",
        }
    }

    /// 事件时间戳。
    pub fn timestamp(&self) -> DateTime<Local> {
        match self {
            OrderEvent::OrderCreated { timestamp, .. }
            | OrderEvent::OrderValidated { timestamp, .. }
            | OrderEvent::OrderPendingSubmit { timestamp, .. }
            | OrderEvent::OrderSubmitted { timestamp, .. }
            | OrderEvent::OrderAccepted { timestamp, .. }
            | OrderEvent::OrderPartiallyFilled { timestamp, .. }
            | OrderEvent::OrderFilled { timestamp, .. }
            | OrderEvent::OrderCancelled { timestamp, .. }
            | OrderEvent::OrderRejected { timestamp, .. }
            | OrderEvent::OrderExpired { timestamp, .. }
            | OrderEvent::ValidationFailed { timestamp, .. }
            | OrderEvent::GatewayError { timestamp, .. }
            | OrderEvent::StateTransitionRejected { timestamp, .. }
            | OrderEvent::RecoveryCompleted { timestamp, .. } => *timestamp,
        }
    }
}

// ============================================================================
// CSV 表头
// ============================================================================

/// OMS 事件 CSV 表头。
pub const OMS_EVENTS_HEADER: &[&str] = &[
    "timestamp",
    "event_type",
    "event_name_zh",
    "order_id",
    "extra_json",
];

// ============================================================================
// Subscriber — 订阅者 trait
// ============================================================================

/// 事件订阅者。
///
/// OMS 不直接调用 Portfolio / Metrics / Audit 等模块；
/// 这些模块通过实现本 trait 订阅 OrderEvent，自行处理。
pub trait Subscriber: Send + Sync {
    /// 订阅者名称。
    fn name(&self) -> &str;
    /// 处理事件。返回 `Err` 仅记日志，不影响 OMS 主流程。
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()>;
}

// ============================================================================
// EventBus — 事件总线
// ============================================================================

/// OMS 事件总线。
#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Box<dyn Subscriber>>>>,
    /// 已发布事件计数（不存全量，避免内存爆炸）。
    published_count: Arc<Mutex<u64>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// 创建新事件总线。
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            published_count: Arc::new(Mutex::new(0)),
        }
    }

    /// 注册订阅者。
    pub fn subscribe(&self, sub: Box<dyn Subscriber>) {
        let mut subs = self.subscribers.lock().unwrap();
        tracing::info!(subscriber = %sub.name(), "OMS 事件订阅者已注册");
        subs.push(sub);
    }

    /// 当前订阅者数量。
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().unwrap().len()
    }

    /// 已发布事件计数。
    pub fn published_count(&self) -> u64 {
        *self.published_count.lock().unwrap()
    }

    /// 发布事件（同步触发所有订阅者）。
    pub fn publish(&self, event: OrderEvent) {
        let event_name_zh = event.event_name_zh();
        let order_id = event.order_id().to_string();
        // 先计数再分发（避免订阅者处理慢导致计数偏小）
        {
            let mut count = self.published_count.lock().unwrap();
            *count += 1;
        }
        tracing::info!(
            event = %event_name_zh,
            order_id = %order_id,
            "OMS 事件发布"
        );
        let subs = self.subscribers.lock().unwrap();
        for sub in subs.iter() {
            if let Err(e) = sub.on_event(&event) {
                tracing::warn!(
                    subscriber = %sub.name(),
                    error = %e,
                    "OMS 订阅者处理失败（已隔离）"
                );
            }
        }
    }

    /// 仅记录 CSV 行（不触发订阅者），用于后台恢复 / 重放场景。
    pub fn record_only(&self, event: OrderEvent) {
        let mut count = self.published_count.lock().unwrap();
        *count += 1;
        tracing::info!(
            event = %event.event_name_zh(),
            order_id = %event.order_id(),
            "OMS 事件已记录"
        );
    }
}

// ============================================================================
// CSV 持久化辅助
// ============================================================================

/// 把 OrderEvent 序列化为 CSV 行（5 列：timestamp, event_type, event_name_zh, order_id, extra_json）。
pub fn event_to_csv_row(event: &OrderEvent) -> [String; 5] {
    let timestamp = event
        .timestamp()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    let event_type = event.event_name().to_string();
    let event_name_zh = event.event_name_zh().to_string();
    let order_id = event.order_id().to_string();
    let extra_json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    [timestamp, event_type, event_name_zh, order_id, extra_json]
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    /// 测试订阅者：把所有事件收集到一个 Mutex<Vec> 中。
    struct CollectSubscriber {
        name: String,
        sink: Arc<Mutex<Vec<String>>>,
    }
    impl CollectSubscriber {
        fn new(name: &str, sink: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name: name.to_string(),
                sink,
            }
        }
    }
    impl Subscriber for CollectSubscriber {
        fn name(&self) -> &str {
            &self.name
        }
        fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
            self.sink
                .lock()
                .unwrap()
                .push(event.event_name_zh().to_string());
            Ok(())
        }
    }

    #[test]
    fn event_chinese_name_unique() {
        let evs = [
            OrderEvent::OrderCreated {
                order_id: "X".into(),
                client_order_id: "C".into(),
                market_id: "M".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderValidated {
                order_id: "X".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderPendingSubmit {
                order_id: "X".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderSubmitted {
                order_id: "X".into(),
                gateway: "MockGateway".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderAccepted {
                order_id: "X".into(),
                gateway: "MockGateway".into(),
                exchange_order_id: "GW-1".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderPartiallyFilled {
                order_id: "X".into(),
                filled: 50.0,
                remaining: 50.0,
                avg_price: 0.45,
                timestamp: Local::now(),
            },
            OrderEvent::OrderFilled {
                order_id: "X".into(),
                avg_price: 0.45,
                slippage: 0.01,
                timestamp: Local::now(),
            },
            OrderEvent::OrderCancelled {
                order_id: "X".into(),
                reason: "用户取消".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderRejected {
                order_id: "X".into(),
                reason: "资金不足".into(),
                timestamp: Local::now(),
            },
            OrderEvent::OrderExpired {
                order_id: "X".into(),
                timestamp: Local::now(),
            },
            OrderEvent::ValidationFailed {
                order_id: "X".into(),
                reason: "价格非法".into(),
                timestamp: Local::now(),
            },
            OrderEvent::GatewayError {
                order_id: "X".into(),
                gateway: "MockGateway".into(),
                error: "超时".into(),
                timestamp: Local::now(),
            },
            OrderEvent::StateTransitionRejected {
                order_id: "X".into(),
                from: OrderStatus::Filled,
                to: OrderStatus::Created,
                reason: "终态".into(),
                timestamp: Local::now(),
            },
            OrderEvent::RecoveryCompleted {
                recovered_count: 3,
                timestamp: Local::now(),
            },
        ];
        for e in &evs {
            assert!(!e.event_name().is_empty());
            assert!(!e.event_name_zh().is_empty());
        }
    }

    #[test]
    fn bus_publish_to_subscribers() {
        let bus = EventBus::new();
        let sink = Arc::new(Mutex::new(Vec::new()));
        let sub1 = CollectSubscriber::new("portfolio", sink.clone());
        let sub2 = CollectSubscriber::new("metrics", sink.clone());
        bus.subscribe(Box::new(sub1));
        bus.subscribe(Box::new(sub2));
        assert_eq!(bus.subscriber_count(), 2);

        bus.publish(OrderEvent::OrderCreated {
            order_id: "OMS-001".into(),
            client_order_id: "C1".into(),
            market_id: "mkt-1".into(),
            timestamp: Local::now(),
        });
        bus.publish(OrderEvent::OrderFilled {
            order_id: "OMS-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: Local::now(),
        });

        let s = sink.lock().unwrap();
        assert_eq!(s.len(), 4); // 2 个订阅者 × 2 个事件
        assert!(s[0].contains("订单创建"));
        assert_eq!(bus.published_count(), 2);
    }

    #[test]
    fn subscriber_failure_isolated() {
        struct FailSubscriber;
        impl Subscriber for FailSubscriber {
            fn name(&self) -> &str {
                "failing"
            }
            fn on_event(&self, _: &OrderEvent) -> anyhow::Result<()> {
                Err(anyhow::anyhow!("模拟失败"))
            }
        }
        let bus = EventBus::new();
        bus.subscribe(Box::new(FailSubscriber));
        // 不应 panic，仅记 warn 日志
        bus.publish(OrderEvent::OrderCreated {
            order_id: "OMS-001".into(),
            client_order_id: "C1".into(),
            market_id: "mkt-1".into(),
            timestamp: Local::now(),
        });
        assert_eq!(bus.published_count(), 1);
    }

    #[test]
    fn event_to_csv_row_format() {
        let ev = OrderEvent::OrderFilled {
            order_id: "OMS-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: Local::now(),
        };
        let row = event_to_csv_row(&ev);
        assert_eq!(row.len(), 5);
        assert_eq!(row[1], "OrderFilled");
        assert_eq!(row[2], "完全成交");
        assert_eq!(row[3], "OMS-001");
        assert!(row[4].contains("avg_price"));
    }

    #[test]
    fn record_only_no_subscribers_called() {
        let bus = EventBus::new();
        bus.record_only(OrderEvent::OrderCreated {
            order_id: "X".into(),
            client_order_id: "C".into(),
            market_id: "M".into(),
            timestamp: Local::now(),
        });
        assert_eq!(bus.published_count(), 1);
        assert_eq!(bus.subscriber_count(), 0);
    }
}
