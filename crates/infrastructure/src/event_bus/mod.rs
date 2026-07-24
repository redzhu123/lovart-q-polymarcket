//! 事件总线：统一的发布-订阅事件系统。
//!
//! 从 `pm-oms::events` 提取并统一。
//!
//! # 核心能力
//!
//! - [`EventBus`]：线程安全的事件总线
//! - [`Subscriber`] trait：事件订阅者接口
//! - [`SystemEvent`]：统一的事件枚举

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// 统一系统事件枚举
///
/// 覆盖市场、订单、结算、组合、风控、系统等所有领域事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SystemEvent {
    /// 市场数据更新
    MarketUpdated {
        market_id: String,
        timestamp: DateTime<Local>,
    },
    /// 订单创建
    OrderCreated {
        order_id: String,
        market_id: String,
        timestamp: DateTime<Local>,
    },
    /// 订单成交
    OrderFilled {
        order_id: String,
        avg_price: f64,
        timestamp: DateTime<Local>,
    },
    /// 订单取消
    OrderCancelled {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 订单拒绝
    OrderRejected {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    /// 结算完成
    SettlementCompleted {
        settlement_id: String,
        timestamp: DateTime<Local>,
    },
    /// 组合更新
    PortfolioUpdated { timestamp: DateTime<Local> },
    /// 风控触发
    RiskTriggered {
        rule_id: String,
        detail: String,
        timestamp: DateTime<Local>,
    },
    /// 健康状态变化
    HealthChanged {
        component: String,
        healthy: bool,
        timestamp: DateTime<Local>,
    },
    /// 指标已收集
    MetricsCollected { timestamp: DateTime<Local> },
    /// 自定义事件（扩展预留）
    Custom {
        name: String,
        payload: serde_json::Value,
        timestamp: DateTime<Local>,
    },
}

impl SystemEvent {
    /// 事件英文名称
    pub fn event_name(&self) -> &'static str {
        match self {
            SystemEvent::MarketUpdated { .. } => "MarketUpdated",
            SystemEvent::OrderCreated { .. } => "OrderCreated",
            SystemEvent::OrderFilled { .. } => "OrderFilled",
            SystemEvent::OrderCancelled { .. } => "OrderCancelled",
            SystemEvent::OrderRejected { .. } => "OrderRejected",
            SystemEvent::SettlementCompleted { .. } => "SettlementCompleted",
            SystemEvent::PortfolioUpdated { .. } => "PortfolioUpdated",
            SystemEvent::RiskTriggered { .. } => "RiskTriggered",
            SystemEvent::HealthChanged { .. } => "HealthChanged",
            SystemEvent::MetricsCollected { .. } => "MetricsCollected",
            SystemEvent::Custom { .. } => "Custom",
        }
    }

    /// 事件中文名称
    pub fn event_name_zh(&self) -> &'static str {
        match self {
            SystemEvent::MarketUpdated { .. } => "市场数据更新",
            SystemEvent::OrderCreated { .. } => "订单创建",
            SystemEvent::OrderFilled { .. } => "订单成交",
            SystemEvent::OrderCancelled { .. } => "订单取消",
            SystemEvent::OrderRejected { .. } => "订单拒绝",
            SystemEvent::SettlementCompleted { .. } => "结算完成",
            SystemEvent::PortfolioUpdated { .. } => "组合更新",
            SystemEvent::RiskTriggered { .. } => "风控触发",
            SystemEvent::HealthChanged { .. } => "健康状态变化",
            SystemEvent::MetricsCollected { .. } => "指标收集",
            SystemEvent::Custom { .. } => "自定义事件",
        }
    }

    /// 事件时间戳
    pub fn timestamp(&self) -> DateTime<Local> {
        match self {
            SystemEvent::MarketUpdated { timestamp, .. }
            | SystemEvent::OrderCreated { timestamp, .. }
            | SystemEvent::OrderFilled { timestamp, .. }
            | SystemEvent::OrderCancelled { timestamp, .. }
            | SystemEvent::OrderRejected { timestamp, .. }
            | SystemEvent::SettlementCompleted { timestamp, .. }
            | SystemEvent::PortfolioUpdated { timestamp, .. }
            | SystemEvent::RiskTriggered { timestamp, .. }
            | SystemEvent::HealthChanged { timestamp, .. }
            | SystemEvent::MetricsCollected { timestamp, .. }
            | SystemEvent::Custom { timestamp, .. } => *timestamp,
        }
    }

    /// 事件详情摘要
    pub fn detail_zh(&self) -> String {
        match self {
            SystemEvent::MarketUpdated { market_id, .. } => {
                format!("市场 {} 数据已更新", market_id)
            }
            SystemEvent::OrderCreated {
                order_id,
                market_id,
                ..
            } => {
                format!("订单 {} 已创建（市场 {}）", order_id, market_id)
            }
            SystemEvent::OrderFilled {
                order_id,
                avg_price,
                ..
            } => {
                format!("订单 {} 已成交（均价 {:.4}）", order_id, avg_price)
            }
            SystemEvent::OrderCancelled {
                order_id, reason, ..
            } => {
                format!("订单 {} 已取消（原因: {}）", order_id, reason)
            }
            SystemEvent::OrderRejected {
                order_id, reason, ..
            } => {
                format!("订单 {} 已拒绝（原因: {}）", order_id, reason)
            }
            SystemEvent::SettlementCompleted { settlement_id, .. } => {
                format!("结算 {} 已完成", settlement_id)
            }
            SystemEvent::PortfolioUpdated { .. } => "投资组合已更新".to_string(),
            SystemEvent::RiskTriggered {
                rule_id, detail, ..
            } => {
                format!("风控规则 {} 触发: {}", rule_id, detail)
            }
            SystemEvent::HealthChanged {
                component, healthy, ..
            } => {
                format!(
                    "组件 {} 健康状态变为: {}",
                    component,
                    if *healthy { "健康" } else { "异常" }
                )
            }
            SystemEvent::MetricsCollected { .. } => "指标已收集".to_string(),
            SystemEvent::Custom { name, payload, .. } => {
                format!("自定义事件 {}: {}", name, payload)
            }
        }
    }
}

/// 事件订阅者 trait
///
/// 从 `pm-oms::events::Subscriber` 提取并泛化。
pub trait Subscriber: Send + Sync {
    /// 订阅者名称
    fn name(&self) -> &str;

    /// 接收事件
    fn on_event(&self, event: &SystemEvent) -> anyhow::Result<()>;

    /// 是否对该事件感兴趣（默认全部感兴趣）
    fn interested_in(&self, _event: &SystemEvent) -> bool {
        true
    }
}

/// 事件总线
///
/// 线程安全的发布-订阅系统。
/// 订阅者错误仅记录日志，不会传播到发布者。
///
/// # 示例
///
/// ```ignore
/// let bus = EventBus::new();
/// bus.subscribe(Box::new(MySubscriber));
/// bus.publish(SystemEvent::OrderCreated {
///     order_id: "1".into(),
///     market_id: "m1".into(),
///     timestamp: Local::now(),
/// });
/// ```
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Box<dyn Subscriber>>>>,
    published_count: Arc<Mutex<u64>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            published_count: Arc::new(Mutex::new(0)),
        }
    }

    /// 注册订阅者
    pub fn subscribe(&self, sub: Box<dyn Subscriber>) {
        let mut subs = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!("事件总线注册订阅者: {}", sub.name());
        subs.push(sub);
    }

    /// 发布事件（通知所有感兴趣的订阅者）
    pub fn publish(&self, event: SystemEvent) {
        let subs = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        let event_name = event.event_name_zh();
        tracing::debug!("发布事件: {}", event_name);

        for sub in subs.iter() {
            if sub.interested_in(&event) {
                if let Err(e) = sub.on_event(&event) {
                    // 失败隔离：订阅者错误不传播
                    tracing::warn!("订阅者 {} 处理事件 {} 失败: {}", sub.name(), event_name, e);
                }
            }
        }

        if let Ok(mut count) = self.published_count.lock() {
            *count += 1;
        }
    }

    /// 仅记录事件（不通知订阅者）
    pub fn record_only(&self, event: SystemEvent) {
        tracing::debug!("记录事件（不通知）: {}", event.event_name_zh());
        if let Ok(mut count) = self.published_count.lock() {
            *count += 1;
        }
    }

    /// 订阅者数量
    pub fn subscriber_count(&self) -> usize {
        let subs = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        subs.len()
    }

    /// 已发布事件数
    pub fn published_count(&self) -> u64 {
        *self
            .published_count
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// 事件 CSV 表头
pub const EVENT_CSV_HEADER: &[&str] = &["timestamp", "event_type", "event_name_zh", "details"];

/// 将事件转换为 CSV 行
pub fn event_to_csv_row(event: &SystemEvent) -> [String; 4] {
    [
        event.timestamp().format("%Y-%m-%d %H:%M:%S").to_string(),
        event.event_name().to_string(),
        event.event_name_zh().to_string(),
        event.detail_zh(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSubscriber {
        name: String,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl TestSubscriber {
        fn new(name: &str, events: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name: name.to_string(),
                events,
            }
        }
    }

    impl Subscriber for TestSubscriber {
        fn name(&self) -> &str {
            &self.name
        }

        fn on_event(&self, event: &SystemEvent) -> anyhow::Result<()> {
            let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
            events.push(event.event_name().to_string());
            Ok(())
        }
    }

    struct FailingSubscriber;

    impl Subscriber for FailingSubscriber {
        fn name(&self) -> &str {
            "failing"
        }

        fn on_event(&self, _event: &SystemEvent) -> anyhow::Result<()> {
            anyhow::bail!("模拟订阅者失败")
        }
    }

    #[test]
    fn event_bus_subscribe_and_publish() {
        let bus = EventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sub = TestSubscriber::new("test-sub", events.clone());
        bus.subscribe(Box::new(sub));
        assert_eq!(bus.subscriber_count(), 1);

        let event = SystemEvent::OrderCreated {
            order_id: "order-1".to_string(),
            market_id: "market-1".to_string(),
            timestamp: Local::now(),
        };
        bus.publish(event);
        assert_eq!(bus.published_count(), 1);

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0], "OrderCreated");
    }

    #[test]
    fn event_bus_failure_isolation() {
        let bus = EventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let good = TestSubscriber::new("good", events.clone());
        let bad = FailingSubscriber;

        bus.subscribe(Box::new(good));
        bus.subscribe(Box::new(bad));

        let event = SystemEvent::MarketUpdated {
            market_id: "m1".to_string(),
            timestamp: Local::now(),
        };
        // 不应 panic，好的订阅者仍应收到事件
        bus.publish(event);

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
    }

    #[test]
    fn record_only_does_not_notify() {
        let bus = EventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sub = TestSubscriber::new("test", events.clone());
        bus.subscribe(Box::new(sub));

        bus.record_only(SystemEvent::MetricsCollected {
            timestamp: Local::now(),
        });
        assert_eq!(bus.published_count(), 1);
        assert_eq!(events.lock().unwrap().len(), 0);
    }

    #[test]
    fn event_csv_row_format() {
        let event = SystemEvent::OrderFilled {
            order_id: "o1".to_string(),
            avg_price: 1.2345,
            timestamp: Local::now(),
        };
        let row = event_to_csv_row(&event);
        assert_eq!(row[1], "OrderFilled");
        assert_eq!(row[2], "订单成交");
        assert!(row[3].contains("o1"));
    }

    #[test]
    fn event_chinese_names() {
        assert_eq!(
            SystemEvent::MarketUpdated {
                market_id: "".into(),
                timestamp: Local::now()
            }
            .event_name_zh(),
            "市场数据更新"
        );
        assert_eq!(
            SystemEvent::RiskTriggered {
                rule_id: "".into(),
                detail: "".into(),
                timestamp: Local::now()
            }
            .event_name_zh(),
            "风控触发"
        );
    }

    #[test]
    fn event_detail_zh() {
        let event = SystemEvent::HealthChanged {
            component: "Gateway".to_string(),
            healthy: false,
            timestamp: Local::now(),
        };
        let detail = event.detail_zh();
        assert!(detail.contains("Gateway"));
        assert!(detail.contains("异常"));
    }

    #[test]
    fn event_bus_default() {
        let bus = EventBus::default();
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(bus.published_count(), 0);
    }
}
