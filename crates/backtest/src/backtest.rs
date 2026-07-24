//! 完整回测：在 Replay 数据基础上重新执行 Shadow Strategy，重新计算理论收益。
//!
//! 不读取 shadow_trades.csv 旧结果，只读 opportunities.csv 原始机会数据，
//! 确保策略修改后可重复回测（改 `entry_slippage` 或 ShadowTrade::close 即可重跑）。
//!
//! Simulation Only：opportunities.csv 未保存逐轮扫描快照与开仓瞬时价格，
//! 回测对开仓价做策略假设（见 [`run_backtest`]），结果偏乐观，不代表真实收益。

use anyhow::Result;

use pm_models::{Config, FinishedOpportunity};
use pm_shadow::ShadowEngine;

use crate::report::BacktestReport;

/// backtest 模式入口：重放全部历史机会，对每个机会重新执行开/平仓并累计统计。
pub fn run_backtest(cfg: &Config) -> Result<()> {
    let opps = pm_storage::load_sorted_opportunities(&cfg.paths.opportunities_csv)?;
    if opps.is_empty() {
        println!();
        println!("No backtest data found in {}", cfg.paths.opportunities_csv);
        println!("Run `cargo run -- scan` first to collect opportunities.");
        return Ok(());
    }

    let slippage = cfg.backtest.entry_slippage;
    let strategy_name = cfg.backtest.strategy_name.as_str();

    println!();
    println!("Loaded {} opportunities", opps.len());
    println!("Strategy: {}", strategy_name);
    println!("Entry slippage: {:.2}% (Simulation Only)", slippage * 100.0);
    println!();
    println!("Backtesting...");
    println!();

    // 复用 ShadowEngine：保证回测使用与实盘扫描一致的 Shadow Strategy。
    // 不注入历史（load_history），确保统计全部来自本次重新计算。
    let mut shadow = ShadowEngine::new();
    let mut closed_trades: Vec<pm_shadow::ShadowTrade> = Vec::new();

    // 顺序重放每个机会：开仓 -> 平仓 -> 重新计算理论收益
    for opp in &opps {
        // 策略假设：入场 SUM = best_sum * (1 + slippage)，YES / NO 对称
        let entry_sum = opp.best_sum * (1.0 + slippage);
        let entry_yes = entry_sum / 2.0;
        let entry_no = entry_sum / 2.0;

        // 开仓（Simulation Only）；返回值这里不需要，仅触发引擎内部建仓
        shadow.open_trade(&opp.question, entry_yes, entry_no, opp.start_time);

        // 平仓：用最后扫描价，与 Shadow 平仓逻辑一致
        let finished = FinishedOpportunity {
            question: opp.question.clone(),
            start_time: opp.start_time,
            end_time: opp.end_time,
            duration_sec: opp.duration_sec,
            best_sum: opp.best_sum,
            scan_count: opp.scan_count,
            last_yes: opp.last_yes,
            last_no: opp.last_no,
            volume: opp.volume,
            liquidity: opp.liquidity,
        };
        if let Some(trade) = shadow.close_trade(&finished, opp.end_time) {
            closed_trades.push(trade);
        }
    }

    // 从已平仓交易列表聚合报告（含 median / longest 等 ShadowStats 未覆盖的指标）
    let report = BacktestReport::from_trades(&closed_trades, opps.len() as u64, strategy_name);
    report.print();

    // 写入 backtest_report.csv（失败只提示，不阻断）
    println!();
    match report.write_csv(&cfg.paths.backtest_report_csv) {
        Ok(_) => println!("Report saved to {}", cfg.paths.backtest_report_csv),
        Err(e) => println!("Report save failed: {:#}", e),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use pm_models::ReplayOpportunity;

    fn opp(question: &str, best_sum: f64, last_yes: f64, last_no: f64) -> ReplayOpportunity {
        let now = Local::now();
        ReplayOpportunity {
            question: question.into(),
            start_time: now,
            end_time: now,
            duration_sec: 100,
            best_sum,
            scan_count: 2,
            last_yes,
            last_no,
            volume: 0.0,
            liquidity: 0.0,
        }
    }

    #[test]
    fn backtest_logic_produces_mixed_pnl() {
        // 两个机会：一个套利回升（盈利），一个回落（亏损）
        let opps = vec![
            opp("A", 0.90, 0.55, 0.55), // entry_sum=0.9045, exit_sum=1.10 -> 盈利
            opp("B", 0.90, 0.40, 0.40), // exit_sum=0.80 -> 亏损
        ];
        let mut shadow = ShadowEngine::new();
        let mut trades = Vec::new();
        for opp in &opps {
            let entry_sum = opp.best_sum * (1.0 + 0.005);
            shadow.open_trade(
                &opp.question,
                entry_sum / 2.0,
                entry_sum / 2.0,
                opp.start_time,
            );
            let finished = FinishedOpportunity {
                question: opp.question.clone(),
                start_time: opp.start_time,
                end_time: opp.end_time,
                duration_sec: opp.duration_sec,
                best_sum: opp.best_sum,
                scan_count: opp.scan_count,
                last_yes: opp.last_yes,
                last_no: opp.last_no,
                volume: opp.volume,
                liquidity: opp.liquidity,
            };
            if let Some(t) = shadow.close_trade(&finished, opp.end_time) {
                trades.push(t);
            }
        }
        assert_eq!(trades.len(), 2);
        // 至少一盈一亏
        let pnls: Vec<f64> = trades
            .iter()
            .map(|t| t.estimated_pnl.unwrap_or(0.0))
            .collect();
        assert!(pnls.iter().any(|p| *p > 0.0));
        assert!(pnls.iter().any(|p| *p < 0.0));
    }
}
