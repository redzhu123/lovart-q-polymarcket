//! Paper Trading 的简单订单模型。
//!
//! Simulation Only -- 所有订单均为模拟，绝不发送到任何真实交易所 / 钱包 / 链上。
//! 订单生命周期：Pending（创建）-> Filled（立即模拟成交）或 Cancelled（风控拒绝）。
//! 在立即成交模型下，Pending 只是瞬时状态：创建后立刻转入 Filled。
//!
//! `Side` 复用 [`pm_core::Side`]（与 Execution Simulator 共用）。

use chrono::{DateTime, Local};
use pm_core::Side;

/// 订单状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    /// 已创建，待成交（立即成交模型下为瞬时状态）。
    Pending,
    /// 已模拟成交。
    Filled,
    /// 已取消（风控拒绝等）。
    Cancelled,
}

impl OrderStatus {
    /// 用于 CSV 输出与控制台展示的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "Pending",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
        }
    }
}

/// 模拟订单。Simulation Only -- simulation_only 恒为 true。
#[derive(Debug, Clone)]
pub struct Order {
    pub order_id: String,
    pub question: String,
    pub side: Side,
    pub quantity: f64,
    pub price: f64,
    pub create_time: DateTime<Local>,
    pub fill_time: Option<DateTime<Local>>,
    pub status: OrderStatus,
    /// 永远为 true：标记本订单仅为模拟，不涉及任何真实资金 / 钱包 / 签名 / 上链。
    pub simulation_only: bool,
    /// 来源 Opportunity ID（用于链路追踪）。
    /// None 表示该订单为孤儿（缺少 Opportunity 来源）。
    pub source_opportunity_id: Option<String>,
}

impl Order {
    /// 创建一笔 BUY / SELL 订单（初始状态 Pending）。Simulation Only。
    ///
    /// `source_opportunity_id` 为订单的来源 Opportunity ID。
    /// 当为 None 时，记录错误日志：`[PaperTrading] 创建订单失败: 原因: 缺少 Opportunity 来源`。
    pub fn new(
        order_id: String,
        question: String,
        side: Side,
        quantity: f64,
        price: f64,
        now: DateTime<Local>,
        source_opportunity_id: Option<String>,
    ) -> Self {
        if source_opportunity_id.is_none() {
            tracing::error!(
                order_id = %order_id,
                question = %question,
                side = ?side,
                "[PaperTrading] 创建订单失败: 原因: 缺少 Opportunity 来源"
            );
        }
        Self {
            order_id,
            question,
            side,
            quantity,
            price,
            create_time: now,
            fill_time: None,
            status: OrderStatus::Pending,
            simulation_only: true,
            source_opportunity_id,
        }
    }

    /// 立即模拟成交：状态转 Filled，记录成交时间。Simulation Only。
    pub fn fill(&mut self, now: DateTime<Local>) {
        self.fill_time = Some(now);
        self.status = OrderStatus::Filled;
    }

    /// 取消（风控拒绝等）：状态转 Cancelled。
    pub fn cancel(&mut self) {
        self.status = OrderStatus::Cancelled;
    }

    /// 成交金额 = quantity * price。
    pub fn notional(&self) -> f64 {
        self.quantity * self.price
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_lifecycle() {
        let now = Local::now();
        let mut o = Order::new(
            "PO-1".into(),
            "Q".into(),
            Side::Buy,
            200.0,
            0.5,
            now,
            Some("OPP-test-001".into()),
        );
        assert_eq!(o.status, OrderStatus::Pending);
        assert!(o.fill_time.is_none());
        assert!(approx(o.notional(), 100.0));
        assert!(o.simulation_only);
        assert_eq!(o.source_opportunity_id, Some("OPP-test-001".to_string()));

        o.fill(now);
        assert_eq!(o.status, OrderStatus::Filled);
        assert!(o.fill_time.is_some());

        let mut o2 = Order::new(
            "PO-2".into(),
            "Q".into(),
            Side::Sell,
            10.0,
            0.4,
            now,
            Some("OPP-test-001".into()),
        );
        o2.cancel();
        assert_eq!(o2.status, OrderStatus::Cancelled);
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }
}
