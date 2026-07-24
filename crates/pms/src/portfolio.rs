//! PortfolioManager — 投资组合管理器（P2-05 第三节）。
//!
//! 管理总资产/可用资金/冻结资金/持仓价值/总权益/未实现盈亏/已实现盈亏/收益率。
//! 支持多账户。

use crate::domain::{Portfolio, Position};
use chrono::{DateTime, Local};

/// 投资组合管理器。
pub struct PortfolioManager {
    portfolio: Portfolio,
}

impl PortfolioManager {
    pub fn new(portfolio: Portfolio) -> Self {
        tracing::info!(
            portfolio_id = %portfolio.portfolio_id,
            initial_capital = %portfolio.initial_capital,
            "投资组合管理器初始化"
        );
        Self { portfolio }
    }

    pub fn portfolio(&self) -> &Portfolio {
        &self.portfolio
    }
    pub fn portfolio_mut(&mut self) -> &mut Portfolio {
        &mut self.portfolio
    }

    /// 冻结 + 扣款（开仓成交）。
    pub fn freeze_and_debit(&mut self, amount: f64) {
        self.portfolio.freeze_cash(amount);
        self.portfolio.debit(amount);
        tracing::info!(
            amount = %amount,
            available = %self.portfolio.available_cash,
            frozen = %self.portfolio.frozen_cash,
            "组合资金扣款"
        );
    }

    /// 平仓入账。
    pub fn credit(&mut self, amount: f64) {
        self.portfolio.credit(amount);
        tracing::info!(
            amount = %amount,
            available = %self.portfolio.available_cash,
            "组合资金入账"
        );
    }

    /// 释放冻结资金。
    pub fn unfreeze(&mut self, amount: f64) {
        self.portfolio.unfreeze_cash(amount);
    }

    /// 重算组合指标。
    pub fn revalue(&mut self, positions: &[Position], now: DateTime<Local>) {
        self.portfolio.revalue(positions, now);
    }

    /// 中文打印投资组合。
    pub fn print_zh(&self) {
        let pf = &self.portfolio;
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  投资组合：{}", pf.name);
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  组合 ID      : {}", pf.portfolio_id);
        println!("  初始资金      : {:.2} USDC", pf.initial_capital);
        println!("  可用资金      : {:.2} USDC", pf.available_cash);
        println!("  冻结资金      : {:.2} USDC", pf.frozen_cash);
        println!("  持仓价值      : {:.2} USDC", pf.position_value);
        println!("  ───────────────────────────────────────────");
        println!("  总资产        : {:.2} USDC", pf.total_assets);
        println!("  总权益        : {:.2} USDC", pf.total_equity);
        println!("  已实现盈亏    : {:.2} USDC", pf.realized_pnl);
        println!("  未实现盈亏    : {:.2} USDC", pf.unrealized_pnl);
        println!("  总盈亏        : {:.2} USDC", pf.total_pnl);
        println!("  收益率        : {:.2}%", pf.roi * 100.0);
        println!("  关联账户      : {} 个", pf.account_ids.len());
        println!(
            "  更新时间      : {}",
            pf.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn freeze_and_debit_flow() {
        let pf = Portfolio::new("PF-001".into(), "测试".into(), 10000.0, Local::now());
        let mut mgr = PortfolioManager::new(pf);
        mgr.freeze_and_debit(500.0);
        assert!(approx(mgr.portfolio().available_cash, 9500.0));
    }

    #[test]
    fn credit_adds_cash() {
        let pf = Portfolio::new("PF-001".into(), "测试".into(), 10000.0, Local::now());
        let mut mgr = PortfolioManager::new(pf);
        mgr.credit(300.0);
        assert!(approx(mgr.portfolio().available_cash, 10300.0));
    }
}
