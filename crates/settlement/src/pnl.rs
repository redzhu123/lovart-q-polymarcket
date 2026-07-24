//! PnL Settlement（盈亏结算 — P2-06 第六节）。
//!
//! 统一盈亏计算，包括：
//! - Realized PnL（已实现盈亏）
//! - Unrealized PnL（未实现盈亏）
//! - ROI（投资回报率）
//! - Return（累计收益率）
//! - Cost Basis（成本基础）
//! - Average Entry Price / Average Exit Price（均价）
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};

use crate::types::PositionState;

// ============================================================================
// PnLReport — Settlement PnL 报告
// ============================================================================

/// Settlement PnL 报告。
#[derive(Debug, Clone)]
pub struct PnLReport {
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 总盈亏。
    pub total_pnl: f64,
    /// 投资回报率（total_pnl / initial_capital）。
    pub roi: f64,
    /// 成本基础（所有开仓成本之和）。
    pub cost_basis: f64,
    /// 加权平均入场价。
    pub avg_entry_price: f64,
    /// 加权平均出场价（仅已平仓）。
    pub avg_exit_price: f64,
    /// 盈利持仓数。
    pub winning_positions: usize,
    /// 亏损持仓数。
    pub losing_positions: usize,
    /// 已平仓持仓数。
    pub closed_count: usize,
    /// 未平仓持仓数。
    pub open_count: usize,
    /// 计算时间。
    pub calculated_at: DateTime<Local>,
}

impl PnLReport {
    /// 创建空报告。
    pub fn empty(now: DateTime<Local>) -> Self {
        Self {
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            total_pnl: 0.0,
            roi: 0.0,
            cost_basis: 0.0,
            avg_entry_price: 0.0,
            avg_exit_price: 0.0,
            winning_positions: 0,
            losing_positions: 0,
            closed_count: 0,
            open_count: 0,
            calculated_at: now,
        }
    }
}

// ============================================================================
// PnLEngine — Settlement 盈亏引擎
// ============================================================================

/// Settlement 盈亏引擎。
///
/// 从持仓状态计算盈亏。所有盈亏统一由此引擎计算。
#[derive(Debug, Clone)]
pub struct PnLEngine {
    /// 初始资金（用于 ROI 计算）。
    initial_capital: f64,
    /// 累计已实现盈亏。
    pub accumulated_realized_pnl: f64,
    /// 历史已平仓盈亏（用于累计）。
    historical_pnl: Vec<f64>,
}

impl PnLEngine {
    /// 创建新盈亏引擎。
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            accumulated_realized_pnl: 0.0,
            historical_pnl: Vec::new(),
        }
    }

    /// 记录一笔已实现盈亏。
    pub fn record_realized(&mut self, realized: f64) {
        self.accumulated_realized_pnl += realized;
        self.historical_pnl.push(realized);
        tracing::info!(
            realized = %realized,
            accumulated = %self.accumulated_realized_pnl,
            "已实现盈亏已记录"
        );
    }

    /// 基于当前持仓状态计算 PnL 报告。
    pub fn calculate(
        &self,
        open_positions: &[&PositionState],
        closed_positions: &[PositionState],
        now: DateTime<Local>,
    ) -> PnLReport {
        // 已实现盈亏 = 平仓盈亏 + 未平仓持仓的部分已实现盈亏
        let closed_realized: f64 = closed_positions.iter().map(|p| p.realized_pnl).sum::<f64>();
        let open_realized: f64 = open_positions.iter().map(|p| p.realized_pnl).sum::<f64>();
        let total_realized = closed_realized + open_realized + self.accumulated_realized_pnl;

        // 未实现盈亏 = 所有未平仓持仓的未实现盈亏
        let total_unrealized: f64 = open_positions.iter().map(|p| p.unrealized_pnl).sum::<f64>();

        let total_pnl = total_realized + total_unrealized;

        let roi = if self.initial_capital.abs() > f64::EPSILON {
            total_pnl / self.initial_capital
        } else {
            0.0
        };

        // 成本基础
        let cost_basis: f64 = open_positions.iter().map(|p| p.cost_basis).sum::<f64>()
            + closed_positions.iter().map(|p| p.cost_basis).sum::<f64>();

        // 加权平均入场价（按数量加权）
        let total_qty: f64 = open_positions.iter().map(|p| p.quantity).sum::<f64>()
            + closed_positions.iter().map(|p| p.quantity).sum::<f64>();
        let avg_entry = if total_qty > f64::EPSILON {
            let total_cost: f64 = open_positions.iter().map(|p| p.cost_basis).sum::<f64>()
                + closed_positions.iter().map(|p| p.cost_basis).sum::<f64>();
            total_cost / total_qty
        } else {
            0.0
        };

        // 加权平均出场价（仅已平仓）
        let closed_total_qty: f64 = closed_positions
            .iter()
            .map(|p| {
                // 用原始数量（从 order_ids 推算，简化：用成本/均价反推）
                if p.average_price > f64::EPSILON {
                    p.cost_basis / p.average_price
                } else {
                    0.0
                }
            })
            .sum::<f64>();

        let avg_exit = if closed_total_qty > f64::EPSILON {
            let realized_sum: f64 = closed_positions.iter().map(|p| p.realized_pnl).sum::<f64>();
            let cost_sum: f64 = closed_positions.iter().map(|p| p.cost_basis).sum::<f64>();
            (cost_sum + realized_sum) / closed_total_qty
        } else {
            0.0
        };

        // 盈亏分类
        let winning = self.historical_pnl.iter().filter(|&&p| p > 0.0).count();
        let losing = self.historical_pnl.iter().filter(|&&p| p < 0.0).count();

        PnLReport {
            realized_pnl: total_realized,
            unrealized_pnl: total_unrealized,
            total_pnl,
            roi,
            cost_basis,
            avg_entry_price: avg_entry,
            avg_exit_price: avg_exit,
            winning_positions: winning,
            losing_positions: losing,
            closed_count: closed_positions.len(),
            open_count: open_positions.len(),
            calculated_at: now,
        }
    }

    /// 累计回报率（ROI）。
    pub fn cumulative_roi(&self) -> f64 {
        if self.initial_capital.abs() > f64::EPSILON {
            self.accumulated_realized_pnl / self.initial_capital
        } else {
            0.0
        }
    }

    /// 历史盈亏序列（用于胜率、盈亏比等统计）。
    pub fn historical_pnl(&self) -> &[f64] {
        &self.historical_pnl
    }

    /// 盈利交易数。
    pub fn winning_trades(&self) -> usize {
        self.historical_pnl.iter().filter(|&&p| p > 0.0).count()
    }

    /// 亏损交易数。
    pub fn losing_trades(&self) -> usize {
        self.historical_pnl.iter().filter(|&&p| p < 0.0).count()
    }

    /// 胜率。
    pub fn win_rate(&self) -> f64 {
        let total = self.winning_trades() + self.losing_trades();
        if total > 0 {
            self.winning_trades() as f64 / total as f64
        } else {
            0.0
        }
    }

    /// 盈亏比（avg_profit / avg_loss）。
    pub fn profit_factor(&self) -> f64 {
        let profits: Vec<f64> = self
            .historical_pnl
            .iter()
            .filter(|&&p| p > 0.0)
            .copied()
            .collect();
        let losses: Vec<f64> = self
            .historical_pnl
            .iter()
            .filter(|&&p| p < 0.0)
            .copied()
            .collect();
        let avg_profit = if !profits.is_empty() {
            profits.iter().sum::<f64>() / profits.len() as f64
        } else {
            0.0
        };
        let avg_loss = if !losses.is_empty() {
            losses.iter().map(|&l| l.abs()).sum::<f64>() / losses.len() as f64
        } else {
            0.0
        };
        if avg_loss > f64::EPSILON {
            avg_profit / avg_loss
        } else {
            f64::INFINITY
        }
    }

    /// 打印盈亏报告（中文 CLI 输出）。
    pub fn print_zh(&self, report: &PnLReport) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  Settlement PnL 报告");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("── 盈亏 ──");
        println!("  已实现盈亏  : {:+.2} USDC", report.realized_pnl);
        println!("  未实现盈亏  : {:+.2} USDC", report.unrealized_pnl);
        println!("  总盈亏      : {:+.2} USDC", report.total_pnl);
        println!("  ROI         : {:+.2}%", report.roi * 100.0);
        println!();
        println!("── 成本 ──");
        println!("  成本基础    : {:.2} USDC", report.cost_basis);
        println!("  平均入场价  : {:.4}", report.avg_entry_price);
        println!("  平均出场价  : {:.4}", report.avg_exit_price);
        println!();
        println!("── 统计 ──");
        println!("  未平仓      : {} 个", report.open_count);
        println!("  已平仓      : {} 个", report.closed_count);
        println!("  胜率        : {:.1}%", self.win_rate() * 100.0);
        println!("  盈亏比      : {:.2}", self.profit_factor());
        println!("  盈利交易    : {} 笔", self.winning_trades());
        println!("  亏损交易    : {} 笔", self.losing_trades());
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_report_all_zeros() {
        let engine = PnLEngine::new(10000.0);
        let report = engine.calculate(&[], &[], Local::now());
        assert!(approx(report.realized_pnl, 0.0));
        assert!(approx(report.unrealized_pnl, 0.0));
        assert!(approx(report.total_pnl, 0.0));
        assert!(approx(report.roi, 0.0));
    }

    #[test]
    fn realized_pnl_accumulates() {
        let mut engine = PnLEngine::new(10000.0);
        engine.record_realized(50.0);
        engine.record_realized(-30.0);
        assert!(approx(engine.accumulated_realized_pnl, 20.0));
        assert_eq!(engine.winning_trades(), 1);
        assert_eq!(engine.losing_trades(), 1);
        assert!(approx(engine.win_rate(), 0.5));
    }

    #[test]
    fn roi_calculation() {
        let mut engine = PnLEngine::new(10000.0);
        engine.record_realized(500.0);
        assert!(approx(engine.cumulative_roi(), 0.05));
    }

    #[test]
    fn calculate_from_positions() {
        let now = Local::now();
        let engine = PnLEngine::new(10000.0);

        let open_pos = PositionState::open(
            "SPOS-001".into(),
            "mkt-btc".into(),
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            "T-001".into(),
            now,
        );

        let closed_pos = {
            let mut p = PositionState::open(
                "SPOS-002".into(),
                "mkt-eth".into(),
                Direction::Yes,
                Side::Buy,
                100.0,
                0.40,
                "OMS-002".into(),
                "T-002".into(),
                now,
            );
            p.reduce(100.0, 0.45, now);
            p
        };

        let report = engine.calculate(&[&open_pos], &[closed_pos], now);
        assert!(approx(report.realized_pnl, 5.0)); // 100 * (0.45 - 0.40)
        assert!(approx(report.unrealized_pnl, 0.0)); // open 未标记
        assert_eq!(report.open_count, 1);
        assert_eq!(report.closed_count, 1);
    }

    #[test]
    fn profit_factor_calculation() {
        let mut engine = PnLEngine::new(10000.0);
        engine.record_realized(100.0);
        engine.record_realized(50.0);
        engine.record_realized(-30.0);
        engine.record_realized(-20.0);
        // avg_profit = (100+50)/2 = 75, avg_loss = (30+20)/2 = 25
        // profit_factor = 75/25 = 3.0
        assert!(approx(engine.profit_factor(), 3.0));
    }

    #[test]
    fn profit_factor_no_losses() {
        let mut engine = PnLEngine::new(10000.0);
        engine.record_realized(100.0);
        assert!(engine.profit_factor().is_infinite());
    }

    #[test]
    fn profit_factor_no_trades() {
        let engine = PnLEngine::new(10000.0);
        assert!(engine.profit_factor().is_infinite()); // 无亏损 → INFINITY
    }

    #[test]
    fn print_zh_does_not_panic() {
        let engine = PnLEngine::new(10000.0);
        let report = engine.calculate(&[], &[], Local::now());
        engine.print_zh(&report);
    }
}
