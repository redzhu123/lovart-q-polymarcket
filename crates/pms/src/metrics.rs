//! MetricsCalculator — PMS 指标计算器（P2-05 第十一节）。
//!
//! 计算 NAV、盈利率、累计收益率、平均持仓时间等统计指标。

use crate::domain::{PmsMetrics, Portfolio, Position, PositionStatus};
use chrono::{DateTime, Local};
use tracing;

/// 指标计算器。
pub struct MetricsCalculator {
    // Reserve for future configuration.
}

impl MetricsCalculator {
    pub fn new() -> Self {
        tracing::info!("PMS 指标计算器初始化");
        Self {}
    }

    /// 计算 PMS 统计指标。
    pub fn calculate(
        &self,
        positions: &[Position],
        portfolio: &Portfolio,
        now: DateTime<Local>,
    ) -> PmsMetrics {
        let closed: Vec<&Position> = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Closed)
            .collect();

        let open_count = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .count();

        let total_closed_trades = closed.len();
        let winning_trades = closed
            .iter()
            .filter(|p| p.realized_pnl > f64::EPSILON)
            .count();

        let win_rate = if total_closed_trades > 0 {
            winning_trades as f64 / total_closed_trades as f64
        } else {
            0.0
        };

        let total_pnl: f64 = closed.iter().map(|p| p.realized_pnl).sum();
        let avg_profit_per_trade = if total_closed_trades > 0 {
            total_pnl / total_closed_trades as f64
        } else {
            0.0
        };

        let return_rate = if portfolio.initial_capital.abs() > f64::EPSILON {
            total_pnl / portfolio.initial_capital
        } else {
            0.0
        };

        // 平均持仓时间
        let avg_holding_time_secs = if !closed.is_empty() {
            closed
                .iter()
                .filter_map(|p| {
                    p.closed_at
                        .map(|closed_at| (closed_at - p.created_at).num_seconds() as f64)
                })
                .sum::<f64>()
                / closed.len() as f64
        } else {
            0.0
        };

        PmsMetrics {
            nav: portfolio.total_assets,
            position_count: open_count,
            win_rate,
            max_drawdown: None, // 预留
            return_rate,
            avg_holding_time_secs,
            avg_profit_per_trade,
            total_closed_trades,
            generated_at: now,
        }
    }
}

impl Default for MetricsCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction};
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_positions_produces_default_metrics() {
        let calc = MetricsCalculator::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let m = calc.calculate(&[], &pf, now);
        assert_eq!(m.position_count, 0);
        assert_eq!(m.total_closed_trades, 0);
        assert!(approx(m.win_rate, 0.0));
    }

    #[test]
    fn with_closed_positions_calculates_metrics() {
        let calc = MetricsCalculator::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();

        let mut p1 = Position::open(
            "P-001".into(),
            "mkt-a".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        p1.close(0.60, now); // +10

        let m = calc.calculate(&[p1], &pf, now);
        assert_eq!(m.total_closed_trades, 1);
        assert!(approx(m.win_rate, 1.0));
        assert!(approx(m.avg_profit_per_trade, 10.0));
    }
}
