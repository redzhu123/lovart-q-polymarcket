//! PmsEventSubscriber — PMS 事件订阅者（P2-05 第八节）。
//!
//! 实现 OMS Subscriber trait，监听 OMS 订单事件自动更新 PMS。
//!
//! 监听事件：OrderFilled → 开仓+扣款、OrderCancelled → 释放冻结、OrderRejected → 释放冻结。
//! PMS 不主动调用 OMS，采用事件驱动方式。

use pm_oms::events::{OrderEvent, Subscriber};
use std::sync::{Arc, Mutex};

/// PMS 事件处理器引用。
pub type PmsEventHandler = Arc<Mutex<dyn FnMut(&OrderEvent) -> anyhow::Result<()> + Send>>;

/// PMS 事件订阅者，实现 `pm_oms::events::Subscriber` trait。
pub struct PmsEventSubscriber {
    name: String,
    handler: PmsEventHandler,
}

impl PmsEventSubscriber {
    pub fn new(name: &str, handler: PmsEventHandler) -> Self {
        tracing::info!(subscriber_name = %name, "PMS 事件订阅者创建");
        Self {
            name: name.to_string(),
            handler,
        }
    }

    pub fn pms_portfolio(handler: PmsEventHandler) -> Self {
        Self::new("pms-portfolio", handler)
    }
}

impl Subscriber for PmsEventSubscriber {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        tracing::debug!(
            event = %event.event_name_zh(),
            order_id = %event.order_id(),
            "PMS 收到 OMS 事件"
        );
        match self.handler.lock() {
            Ok(mut handler) => {
                if let Err(e) = handler(event) {
                    tracing::warn!(
                        error = %e,
                        event = %event.event_name_zh(),
                        "PMS 事件处理失败（已隔离）"
                    );
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "PMS 事件 handler 锁获取失败");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn subscriber_receives_events() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let handler: PmsEventHandler = Arc::new(Mutex::new(move |_event: &OrderEvent| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));
        let sub = PmsEventSubscriber::pms_portfolio(handler);
        let ev = OrderEvent::OrderFilled {
            order_id: "OMS-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: Local::now(),
        };
        sub.on_event(&ev).unwrap();
        sub.on_event(&ev).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn handler_error_isolated() {
        let handler: PmsEventHandler = Arc::new(Mutex::new(move |_event: &OrderEvent| {
            Err(anyhow::anyhow!("模拟失败"))
        }));
        let sub = PmsEventSubscriber::pms_portfolio(handler);
        let ev = OrderEvent::OrderCreated {
            order_id: "OMS-001".into(),
            client_order_id: "C1".into(),
            market_id: "mkt-1".into(),
            timestamp: Local::now(),
        };
        assert!(sub.on_event(&ev).is_ok()); // 错误已隔离
    }
}
