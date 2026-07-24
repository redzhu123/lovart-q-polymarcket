//! PnLEngine — 盈亏引擎（P2-05 第五节）。
//!
//! 统一计算：
//! - 已实现盈亏 / 未实现盈亏
//! - 当日盈亏
//! - 总盈亏 / 收益率
//! - 胜率 / 平均盈利 / 平均亏损 / 盈亏比

use crate::domain::{PnLReport, Portfolio, Position, PositionStatus};
use tracing;

/// 盈亏引擎。
pub struct PnLEngine {
    /// 初始资金（用于 ROI 计算）。
    initial_capital: f64,
    /// 当日盈亏基准（当日开始时 total_pnl）。
    day_start_pnl: f64,
}

impl PnLEngine {
    pub fn new(initial_capital: f64) -> Self {
        tracing::info!(
            initial_capital = %initial_capital,
            "PnL 引擎初始化"
        );
        Self {
            initial_capital,
            day_start_pnl: 0.0,
        }
    }

    /// 计算盈亏报告。
    pub fn calculate(&self, positions: &[Position], _portfolio: &Portfolio) -> PnLReport {
        // 已实现盈亏：所有持仓的 realized_pnl 之和
        let realized_pnl: f64 = positions.iter().map(|p| p.realized_pnl).sum();

        // 未实现盈亏：活跃持仓的 unrealized_pnl 之和
        let unrealized_pnl: f64 = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .map(|p| p.unrealized_pnl)
            .sum();

        let total_pnl = realized_pnl + unrealized_pnl;

        // 当日盈亏 = 当前总盈亏 - 当日开始时的总盈亏
        let daily_pnl = total_pnl - self.day_start_pnl;

        // ROI
        let roi = if self.initial_capital.abs() > f64::EPSILON {
            total_pnl / self.initial_capital
        } else {
            0.0
        };

        // 胜率统计（基于已平仓持仓）
        let closed: Vec<&Position> = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Closed)
            .collect();
        let total_trades = closed.len();
        let winning_trades = closed
            .iter()
            .filter(|p| p.realized_pnl > f64::EPSILON)
            .count();
        let losing_trades = closed
            .iter()
            .filter(|p| p.realized_pnl < -f64::EPSILON)
            .count();

        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        // 平均盈利
        let winning_sum: f64 = closed
            .iter()
            .filter(|p| p.realized_pnl > f64::EPSILON)
            .map(|p| p.realized_pnl)
            .sum();
        let avg_profit = if winning_trades > 0 {
            winning_sum / winning_trades as f64
        } else {
            0.0
        };

        // 平均亏损（正数表示亏损幅度）
        let losing_sum: f64 = closed
            .iter()
            .filter(|p| p.realized_pnl < -f64::EPSILON)
            .map(|p| -p.realized_pnl)
            .sum();
        let avg_loss = if losing_trades > 0 {
            losing_sum / losing_trades as f64
        } else {
            0.0
        };

        // 盈亏比
        let profit_factor = if avg_loss > f64::EPSILON {
            avg_profit / avg_loss
        } else if avg_profit > f64::EPSILON {
            f64::INFINITY
        } else {
            0.0
        };

        PnLReport {
            realized_pnl,
            unrealized_pnl,
            daily_pnl,
            total_pnl,
            roi,
            win_rate,
            avg_profit,
            avg_loss,
            profit_factor,
            total_trades,
            winning_trades,
            losing_trades,
        }
    }

    /// 重置当日基准（跨日调用）。
    pub fn reset_day(&mut self, current_total_pnl: f64) {
        tracing::info!(
            old_baseline = %self.day_start_pnl,
            new_baseline = %current_total_pnl,
            "PnL 引擎：重置当日基准"
        );
        self.day_start_pnl = current_total_pnl;
    }

    /// 更新初始资金。
    pub fn set_initial_capital(&mut self, capital: f64) {
        self.initial_capital = capital;
    }

    /// 中文打印盈亏报告。
    pub fn print_zh(&self, report: &PnLReport) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  盈亏报告 (PnL Report)");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  ── 盈亏 ──");
        println!("  已实现盈亏    : {:+.2} USDC", report.realized_pnl);
        println!("  未实现盈亏    : {:+.2} USDC", report.unrealized_pnl);
        println!("  当日盈亏      : {:+.2} USDC", report.daily_pnl);
        println!("  累计总盈亏    : {:+.2} USDC", report.total_pnl);
        println!("  收益率        : {:.2}%", report.roi * 100.0);
        println!();
        println!("  ── 交易统计 ──");
        println!("  总交易数      : {}", report.total_trades);
        println!("  盈利交易      : {}", report.winning_trades);
        println!("  亏损交易      : {}", report.losing_trades);
        println!("  胜率          : {:.1}%", report.win_rate * 100.0);
        println!("  平均盈利      : {:+.2} USDC", report.avg_profit);
        println!("  平均亏损      : {:.2} USDC", report.avg_loss);
        println!(
            "  盈亏比        : {}",
            if report.profit_factor.is_infinite() {
                "∞ (无亏损)".to_string()
            } else {
                format!("{:.2}", report.profit_factor)
            }
        );
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction, Position};
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_positions_zero_pnl() {
        let engine = PnLEngine::new(10000.0);
        let pf = Portfolio::default_portfolio(Local::now());
        let report = engine.calculate(&[], &pf);
        assert!(approx(report.total_pnl, 0.0));
        assert!(approx(report.win_rate, 0.0));
        assert_eq!(report.total_trades, 0);
    }

    #[test]
    fn realized_pnl_from_closed() {
        let engine = PnLEngine::new(10000.0);
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let mut pos = Position::open(
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
        pos.close(0.60, now); // realized = 200 * 0.10 = 20
        let report = engine.calculate(&[pos], &pf);
        assert!(approx(report.realized_pnl, 20.0));
        assert!(approx(report.total_pnl, 20.0));
        assert_eq!(report.total_trades, 1);
        assert_eq!(report.winning_trades, 1);
        assert!(approx(report.win_rate, 1.0));
    }

    #[test]
    fn win_rate_calculation() {
        let engine = PnLEngine::new(10000.0);
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();

        let mut p1 = Position::open(
            "POS-001".into(),
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

        let mut p2 = Position::open(
            "POS-002".into(),
            "mkt-b".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-002".into(),
            now,
        );
        p2.close(0.40, now); // -10

        let mut p3 = Position::open(
            "POS-003".into(),
            "mkt-c".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-003".into(),
            now,
        );
        p3.close(0.55, now); // +5

        let report = engine.calculate(&[p1, p2, p3], &pf);
        assert_eq!(report.total_trades, 3);
        assert_eq!(report.winning_trades, 2);
        assert_eq!(report.losing_trades, 1);
        assert!(approx(report.win_rate, 2.0 / 3.0));
        assert!(approx(report.total_pnl, 5.0));
        assert!(approx(report.avg_profit, 7.5)); // (10+5)/2
        assert!(approx(report.avg_loss, 10.0));
        assert!(approx(report.profit_factor, 0.75)); // 7.5/10
    }

    #[test]
    fn daily_pnl_tracks_intraday_change() {
        let mut engine = PnLEngine::new(10000.0);
        engine.reset_day(50.0); // 当日开始时总盈亏 50
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let mut pos = Position::open(
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
        pos.mark(0.60, now); // unrealized = +20
        let report = engine.calculate(&[pos], &pf);
        assert!(approx(report.daily_pnl, -30.0)); // 20 - 50
    }

    #[test]
    fn print_zh_does_not_panic() {
        let engine = PnLEngine::new(10000.0);
        let pf = Portfolio::default_portfolio(Local::now());
        let report = engine.calculate(&[], &pf);
        engine.print_zh(&report);
    }
}
