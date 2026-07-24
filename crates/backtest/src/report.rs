//! 回测报告：汇总回测产生的影子交易，终端打印 + 追加写入 backtest_report.csv。
//!
//! Simulation Only -- 所有 ROI / PnL 均为模拟估算，不代表真实收益。

use std::path::Path;

use anyhow::{Context, Result};
use pm_shadow::ShadowTrade;
use pm_utils::{mean, median};

/// 报告 CSV 表头（列顺序固定，须与 [`BacktestCsvRecord`] 字段顺序一致）。
pub const HEADER: &[&str] = &[
    "run_time",
    "strategy",
    "trades",
    "wins",
    "losses",
    "win_rate",
    "avg_roi",
    "best_roi",
    "worst_roi",
    "avg_duration",
];

/// 回测报告。
#[derive(Debug, Clone)]
pub struct BacktestReport {
    pub run_time: String,
    pub strategy: String,
    pub total_opportunities: u64,
    pub total_trades: u64,
    pub winners: u64,
    pub losers: u64,
    pub win_rate: f64,
    pub avg_roi: f64,
    pub median_roi: f64,
    pub best_roi: f64,
    pub worst_roi: f64,
    pub avg_duration_sec: i64,
    pub longest_duration_sec: i64,
}

impl BacktestReport {
    /// 从已平仓交易列表聚合统计。
    /// winners / losers 沿用 V0.6 口径：PnL > 0 计胜，PnL < 0 计负，PnL == 0 不计入。
    pub fn from_trades(trades: &[ShadowTrade], total_opportunities: u64, strategy: &str) -> Self {
        let total_trades = trades.len() as u64;
        let mut winners: u64 = 0;
        let mut losers: u64 = 0;
        let mut rois: Vec<f64> = Vec::with_capacity(trades.len());
        let mut durs: Vec<i64> = Vec::with_capacity(trades.len());

        for t in trades {
            let pnl = t.estimated_pnl.unwrap_or(0.0);
            let roi = t.estimated_roi.unwrap_or(0.0);
            let dur = t.duration_sec.unwrap_or(0);
            if pnl > 0.0 {
                winners += 1;
            } else if pnl < 0.0 {
                losers += 1;
            }
            if roi.is_finite() {
                rois.push(roi);
            }
            durs.push(dur);
        }

        let win_rate = pm_utils::ratio(winners, total_trades);
        let avg_roi = mean(&rois);
        let median_roi = median(&rois);
        // 空 rois 时 best/worst 兜底为 0（fold 的初值 NEG_INFINITY / INFINITY 不可展示）
        let best_roi = rois.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let worst_roi = rois.iter().copied().fold(f64::INFINITY, f64::min);
        let best_roi = if rois.is_empty() { 0.0 } else { best_roi };
        let worst_roi = if rois.is_empty() { 0.0 } else { worst_roi };
        let avg_duration_sec = if durs.is_empty() {
            0
        } else {
            durs.iter().sum::<i64>() / durs.len() as i64
        };
        let longest_duration_sec = durs.iter().copied().max().unwrap_or(0);

        Self {
            run_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            strategy: strategy.to_string(),
            total_opportunities,
            total_trades,
            winners,
            losers,
            win_rate,
            avg_roi,
            median_roi,
            best_roi,
            worst_roi,
            avg_duration_sec,
            longest_duration_sec,
        }
    }

    /// 终端打印报告。
    pub fn print(&self) {
        println!("======================================");
        println!();
        println!("Backtesting Report");
        println!();
        println!("======================================");
        println!();
        println!("Total Opportunities");
        println!();
        println!("{}", self.total_opportunities);
        println!();
        println!("Total Shadow Trades");
        println!();
        println!("{}", self.total_trades);
        println!();
        println!("Winning Trades");
        println!();
        println!("{}", self.winners);
        println!();
        println!("Losing Trades");
        println!();
        println!("{}", self.losers);
        println!();
        println!("Win Rate");
        println!();
        println!("{:.2}%", self.win_rate * 100.0);
        println!();
        println!("Average ROI");
        println!();
        println!("{:.2}%", self.avg_roi * 100.0);
        println!();
        println!("Median ROI");
        println!();
        println!("{:.2}%", self.median_roi * 100.0);
        println!();
        println!("Best Trade");
        println!();
        println!("{:.2}%", self.best_roi * 100.0);
        println!();
        println!("Worst Trade");
        println!();
        println!("{:.2}%", self.worst_roi * 100.0);
        println!();
        println!("Average Duration");
        println!();
        println!("{} sec", self.avg_duration_sec);
        println!();
        println!("Longest Duration");
        println!();
        println!("{} sec", self.longest_duration_sec);
        println!();
        println!("======================================");
        println!();
        println!("Simulation Only -- 理论估算，非真实收益");
    }

    /// 追加写入 backtest_report.csv（不存在则建表头）。返回写入行数。
    pub fn write_csv(&self, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref();
        let need_header = !path.exists();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("打开 backtest_report.csv 失败")?;
        // has_headers(false)：追加模式，由 need_header 手动写一次表头
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);
        if need_header {
            wtr.write_record(HEADER).context("写表头失败")?;
        }
        // 数值字段格式化为定宽字符串，避免 CSV 里出现超长浮点
        let record = BacktestCsvRecord {
            run_time: self.run_time.clone(),
            strategy: self.strategy.clone(),
            trades: self.total_trades,
            wins: self.winners,
            losses: self.losers,
            win_rate: format!("{:.4}", self.win_rate),
            avg_roi: format!("{:.6}", self.avg_roi),
            best_roi: format!("{:.6}", self.best_roi),
            worst_roi: format!("{:.6}", self.worst_roi),
            avg_duration: self.avg_duration_sec,
        };
        wtr.serialize(&record).context("写报告行失败")?;
        wtr.flush().context("flush 报告失败")?;
        Ok(1)
    }
}

/// 报告 CSV 单行（字段顺序须与 [`HEADER`] 对齐）。
#[derive(Debug, serde::Serialize)]
struct BacktestCsvRecord {
    run_time: String,
    strategy: String,
    trades: u64,
    wins: u64,
    losses: u64,
    win_rate: String,
    avg_roi: String,
    best_roi: String,
    worst_roi: String,
    avg_duration: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use pm_shadow::ShadowTrade;

    fn closed_trade(yes_in: f64, no_in: f64, yes_out: f64, no_out: f64) -> ShadowTrade {
        let now = Local::now();
        let mut t = ShadowTrade::open("ST-1".into(), "Q".into(), now, yes_in, no_in);
        t.close(yes_out, no_out, now);
        t
    }

    #[test]
    fn from_trades_aggregates() {
        let trades = vec![
            closed_trade(0.40, 0.50, 0.45, 0.55), // 盈利
            closed_trade(0.40, 0.50, 0.35, 0.45), // 亏损
        ];
        let r = BacktestReport::from_trades(&trades, 2, "Test");
        assert_eq!(r.total_trades, 2);
        assert_eq!(r.total_opportunities, 2);
        assert_eq!(r.winners, 1);
        assert_eq!(r.losers, 1);
        assert!((r.win_rate - 0.5).abs() < 1e-9);
        assert_eq!(r.strategy, "Test");
    }

    #[test]
    fn from_empty_trades() {
        let r = BacktestReport::from_trades(&[], 0, "Empty");
        assert_eq!(r.total_trades, 0);
        assert_eq!(r.win_rate, 0.0);
        assert_eq!(r.best_roi, 0.0);
        assert_eq!(r.worst_roi, 0.0);
    }
}
