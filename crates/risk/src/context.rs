//! RiskContext（V1.05 第二节）。
//!
//! 包含所有风险判断所需的信息：
//! - 组合（现金/总价值/已实现盈亏/未实现盈亏）
//! - 持仓（当前持仓列表）
//! - 待处理订单
//! - 市场数据
//! - 机会数据
//! - 策略建议
//! - 市场快照
//!
//! 所有风险判断统一使用 RiskContext。

use chrono::{DateTime, Local};
use pm_execution::ExecutionOrder;
use pm_opportunity::Opportunity;
use pm_portfolio::Position;

/// 风险上下文：聚合所有风险判断所需的运行时状态。
///
/// Risk Engine 的每个规则都接收 `&RiskContext`，从中提取所需信息。
/// 不包含可变引用 —— 规则只读，决策由 Engine 统一执行。
#[derive(Debug, Clone)]
pub struct RiskContext {
    // ---- 组合 ----
    /// 初始资金（USDC）。
    pub initial_capital: f64,
    /// 当前可用现金（USDC）。
    pub available_cash: f64,
    /// 当前总价值（USDC）。
    pub total_value: f64,
    /// 已实现盈亏累计（USDC）。
    pub realized_pnl: f64,
    /// 未实现盈亏（USDC）。
    pub unrealized_pnl: f64,
    /// 锁定现金（开仓成本占用，USDC）。
    pub locked_cash: f64,
    /// 当日已实现盈亏（USDC）。
    pub daily_realized_pnl: f64,

    // ---- 持仓 ----
    /// 当前开仓列表。
    pub open_positions: Vec<Position>,
    /// 已平仓列表。
    pub closed_positions: Vec<Position>,
    /// 当前开仓数。
    pub open_position_count: usize,

    // ---- 待处理订单 ----
    /// 当前待处理订单数。
    pub pending_order_count: usize,
    /// 当前待处理订单列表。
    pub pending_orders: Vec<ExecutionOrder>,

    // ---- 连续亏损 ----
    /// 连续亏损次数。
    pub consecutive_losses: usize,

    // ---- 最大回撤 ----
    /// 历史最大净值。
    pub peak_value: f64,
    /// 当前回撤比例（0.0~1.0）。
    pub current_drawdown: f64,

    // ---- 市场 ----
    /// 市场 ID（当前评估的机会所属市场）。
    pub market_id: String,
    /// 市场类别（如有）。
    pub category: Option<String>,
    /// 市场流动性。
    pub market_liquidity: f64,
    /// 市场成交量。
    pub market_volume: f64,

    // ---- 机会 ----
    /// 当前评估的机会（如有）。
    pub opportunity: Option<Opportunity>,

    // ---- 订单簿 ----
    /// 最优买价。
    pub best_bid: Option<f64>,
    /// 最优卖价。
    pub best_ask: Option<f64>,
    /// 买卖价差。
    pub spread: Option<f64>,
    /// 买盘深度。
    pub bid_depth: Option<f64>,
    /// 卖盘深度。
    pub ask_depth: Option<f64>,

    // ---- 策略建议 ----
    /// 建议方向（BUY/SELL）。
    pub suggested_side: Option<pm_core::Side>,
    /// 建议价格。
    pub suggested_price: f64,
    /// 建议数量。
    pub suggested_quantity: f64,
    /// 建议名义金额（USDC）。
    pub suggested_notional: f64,

    // ---- 暴露 ----
    /// YES 方向总暴露（USDC）。
    pub yes_exposure: f64,
    /// NO 方向总暴露（USDC）。
    pub no_exposure: f64,
    /// 当前市场已有暴露（USDC）。
    pub market_exposure: f64,
    /// 当前类别已有暴露（USDC）。
    pub category_exposure: f64,

    // ---- 时间 ----
    /// 当前时间。
    pub now: DateTime<Local>,
}

impl RiskContext {
    /// 创建最小 RiskContext（用于测试和简单场景）。
    pub fn minimal(initial_capital: f64, available_cash: f64, now: DateTime<Local>) -> Self {
        Self {
            initial_capital,
            available_cash,
            total_value: available_cash,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            locked_cash: 0.0,
            daily_realized_pnl: 0.0,
            open_positions: Vec::new(),
            closed_positions: Vec::new(),
            open_position_count: 0,
            pending_order_count: 0,
            pending_orders: Vec::new(),
            consecutive_losses: 0,
            peak_value: available_cash,
            current_drawdown: 0.0,
            market_id: String::new(),
            category: None,
            market_liquidity: 0.0,
            market_volume: 0.0,
            opportunity: None,
            best_bid: None,
            best_ask: None,
            spread: None,
            bid_depth: None,
            ask_depth: None,
            suggested_side: None,
            suggested_price: 0.0,
            suggested_quantity: 0.0,
            suggested_notional: 0.0,
            yes_exposure: 0.0,
            no_exposure: 0.0,
            market_exposure: 0.0,
            category_exposure: 0.0,
            now,
        }
    }

    /// 资金利用率（locked_cash / initial_capital）。
    pub fn capital_usage(&self) -> f64 {
        if self.initial_capital > 0.0 {
            self.locked_cash / self.initial_capital
        } else {
            0.0
        }
    }

    /// 现金比例（available_cash / initial_capital）。
    pub fn cash_ratio(&self) -> f64 {
        if self.initial_capital > 0.0 {
            self.available_cash / self.initial_capital
        } else {
            0.0
        }
    }

    /// 总暴露比例（(yes_exposure + no_exposure) / initial_capital）。
    pub fn total_exposure_ratio(&self) -> f64 {
        if self.initial_capital > 0.0 {
            (self.yes_exposure + self.no_exposure) / self.initial_capital
        } else {
            0.0
        }
    }

    /// 当前 ROI（total_value / initial_capital - 1）。
    pub fn roi(&self) -> f64 {
        if self.initial_capital > 0.0 {
            (self.total_value / self.initial_capital) - 1.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn minimal_context_has_expected_defaults() {
        let now = Local::now();
        let ctx = RiskContext::minimal(10000.0, 10000.0, now);
        assert!((ctx.initial_capital - 10000.0).abs() < 1e-9);
        assert!((ctx.available_cash - 10000.0).abs() < 1e-9);
        assert_eq!(ctx.open_position_count, 0);
        assert_eq!(ctx.pending_order_count, 0);
        assert_eq!(ctx.consecutive_losses, 0);
        assert!((ctx.current_drawdown).abs() < 1e-9);
    }

    #[test]
    fn capital_usage_calculation() {
        let now = Local::now();
        let mut ctx = RiskContext::minimal(10000.0, 8000.0, now);
        ctx.locked_cash = 2000.0;
        assert!((ctx.capital_usage() - 0.2).abs() < 1e-9);
        assert!((ctx.cash_ratio() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn roi_calculation() {
        let now = Local::now();
        let mut ctx = RiskContext::minimal(10000.0, 10000.0, now);
        ctx.total_value = 10500.0;
        assert!((ctx.roi() - 0.05).abs() < 1e-9);
    }

    #[test]
    fn exposure_ratio() {
        let now = Local::now();
        let mut ctx = RiskContext::minimal(10000.0, 10000.0, now);
        ctx.yes_exposure = 3000.0;
        ctx.no_exposure = 1000.0;
        assert!((ctx.total_exposure_ratio() - 0.4).abs() < 1e-9);
    }
}
