//! Execution Events（V1.06 第十节）。
//!
//! 所有订单状态变化均产生 ExecutionEvent，记录到 CSV，支持 Replay。
//!
//! 事件类型：
//! - OrderCreated / OrderValidated / OrderQueued / OrderSubmitted
//! - OrderAccepted / OrderPartiallyFilled / OrderFilled
//! - OrderRejected / OrderExpired / OrderCancelled / OrderRetry / OrderFailed
//! - QueuePaused / QueueResumed
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

// ============================================================================
// Execution Event
// ============================================================================

/// 执行事件（V1.06 第十节）。
///
/// 所有订单状态变化均记录为事件，可写入 CSV 并用于 Replay。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ExecutionEvent {
    /// 订单创建。
    OrderCreated {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 订单校验通过。
    OrderValidated {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    /// 订单入队。
    OrderQueued {
        order_id: String,
        position: usize,
        timestamp: DateTime<Local>,
    },
    /// 订单提交到 Gateway。
    OrderSubmitted {
        order_id: String,
        gateway: String,
        timestamp: DateTime<Local>,
    },
    /// Gateway 接受订单。
    OrderAccepted {
        order_id: String,
        gateway: String,
        timestamp: DateTime<Local>,
    },
    /// 部分成交。
    OrderPartiallyFilled {
        order_id: String,
        filled: f64,
        remaining: f64,
        price: f64,
        timestamp: DateTime<Local>,
    },
    /// 完全成交。
    OrderFilled {
        order_id: String,
        avg_price: f64,
        slippage: f64,
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
    /// 订单取消。
    OrderCancelled {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 订单重试。
    OrderRetry {
        order_id: String,
        attempt: u32,
        timestamp: DateTime<Local>,
    },
    /// 订单失败。
    OrderFailed {
        order_id: String,
        error: String,
        timestamp: DateTime<Local>,
    },
    /// 队列暂停。
    QueuePaused { timestamp: DateTime<Local> },
    /// 队列恢复。
    QueueResumed { timestamp: DateTime<Local> },
}

impl ExecutionEvent {
    /// 事件类型名称（用于 CSV 分类）。
    pub fn event_name(&self) -> &'static str {
        match self {
            ExecutionEvent::OrderCreated { .. } => "OrderCreated",
            ExecutionEvent::OrderValidated { .. } => "OrderValidated",
            ExecutionEvent::OrderQueued { .. } => "OrderQueued",
            ExecutionEvent::OrderSubmitted { .. } => "OrderSubmitted",
            ExecutionEvent::OrderAccepted { .. } => "OrderAccepted",
            ExecutionEvent::OrderPartiallyFilled { .. } => "OrderPartiallyFilled",
            ExecutionEvent::OrderFilled { .. } => "OrderFilled",
            ExecutionEvent::OrderRejected { .. } => "OrderRejected",
            ExecutionEvent::OrderExpired { .. } => "OrderExpired",
            ExecutionEvent::OrderCancelled { .. } => "OrderCancelled",
            ExecutionEvent::OrderRetry { .. } => "OrderRetry",
            ExecutionEvent::OrderFailed { .. } => "OrderFailed",
            ExecutionEvent::QueuePaused { .. } => "QueuePaused",
            ExecutionEvent::QueueResumed { .. } => "QueueResumed",
        }
    }

    /// 中文事件名称。
    pub fn event_name_zh(&self) -> &'static str {
        match self {
            ExecutionEvent::OrderCreated { .. } => "订单创建",
            ExecutionEvent::OrderValidated { .. } => "校验通过",
            ExecutionEvent::OrderQueued { .. } => "入队等待",
            ExecutionEvent::OrderSubmitted { .. } => "已提交",
            ExecutionEvent::OrderAccepted { .. } => "已接受",
            ExecutionEvent::OrderPartiallyFilled { .. } => "部分成交",
            ExecutionEvent::OrderFilled { .. } => "完全成交",
            ExecutionEvent::OrderRejected { .. } => "已拒绝",
            ExecutionEvent::OrderExpired { .. } => "已过期",
            ExecutionEvent::OrderCancelled { .. } => "已取消",
            ExecutionEvent::OrderRetry { .. } => "重试",
            ExecutionEvent::OrderFailed { .. } => "失败",
            ExecutionEvent::QueuePaused { .. } => "队列暂停",
            ExecutionEvent::QueueResumed { .. } => "队列恢复",
        }
    }

    /// 关联的订单 ID（队列事件返回空串）。
    pub fn order_id(&self) -> &str {
        match self {
            ExecutionEvent::OrderCreated { order_id, .. }
            | ExecutionEvent::OrderValidated { order_id, .. }
            | ExecutionEvent::OrderQueued { order_id, .. }
            | ExecutionEvent::OrderSubmitted { order_id, .. }
            | ExecutionEvent::OrderAccepted { order_id, .. }
            | ExecutionEvent::OrderPartiallyFilled { order_id, .. }
            | ExecutionEvent::OrderFilled { order_id, .. }
            | ExecutionEvent::OrderRejected { order_id, .. }
            | ExecutionEvent::OrderExpired { order_id, .. }
            | ExecutionEvent::OrderCancelled { order_id, .. }
            | ExecutionEvent::OrderRetry { order_id, .. }
            | ExecutionEvent::OrderFailed { order_id, .. } => order_id,
            ExecutionEvent::QueuePaused { .. } | ExecutionEvent::QueueResumed { .. } => "",
        }
    }

    /// 事件时间戳。
    pub fn timestamp(&self) -> &DateTime<Local> {
        match self {
            ExecutionEvent::OrderCreated { timestamp, .. }
            | ExecutionEvent::OrderValidated { timestamp, .. }
            | ExecutionEvent::OrderQueued { timestamp, .. }
            | ExecutionEvent::OrderSubmitted { timestamp, .. }
            | ExecutionEvent::OrderAccepted { timestamp, .. }
            | ExecutionEvent::OrderPartiallyFilled { timestamp, .. }
            | ExecutionEvent::OrderFilled { timestamp, .. }
            | ExecutionEvent::OrderRejected { timestamp, .. }
            | ExecutionEvent::OrderExpired { timestamp, .. }
            | ExecutionEvent::OrderCancelled { timestamp, .. }
            | ExecutionEvent::OrderRetry { timestamp, .. }
            | ExecutionEvent::OrderFailed { timestamp, .. }
            | ExecutionEvent::QueuePaused { timestamp, .. }
            | ExecutionEvent::QueueResumed { timestamp, .. } => timestamp,
        }
    }
}

// ============================================================================
// Event Bus
// ============================================================================

/// 事件总线：收集事件并分发到 CSV 记录器 / Metrics / Replay。
///
/// 当前为简单的 Vec 收集器，未来可升级为 channel-based 广播。
#[derive(Debug, Default)]
pub struct EventBus {
    events: Vec<ExecutionEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// 发布事件。
    pub fn publish(&mut self, event: ExecutionEvent) {
        tracing::debug!(
            event = %event.event_name_zh(),
            order_id = %event.order_id(),
            "事件发布"
        );
        self.events.push(event);
    }

    /// 获取所有事件。
    pub fn events(&self) -> &[ExecutionEvent] {
        &self.events
    }

    /// 消费所有事件（清空内部缓冲区）。
    pub fn drain(&mut self) -> Vec<ExecutionEvent> {
        std::mem::take(&mut self.events)
    }

    /// 按订单 ID 过滤事件。
    pub fn events_for_order(&self, order_id: &str) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.order_id() == order_id)
            .collect()
    }

    /// 事件总数。
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 打印所有事件（中文）。
    pub fn print_all(&self) {
        println!("【事件日志】共 {} 条", self.events.len());
        println!();
        for event in &self.events {
            println!(
                "  [{}] {} - {}",
                event.timestamp().format("%H:%M:%S"),
                event.event_name_zh(),
                event.order_id()
            );
        }
    }
}

// ============================================================================
// CSV 事件记录
// ============================================================================

/// execution_events.csv 表头。
pub const EVENTS_HEADER: &[&str] = &[
    "timestamp",
    "event_type",
    "event_name_zh",
    "order_id",
    "details",
];

/// 单条事件记录（用于 CSV 序列化/反序列化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: String,
    pub event_type: String,
    pub event_name_zh: String,
    pub order_id: String,
    pub details: String,
}

impl From<&ExecutionEvent> for EventRecord {
    fn from(event: &ExecutionEvent) -> Self {
        let details = match event {
            ExecutionEvent::OrderCreated { .. } => String::new(),
            ExecutionEvent::OrderValidated { .. } => String::new(),
            ExecutionEvent::OrderQueued { position, .. } => format!("位置: {}", position),
            ExecutionEvent::OrderSubmitted { gateway, .. } => format!("Gateway: {}", gateway),
            ExecutionEvent::OrderAccepted { gateway, .. } => format!("Gateway: {}", gateway),
            ExecutionEvent::OrderPartiallyFilled {
                filled,
                remaining,
                price,
                ..
            } => {
                format!(
                    "成交: {:.2}  剩余: {:.2}  价格: {:.4}",
                    filled, remaining, price
                )
            }
            ExecutionEvent::OrderFilled {
                avg_price,
                slippage,
                ..
            } => {
                format!("均价: {:.4}  滑点: {:.2}%", avg_price, slippage * 100.0)
            }
            ExecutionEvent::OrderRejected { reason, .. } => reason.clone(),
            ExecutionEvent::OrderExpired { .. } => String::new(),
            ExecutionEvent::OrderCancelled { reason, .. } => reason.clone(),
            ExecutionEvent::OrderRetry { attempt, .. } => format!("第 {} 次", attempt),
            ExecutionEvent::OrderFailed { error, .. } => error.clone(),
            ExecutionEvent::QueuePaused { .. } => String::new(),
            ExecutionEvent::QueueResumed { .. } => String::new(),
        };

        EventRecord {
            timestamp: event.timestamp().format("%Y-%m-%d %H:%M:%S").to_string(),
            event_type: event.event_name().to_string(),
            event_name_zh: event.event_name_zh().to_string(),
            order_id: event.order_id().to_string(),
            details,
        }
    }
}

/// 确保 execution_events.csv 就绪。
pub fn ensure_events_csv(path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
    pm_storage::ensure_csv(path, EVENTS_HEADER)
}

/// 追加事件记录到 CSV。
pub fn append_events(records: &[EventRecord], path: impl AsRef<std::path::Path>) -> usize {
    pm_storage::append_records(path, records)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_creation() {
        let now = Local::now();
        let event = ExecutionEvent::OrderCreated {
            order_id: "EX-001".into(),
            timestamp: now,
        };
        assert_eq!(event.event_name(), "OrderCreated");
        assert_eq!(event.event_name_zh(), "订单创建");
        assert_eq!(event.order_id(), "EX-001");
    }

    #[test]
    fn event_bus_publish_and_drain() {
        let now = Local::now();
        let mut bus = EventBus::new();
        assert!(bus.is_empty());

        bus.publish(ExecutionEvent::OrderCreated {
            order_id: "EX-001".into(),
            timestamp: now,
        });
        bus.publish(ExecutionEvent::OrderValidated {
            order_id: "EX-001".into(),
            timestamp: now,
        });
        assert_eq!(bus.len(), 2);

        let drained = bus.drain();
        assert_eq!(drained.len(), 2);
        assert!(bus.is_empty());
    }

    #[test]
    fn event_bus_filter_by_order() {
        let now = Local::now();
        let mut bus = EventBus::new();
        bus.publish(ExecutionEvent::OrderCreated {
            order_id: "EX-001".into(),
            timestamp: now,
        });
        bus.publish(ExecutionEvent::OrderCreated {
            order_id: "EX-002".into(),
            timestamp: now,
        });
        bus.publish(ExecutionEvent::OrderFilled {
            order_id: "EX-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: now,
        });

        let ex1_events = bus.events_for_order("EX-001");
        assert_eq!(ex1_events.len(), 2);
    }

    #[test]
    fn event_record_conversion() {
        let now = Local::now();
        let event = ExecutionEvent::OrderFilled {
            order_id: "EX-001".into(),
            avg_price: 0.452,
            slippage: 0.005,
            timestamp: now,
        };
        let record = EventRecord::from(&event);
        assert_eq!(record.event_type, "OrderFilled");
        assert_eq!(record.order_id, "EX-001");
        assert!(record.details.contains("0.452"));
    }
}
