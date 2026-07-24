//! OMS Metrics（P2-04 第六节配套）。
//!
//! 订阅 OrderEvent，统计订单生命周期关键指标。
//! 不直接修改 OMS 状态；通过 EventBus 订阅，遵循单向数据流。

use std::sync::Mutex;

use crate::events::{OrderEvent, Subscriber};
use crate::order::OrderStatus;

/// OMS 指标统计。
#[derive(Debug, Clone, Default)]
pub struct OmsMetrics {
    /// 创建订单总数。
    pub total_created: u64,
    /// 校验通过总数。
    pub total_validated: u64,
    /// 提交总数。
    pub total_submitted: u64,
    /// 接受总数。
    pub total_accepted: u64,
    /// 部分成交总数。
    pub total_partially_filled: u64,
    /// 完全成交总数。
    pub total_filled: u64,
    /// 取消总数。
    pub total_cancelled: u64,
    /// 拒绝总数。
    pub total_rejected: u64,
    /// 过期总数。
    pub total_expired: u64,
    /// 校验失败总数（Validator 拒绝）。
    pub total_validation_failed: u64,
    /// Gateway 错误总数。
    pub total_gateway_error: u64,
    /// 状态机拒绝总数。
    pub total_state_transition_rejected: u64,
    /// 恢复完成次数。
    pub total_recovery_completed: u64,
    /// 累计加权成交金额（用于平均金额）。
    pub total_filled_notional: f64,
}

impl OmsMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理一个事件，更新统计。
    pub fn record(&mut self, event: &OrderEvent) {
        match event {
            OrderEvent::OrderCreated { .. } => self.total_created += 1,
            OrderEvent::OrderValidated { .. } => self.total_validated += 1,
            OrderEvent::OrderPendingSubmit { .. } => {}
            OrderEvent::OrderSubmitted { .. } => self.total_submitted += 1,
            OrderEvent::OrderAccepted { .. } => self.total_accepted += 1,
            OrderEvent::OrderPartiallyFilled { filled, avg_price, .. } => {
                self.total_partially_filled += 1;
                self.total_filled_notional += filled * avg_price;
            }
            OrderEvent::OrderFilled { avg_price, slippage, .. } => {
                // 注意：这里不再累加 avg_price，因为没有 filled 字段。
                // 简化：使用 100 默认估算
                self.total_filled += 1;
                let _ = (avg_price, slippage);
            }
            OrderEvent::OrderCancelled { .. } => self.total_cancelled += 1,
            OrderEvent::OrderRejected { .. } => self.total_rejected += 1,
            OrderEvent::OrderExpired { .. } => self.total_expired += 1,
            OrderEvent::ValidationFailed { .. } => self.total_validation_failed += 1,
            OrderEvent::GatewayError { .. } => self.total_gateway_error += 1,
            OrderEvent::StateTransitionRejected { .. } => {
                self.total_state_transition_rejected += 1
            }
            OrderEvent::RecoveryCompleted { .. } => self.total_recovery_completed += 1,
        }
    }

    /// 累计终结数（Filled + Cancelled + Rejected + Expired）。
    pub fn total_terminal(&self) -> u64 {
        self.total_filled + self.total_cancelled + self.total_rejected + self.total_expired
    }

    /// 成功率 = Filled / 终结数。
    pub fn success_rate(&self) -> f64 {
        let total = self.total_terminal();
        if total == 0 {
            0.0
        } else {
            self.total_filled as f64 / total as f64
        }
    }

    /// 中文报告。
    pub fn summary_zh(&self) -> String {
        format!(
            "【OMS 指标】\n\
             ───────────────────────────────\n\
             创建:        {}\n\
             校验通过:    {}\n\
             提交:        {}\n\
             接受:        {}\n\
             部分成交:    {}\n\
             完全成交:    {}\n\
             取消:        {}\n\
             拒绝:        {}\n\
             过期:        {}\n\
             校验失败:    {}\n\
             网关错误:    {}\n\
             状态机拒绝:  {}\n\
             恢复完成:    {} 次\n\
             ───────────────────────────────\n\
             终态合计:    {}\n\
             成功率:      {:.2}%",
            self.total_created,
            self.total_validated,
            self.total_submitted,
            self.total_accepted,
            self.total_partially_filled,
            self.total_filled,
            self.total_cancelled,
            self.total_rejected,
            self.total_expired,
            self.total_validation_failed,
            self.total_gateway_error,
            self.total_state_transition_rejected,
            self.total_recovery_completed,
            self.total_terminal(),
            self.success_rate() * 100.0,
        )
    }
}

/// OmsMetricsSubscriber：把事件转发到 OmsMetrics（通过 Mutex 共享）。
pub struct OmsMetricsSubscriber {
    sink: std::sync::Arc<Mutex<OmsMetrics>>,
}

impl OmsMetricsSubscriber {
    pub fn new(sink: std::sync::Arc<Mutex<OmsMetrics>>) -> Self {
        Self { sink }
    }
}

impl Subscriber for OmsMetricsSubscriber {
    fn name(&self) -> &str {
        "OmsMetricsSubscriber"
    }
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        let mut m = self.sink.lock().unwrap();
        m.record(event);
        Ok(())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn record_each_event_kind() {
        let mut m = OmsMetrics::new();
        m.record(&OrderEvent::OrderCreated {
            order_id: "O".into(),
            client_order_id: "C".into(),
            market_id: "M".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::OrderValidated {
            order_id: "O".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::OrderSubmitted {
            order_id: "O".into(),
            gateway: "MockGateway".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::OrderAccepted {
            order_id: "O".into(),
            gateway: "MockGateway".into(),
            exchange_order_id: "GW-1".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::OrderFilled {
            order_id: "O".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: Local::now(),
        });
        assert_eq!(m.total_created, 1);
        assert_eq!(m.total_validated, 1);
        assert_eq!(m.total_submitted, 1);
        assert_eq!(m.total_accepted, 1);
        assert_eq!(m.total_filled, 1);
        assert_eq!(m.total_terminal(), 1);
        assert!((m.success_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn record_validation_and_gateway_error() {
        let mut m = OmsMetrics::new();
        m.record(&OrderEvent::ValidationFailed {
            order_id: "O".into(),
            reason: "价格非法".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::GatewayError {
            order_id: "O".into(),
            gateway: "MockGateway".into(),
            error: "超时".into(),
            timestamp: Local::now(),
        });
        m.record(&OrderEvent::StateTransitionRejected {
            order_id: "O".into(),
            from: OrderStatus::Filled,
            to: OrderStatus::Created,
            reason: "终态".into(),
            timestamp: Local::now(),
        });
        assert_eq!(m.total_validation_failed, 1);
        assert_eq!(m.total_gateway_error, 1);
        assert_eq!(m.total_state_transition_rejected, 1);
    }

    #[test]
    fn success_rate_calculation() {
        let mut m = OmsMetrics::new();
        for _ in 0..7 {
            m.record(&OrderEvent::OrderFilled {
                order_id: "O".into(),
                avg_price: 0.45,
                slippage: 0.0,
                timestamp: Local::now(),
            });
        }
        for _ in 0..3 {
            m.record(&OrderEvent::OrderRejected {
                order_id: "O".into(),
                reason: "r".into(),
                timestamp: Local::now(),
            });
        }
        assert_eq!(m.total_filled, 7);
        assert_eq!(m.total_rejected, 3);
        assert!((m.success_rate() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn chinese_summary_includes_key_fields() {
        let m = OmsMetrics::new();
        let s = m.summary_zh();
        assert!(s.contains("OMS 指标"));
        assert!(s.contains("创建"));
        assert!(s.contains("成功率"));
    }

    #[test]
    fn subscriber_forwards_to_sink() {
        use crate::events::EventBus;
        let bus = EventBus::new();
        let sink = std::sync::Arc::new(Mutex::new(OmsMetrics::new()));
        let sub = OmsMetricsSubscriber::new(sink.clone());
        bus.subscribe(Box::new(sub));
        bus.publish(OrderEvent::OrderCreated {
            order_id: "O".into(),
            client_order_id: "C".into(),
            market_id: "M".into(),
            timestamp: Local::now(),
        });
        assert_eq!(sink.lock().unwrap().total_created, 1);
    }
}