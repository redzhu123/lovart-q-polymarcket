//! Paper Trading 历史回放：把 `opportunities.csv` 走 PaperTradingEngine，产出 paper 报告。
//!
//! 与 `pm-backtest`（走 ShadowEngine）对应：本模块走组合层 PaperTradingEngine，
//! 带资金管理 + 持仓 + 风控。Simulation Only -- 入场价做 `entry_slippage` 策略假设，
//! 结果偏乐观，不代表真实收益。

use anyhow::Result;
use pm_models::{Config, ReplayOpportunity};
use pm_portfolio::RiskPolicy;
use pm_utils::{fmt_money, fmt_pct, fmt_pnl};

use crate::engine::PaperTradingEngine;

/// Paper 历史回放报告。
#[derive(Debug, Clone)]
pub struct PaperHistoryReport {
    pub total_opportunities: u64,
    pub opened: u64,
    pub rejected: u64,
    pub closed: u64,
    pub winners: u64,
    pub losers: u64,
    pub final_cash: f64,
    pub final_value: f64,
    pub total_pnl: f64,
    pub roi: f64,
    pub initial_capital: f64,
}

impl PaperHistoryReport {
    /// 终端打印报告。
    pub fn print(&self) {
        println!("======================================");
        println!();
        println!("Paper Trading History Report");
        println!();
        println!("======================================");
        println!();
        println!("Total Opportunities");
        println!();
        println!("{}", self.total_opportunities);
        println!();
        println!("Opened");
        println!();
        println!("{}", self.opened);
        println!();
        println!("Rejected (Risk)");
        println!();
        println!("{}", self.rejected);
        println!();
        println!("Closed");
        println!();
        println!("{}", self.closed);
        println!();
        println!("Winning Trades");
        println!();
        println!("{}", self.winners);
        println!();
        println!("Losing Trades");
        println!();
        println!("{}", self.losers);
        println!();
        println!("Final Cash");
        println!();
        println!("{} USDC", fmt_money(self.final_cash));
        println!();
        println!("Final Value");
        println!();
        println!("{} USDC", fmt_money(self.final_value));
        println!();
        println!("Total PnL");
        println!();
        println!("{} USDC", fmt_pnl(self.total_pnl));
        println!();
        println!("ROI");
        println!();
        println!("{}", fmt_pct(self.roi));
        println!();
        println!("======================================");
        println!();
        println!("Simulation Only -- 理论估算，非真实收益");
    }
}

/// 把历史机会走 PaperTradingEngine，返回聚合报告。
///
/// 策略假设：每个机会在最佳套利点附近开仓 YES（entry_yes = best_sum/2 * (1+slippage)），
/// 在机会结束时按 last_yes 平仓。被风控拒绝的记入 `rejected`。
pub fn paper_backtest(
    opps: &[ReplayOpportunity],
    capital: f64,
    policy: RiskPolicy,
    entry_slippage: f64,
) -> PaperHistoryReport {
    let mut eng = PaperTradingEngine::new(capital, policy);
    let mut opened: u64 = 0;
    let mut rejected: u64 = 0;
    let mut closed: u64 = 0;

    for opp in opps {
        let entry_yes = opp.best_sum / 2.0 * (1.0 + entry_slippage);
        let outcome = eng.open_position(&opp.question, entry_yes, opp.start_time);
        match outcome {
            crate::engine::OpenOutcome::Filled(_) => {
                opened += 1;
                if eng.close_position(&opp.question, opp.last_yes, opp.end_time).is_some() {
                    closed += 1;
                }
            }
            crate::engine::OpenOutcome::Rejected(_) => {
                rejected += 1;
            }
        }
        eng.revalue();
    }

    let pf = eng.portfolio();
    let (winners, losers) = pf
        .closed_positions
        .iter()
        .fold((0u64, 0u64), |(w, l), p| {
            if p.realized_pnl > 0.0 {
                (w + 1, l)
            } else if p.realized_pnl < 0.0 {
                (w, l + 1)
            } else {
                (w, l)
            }
        });

    PaperHistoryReport {
        total_opportunities: opps.len() as u64,
        opened,
        rejected,
        closed,
        winners,
        losers,
        final_cash: pf.cash,
        final_value: pf.total_value,
        total_pnl: pf.total_pnl,
        roi: pf.roi(),
        initial_capital: capital,
    }
}

/// `cargo run -- paper` 入口：加载 `opportunities.csv`，跑 paper 历史回放，打印报告。
pub fn run_paper_history(cfg: &Config) -> Result<()> {
    let opps = pm_storage::load_sorted_opportunities(&cfg.paths.opportunities_csv)?;
    if opps.is_empty() {
        println!();
        println!("No data found in {}", cfg.paths.opportunities_csv);
        println!("Run `cargo run -- scan` first to collect opportunities.");
        return Ok(());
    }

    println!();
    println!("Loaded {} opportunities", opps.len());
    println!();
    println!("Paper Trading History -- Simulation Only");
    println!();

    let policy = RiskPolicy {
        max_positions: cfg.portfolio.max_positions,
        max_position_size: cfg.portfolio.max_position_size,
        max_open_orders: cfg.execution.max_pending_orders,
        max_daily_loss: cfg.risk.max_daily_loss,
    };
    let report = paper_backtest(
        &opps,
        cfg.portfolio.initial_capital,
        policy,
        cfg.backtest.entry_slippage,
    );
    report.print();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn opp(question: &str, best_sum: f64, last_yes: f64) -> ReplayOpportunity {
        let now = Local::now();
        ReplayOpportunity {
            question: question.into(),
            start_time: now,
            end_time: now,
            duration_sec: 100,
            best_sum,
            scan_count: 2,
            last_yes,
            last_no: 1.0 - last_yes,
            volume: 0.0,
            liquidity: 0.0,
        }
    }

    fn policy() -> RiskPolicy {
        RiskPolicy {
            max_positions: 10,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 1_000_000.0, // 不让日亏干扰回放
        }
    }

    #[test]
    fn paper_backtest_produces_pnl() {
        // best_sum=0.90, last_yes=0.60：entry_yes=0.45*1.005=0.45225, exit=0.60 -> 盈利
        let opps = vec![opp("A", 0.90, 0.60)];
        let r = paper_backtest(&opps, 10000.0, policy(), 0.005);
        assert_eq!(r.total_opportunities, 1);
        assert_eq!(r.opened, 1);
        assert_eq!(r.closed, 1);
        assert_eq!(r.rejected, 0);
        assert!(r.total_pnl > 0.0);
        assert!(r.roi > 0.0);
    }

    #[test]
    fn paper_backtest_loses_when_exit_below_entry() {
        // best_sum=0.90 -> entry_yes≈0.452, exit last_yes=0.20 -> 亏损
        let opps = vec![opp("A", 0.90, 0.20)];
        let r = paper_backtest(&opps, 10000.0, policy(), 0.005);
        assert!(r.total_pnl < 0.0);
        assert_eq!(r.losers, 1);
        assert_eq!(r.winners, 0);
    }

    #[test]
    fn paper_backtest_counts_rejections() {
        // 资金 50 < 单笔成本 100 -> 每笔开仓都被风控拒绝（InsufficientCash）
        let opps = vec![opp("A", 0.90, 0.60), opp("B", 0.90, 0.60)];
        let r = paper_backtest(&opps, 50.0, policy(), 0.005);
        assert_eq!(r.opened, 0);
        assert_eq!(r.rejected, 2);
        assert_eq!(r.closed, 0);
        // 无交易 -> 资金不变
        assert!((r.final_cash - 50.0).abs() < 1e-6);
    }
}
