//! Portfolio Sync（V1.06 第十一节）。
//!
//! Execution 成交后统一通知 Portfolio。
//! Portfolio 禁止主动修改订单状态 — Execution 是唯一来源。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};

use crate::order::Order;

// ============================================================================
// Fill Notification
// ============================================================================

/// 成交通知：Execution → Portfolio。
///
/// 当订单成交（完全或部分）时，Execution 发送此通知给 Portfolio，
/// Portfolio 据此更新持仓和现金。
#[derive(Debug, Clone)]
pub struct FillNotification {
    /// 订单 ID。
    pub order_id: String,
    /// 市场 / 问题标识。
    pub market_id: String,
    /// 问题描述。
    pub question: String,
    /// 买卖方向。
    pub side: pm_core::Side,
    /// 本次成交数量。
    pub filled_quantity: f64,
    /// 本次成交均价。
    pub fill_price: f64,
    /// 剩余未成交数量。
    pub remaining: f64,
    /// 是否完全成交。
    pub is_complete: bool,
    /// 成交时间。
    pub timestamp: DateTime<Local>,
}

impl FillNotification {
    /// 从 Order 创建成交通知。
    pub fn from_order(order: &Order, is_complete: bool) -> Self {
        Self {
            order_id: order.order_id.clone(),
            market_id: order.market_id.clone(),
            question: order.market_id.clone(), // market_id 兼作 question key
            side: order.side,
            filled_quantity: order.filled,
            fill_price: if order.avg_fill_price > 0.0 {
                order.avg_fill_price
            } else {
                order.price
            },
            remaining: order.remaining,
            is_complete,
            timestamp: order.update_time,
        }
    }

    /// 本次成交的现金变动（正数 = 组合获得现金，负数 = 组合支付现金）。
    pub fn cash_impact(&self) -> f64 {
        match self.side {
            pm_core::Side::Buy => -(self.filled_quantity * self.fill_price),
            pm_core::Side::Sell => self.filled_quantity * self.fill_price,
        }
    }
}

// ============================================================================
// Portfolio Sync Trait
// ============================================================================

/// Portfolio 同步接口。
///
/// Execution Engine 调用此 trait 通知 Portfolio 更新。
/// Portfolio crate 实现此 trait。
pub trait PortfolioSync: Send {
    /// 接收成交通知。
    fn on_fill(&mut self, notification: &FillNotification);

    /// 接收订单取消/过期通知（释放锁定资金）。
    fn on_cancel(&mut self, order: &Order);

    /// 接收订单拒绝通知。
    fn on_reject(&mut self, order: &Order);
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use chrono::Local;

    #[test]
    fn fill_notification_from_order() {
        let now = Local::now();
        let mut o = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, pm_core::Side::Buy,
            0.45, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        );
        o.filled = 50.0;
        o.remaining = 50.0;
        o.avg_fill_price = 0.452;

        let notif = FillNotification::from_order(&o, false);
        assert_eq!(notif.order_id, "EX-001");
        assert_eq!(notif.filled_quantity, 50.0);
        assert!((notif.fill_price - 0.452).abs() < 1e-9);
        assert!(!notif.is_complete);
        // BUY 支付 → 负数
        assert!((notif.cash_impact() + 22.6).abs() < 0.01);
    }

    #[test]
    fn sell_cash_impact_positive() {
        let now = Local::now();
        let mut o = Order::new(
            "EX-002".into(), "C2".into(), "mkt-1".into(), "mock".into(),
            Direction::No, pm_core::Side::Sell,
            0.50, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        );
        o.filled = 100.0;
        o.avg_fill_price = 0.51;
        let notif = FillNotification::from_order(&o, true);
        // SELL 获得 → 正数
        assert!((notif.cash_impact() - 51.0).abs() < 1e-9);
    }
}
