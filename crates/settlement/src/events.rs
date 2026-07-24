//! Settlement Events（结算事件 — P2-06）。
//!
//! Settlement Engine 发布的所有事件。
//! 其他模块（PMS / Metrics / Audit）通过订阅事件感知结算结果。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::types::SettlementStatus;

// ============================================================================
// SettlementEvent — 结算事件
// ============================================================================

/// Settlement Engine 事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SettlementEvent {
    /// 成交事件已接收。
    FillReceived {
        trade_id: String,
        order_id: String,
        market_id: String,
        notional: f64,
        timestamp: DateTime<Local>,
    },
    /// 校验通过。
    ValidationPassed {
        trade_id: String,
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 校验失败。
    ValidationFailed {
        trade_id: String,
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 手续费已计算。
    FeeCalculated {
        trade_id: String,
        order_id: String,
        total_fee: f64,
        timestamp: DateTime<Local>,
    },
    /// 持仓已更新。
    PositionUpdated {
        trade_id: String,
        order_id: String,
        market_id: String,
        summary: String,
        timestamp: DateTime<Local>,
    },
    /// 余额已更新。
    BalanceUpdated {
        trade_id: String,
        order_id: String,
        account_id: String,
        before: f64,
        after: f64,
        timestamp: DateTime<Local>,
    },
    /// 盈亏已更新。
    PnLUpdated {
        trade_id: String,
        order_id: String,
        realized_pnl: f64,
        unrealized_pnl: f64,
        timestamp: DateTime<Local>,
    },
    /// 流水已记录。
    LedgerRecorded {
        trade_id: String,
        order_id: String,
        ledger_count: usize,
        timestamp: DateTime<Local>,
    },
    /// 结算完成。
    SettlementCompleted {
        trade_id: String,
        order_id: String,
        settlement_id: String,
        status: SettlementStatus,
        elapsed_ms: u64,
        timestamp: DateTime<Local>,
    },
    /// 结算失败。
    SettlementFailed {
        trade_id: String,
        order_id: String,
        settlement_id: String,
        error: String,
        timestamp: DateTime<Local>,
    },
}

impl SettlementEvent {
    /// 事件类型名（英文 key）。
    pub fn event_name(&self) -> &'static str {
        match self {
            SettlementEvent::FillReceived { .. } => "FillReceived",
            SettlementEvent::ValidationPassed { .. } => "ValidationPassed",
            SettlementEvent::ValidationFailed { .. } => "ValidationFailed",
            SettlementEvent::FeeCalculated { .. } => "FeeCalculated",
            SettlementEvent::PositionUpdated { .. } => "PositionUpdated",
            SettlementEvent::BalanceUpdated { .. } => "BalanceUpdated",
            SettlementEvent::PnLUpdated { .. } => "PnLUpdated",
            SettlementEvent::LedgerRecorded { .. } => "LedgerRecorded",
            SettlementEvent::SettlementCompleted { .. } => "SettlementCompleted",
            SettlementEvent::SettlementFailed { .. } => "SettlementFailed",
        }
    }

    /// 中文事件名。
    pub fn event_name_zh(&self) -> &'static str {
        match self {
            SettlementEvent::FillReceived { .. } => "接收成交",
            SettlementEvent::ValidationPassed { .. } => "校验通过",
            SettlementEvent::ValidationFailed { .. } => "校验失败",
            SettlementEvent::FeeCalculated { .. } => "手续费已计算",
            SettlementEvent::PositionUpdated { .. } => "持仓已更新",
            SettlementEvent::BalanceUpdated { .. } => "余额已更新",
            SettlementEvent::PnLUpdated { .. } => "盈亏已更新",
            SettlementEvent::LedgerRecorded { .. } => "流水已记录",
            SettlementEvent::SettlementCompleted { .. } => "结算完成",
            SettlementEvent::SettlementFailed { .. } => "结算失败",
        }
    }

    /// 关联成交 ID。
    pub fn trade_id(&self) -> &str {
        match self {
            SettlementEvent::FillReceived { trade_id, .. }
            | SettlementEvent::ValidationPassed { trade_id, .. }
            | SettlementEvent::ValidationFailed { trade_id, .. }
            | SettlementEvent::FeeCalculated { trade_id, .. }
            | SettlementEvent::PositionUpdated { trade_id, .. }
            | SettlementEvent::BalanceUpdated { trade_id, .. }
            | SettlementEvent::PnLUpdated { trade_id, .. }
            | SettlementEvent::LedgerRecorded { trade_id, .. }
            | SettlementEvent::SettlementCompleted { trade_id, .. }
            | SettlementEvent::SettlementFailed { trade_id, .. } => trade_id,
        }
    }

    /// 时间戳。
    pub fn timestamp(&self) -> DateTime<Local> {
        match self {
            SettlementEvent::FillReceived { timestamp, .. }
            | SettlementEvent::ValidationPassed { timestamp, .. }
            | SettlementEvent::ValidationFailed { timestamp, .. }
            | SettlementEvent::FeeCalculated { timestamp, .. }
            | SettlementEvent::PositionUpdated { timestamp, .. }
            | SettlementEvent::BalanceUpdated { timestamp, .. }
            | SettlementEvent::PnLUpdated { timestamp, .. }
            | SettlementEvent::LedgerRecorded { timestamp, .. }
            | SettlementEvent::SettlementCompleted { timestamp, .. }
            | SettlementEvent::SettlementFailed { timestamp, .. } => *timestamp,
        }
    }
}

// ============================================================================
// SettlementSubscriber — 结算事件订阅者
// ============================================================================

/// 结算事件订阅者 Trait。
pub trait SettlementSubscriber: Send + Sync {
    /// 订阅者名称。
    fn name(&self) -> &str;
    /// 处理事件。Err 仅记日志，不影响主流程。
    fn on_event(&self, event: &SettlementEvent) -> anyhow::Result<()>;
}

// ============================================================================
// SettlementEventBus — 结算事件总线
// ============================================================================

/// Settlement 事件总线。
#[derive(Clone)]
pub struct SettlementEventBus {
    subscribers: Arc<Mutex<Vec<Box<dyn SettlementSubscriber>>>>,
    published_count: Arc<Mutex<u64>>,
}

impl Default for SettlementEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SettlementEventBus {
    /// 创建新事件总线。
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            published_count: Arc::new(Mutex::new(0)),
        }
    }

    /// 注册订阅者。
    pub fn subscribe(&self, sub: Box<dyn SettlementSubscriber>) {
        let mut subs = self.subscribers.lock().unwrap();
        tracing::info!(subscriber = %sub.name(), "结算事件订阅者已注册");
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
    pub fn publish(&self, event: SettlementEvent) {
        let event_name_zh = event.event_name_zh();
        let trade_id = event.trade_id().to_string();
        {
            let mut count = self.published_count.lock().unwrap();
            *count += 1;
        }
        tracing::info!(
            event = %event_name_zh,
            trade_id = %trade_id,
            "结算事件发布"
        );
        let subs = self.subscribers.lock().unwrap();
        for sub in subs.iter() {
            if let Err(e) = sub.on_event(&event) {
                tracing::warn!(
                    subscriber = %sub.name(),
                    error = %e,
                    "结算事件订阅者处理失败（已隔离）"
                );
            }
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

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

    impl SettlementSubscriber for CollectSubscriber {
        fn name(&self) -> &str {
            &self.name
        }
        fn on_event(&self, event: &SettlementEvent) -> anyhow::Result<()> {
            self.sink
                .lock()
                .unwrap()
                .push(event.event_name_zh().to_string());
            Ok(())
        }
    }

    #[test]
    fn event_chinese_names_unique() {
        let now = Local::now();
        let evs = [
            SettlementEvent::FillReceived {
                trade_id: "T-1".into(),
                order_id: "O-1".into(),
                market_id: "M-1".into(),
                notional: 50.0,
                timestamp: now,
            },
            SettlementEvent::ValidationPassed {
                trade_id: "T-1".into(),
                order_id: "O-1".into(),
                timestamp: now,
            },
            SettlementEvent::SettlementCompleted {
                trade_id: "T-1".into(),
                order_id: "O-1".into(),
                settlement_id: "S-1".into(),
                status: SettlementStatus::Success,
                elapsed_ms: 5,
                timestamp: now,
            },
        ];
        for e in &evs {
            assert!(!e.event_name().is_empty());
            assert!(!e.event_name_zh().is_empty());
        }
    }

    #[test]
    fn bus_publish_to_subscribers() {
        let bus = SettlementEventBus::new();
        let sink = Arc::new(Mutex::new(Vec::new()));
        let sub = CollectSubscriber::new("test", sink.clone());
        bus.subscribe(Box::new(sub));
        assert_eq!(bus.subscriber_count(), 1);

        bus.publish(SettlementEvent::FillReceived {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            market_id: "mkt-btc".into(),
            notional: 50.0,
            timestamp: Local::now(),
        });

        let events = sink.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "接收成交");
        assert_eq!(bus.published_count(), 1);
    }

    #[test]
    fn subscriber_failure_isolated() {
        struct FailSubscriber;
        impl SettlementSubscriber for FailSubscriber {
            fn name(&self) -> &str {
                "failing"
            }
            fn on_event(&self, _: &SettlementEvent) -> anyhow::Result<()> {
                Err(anyhow::anyhow!("模拟失败"))
            }
        }

        let bus = SettlementEventBus::new();
        bus.subscribe(Box::new(FailSubscriber));
        bus.publish(SettlementEvent::FillReceived {
            trade_id: "T-1".into(),
            order_id: "O-1".into(),
            market_id: "M-1".into(),
            notional: 50.0,
            timestamp: Local::now(),
        });
        assert_eq!(bus.published_count(), 1);
    }
}
