//! ValuationEngine — 估值引擎（P2-05 第六节）。
//!
//! 统一估值：
//! - 持仓价值 / 投资组合价值
//! - 现金价值 / 总敞口
//! - 净资产价值 (NAV) / 总市值

use crate::domain::{Portfolio, Position, PositionStatus, ValuationReport};
use chrono::{DateTime, Local};
use std::cell::Cell;
use tracing;

/// 估值引擎。
pub struct ValuationEngine {
    /// 上次估值时间。
    last_valuation: Cell<Option<DateTime<Local>>>,
}

impl ValuationEngine {
    pub fn new() -> Self {
        tracing::info!("估值引擎初始化");
        Self {
            last_valuation: Cell::new(None),
        }
    }

    /// 计算估值报告。
    pub fn calculate(
        &self,
        positions: &[Position],
        portfolio: &Portfolio,
        now: DateTime<Local>,
    ) -> ValuationReport {
        // 持仓总价值
        let position_value: f64 = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .map(|p| p.market_value)
            .sum();

        // 现金价值
        let cash_value = portfolio.available_cash + portfolio.frozen_cash;

        // 投资组合总价值
        let portfolio_value = cash_value + position_value;

        // 总敞口 = 持仓价值
        let total_exposure = position_value;

        // NAV = 总资产 = 现金 + 持仓价值
        let nav = portfolio_value;

        // 总市值
        let market_value = nav;

        self.last_valuation.set(Some(now));

        tracing::debug!(
            position_value = %position_value,
            cash_value = %cash_value,
            portfolio_value = %portfolio_value,
            nav = %nav,
            "估值计算完成"
        );

        ValuationReport {
            position_value,
            portfolio_value,
            cash_value,
            total_exposure,
            nav,
            market_value,
            valued_at: now,
        }
    }

    /// 获取上次估值时间。
    pub fn last_valuation_time(&self) -> Option<DateTime<Local>> {
        self.last_valuation.get()
    }

    /// 中文打印估值报告。
    pub fn print_zh(&self, report: &ValuationReport) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  估值报告 (Valuation Report)");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  持仓价值      : {:.2} USDC", report.position_value);
        println!("  现金价值      : {:.2} USDC", report.cash_value);
        println!("  投资组合价值  : {:.2} USDC", report.portfolio_value);
        println!("  总敞口        : {:.2} USDC", report.total_exposure);
        println!("  NAV (净资产)  : {:.2} USDC", report.nav);
        println!("  总市值        : {:.2} USDC", report.market_value);
        println!(
            "  估值时间      : {}",
            report.valued_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!();
    }
}

impl Default for ValuationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction};
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_portfolio_valuation() {
        let engine = ValuationEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let report = engine.calculate(&[], &pf, now);
        assert!(approx(report.position_value, 0.0));
        assert!(approx(
            report.cash_value,
            pf.available_cash + pf.frozen_cash
        ));
    }

    #[test]
    fn with_positions_valuation() {
        let engine = ValuationEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let report = engine.calculate(&[pos], &pf, now);
        assert!(approx(report.position_value, 100.0));
        assert!(approx(report.nav, 10100.0)); // cash(10000) + position(100)
    }

    #[test]
    fn last_valuation_time_updated() {
        let engine = ValuationEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        assert!(engine.last_valuation_time().is_none());
        engine.calculate(&[], &pf, now);
        assert!(engine.last_valuation_time().is_some());
    }
}
