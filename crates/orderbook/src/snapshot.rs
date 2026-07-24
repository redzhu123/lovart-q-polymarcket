//! 订单簿快照（V1.03 第七节）。
//!
//! 每轮扫描保存一份完整的订单簿快照：
//! - 市场标识 / 最优买卖价 / 价差
//! - 卖盘和买盘深度（Top1 / Top5 / Top10）
//! - 流动性（买盘 / 卖盘 / 总流动性 / 失衡度）
//! - 快照时间
//!
//! 追加写入 CSV，便于后续 Replay 与历史分析。

use std::fs::OpenOptions;
use std::io::Write;

use anyhow::Result;
use chrono::{DateTime, Local};

use crate::depth::DepthAnalyzer;
use crate::liquidity::LiquidityAnalyzer;
use crate::spread::SpreadAnalyzer;

/// 单轮订单簿快照（V1.03 第七节）。
///
/// 包含一个市场在某一时刻的完整订单簿状态：
/// 价格、价差、深度、流动性、时间戳。
#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    /// 快照时间（本地时间，"YYYY-MM-DD HH:MM:SS"）。
    pub timestamp: String,
    /// 市场标识。
    pub market_id: String,
    /// 数据来源 Provider。
    pub provider: String,
    /// 最优买价。
    pub best_bid: Option<f64>,
    /// 最优卖价。
    pub best_ask: Option<f64>,
    /// 价差。
    pub spread: Option<f64>,
    /// 中间价。
    pub mid_price: Option<f64>,
    /// 相对价差百分比。
    pub spread_pct: Option<f64>,
    /// 买盘 Top1 深度。
    pub bid_top1: f64,
    /// 买盘 Top5 深度。
    pub bid_top5: f64,
    /// 买盘 Top10 深度。
    pub bid_top10: f64,
    /// 卖盘 Top1 深度。
    pub ask_top1: f64,
    /// 卖盘 Top5 深度。
    pub ask_top5: f64,
    /// 卖盘 Top10 深度。
    pub ask_top10: f64,
    /// 买盘流动性。
    pub bid_liquidity: f64,
    /// 卖盘流动性。
    pub ask_liquidity: f64,
    /// 总流动性。
    pub total_liquidity: f64,
    /// 买卖失衡度。
    pub imbalance: f64,
}

/// CSV 表头。
const SNAPSHOT_HEADER: &str = "\
timestamp,market_id,provider,\
best_bid,best_ask,spread,mid_price,spread_pct,\
bid_top1,bid_top5,bid_top10,\
ask_top1,ask_top5,ask_top10,\
bid_liquidity,ask_liquidity,total_liquidity,imbalance";

impl OrderBookSnapshot {
    /// 从订单簿构造快照（含价差、深度、流动性分析）。
    pub fn from_orderbook(orderbook: &pm_models::OrderBook, now: DateTime<Local>) -> Self {
        let spread_report = SpreadAnalyzer::analyze(orderbook);
        let depth_report = DepthAnalyzer::analyze(orderbook);
        let liquidity_report = LiquidityAnalyzer::analyze(orderbook);

        Self {
            timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            market_id: orderbook.market_id.clone(),
            provider: orderbook.provider.clone(),
            best_bid: orderbook.best_bid,
            best_ask: orderbook.best_ask,
            spread: orderbook.spread,
            mid_price: spread_report.mid_price,
            spread_pct: spread_report.spread_pct,
            bid_top1: depth_report.bid_top1,
            bid_top5: depth_report.bid_top5,
            bid_top10: depth_report.bid_top10,
            ask_top1: depth_report.ask_top1,
            ask_top5: depth_report.ask_top5,
            ask_top10: depth_report.ask_top10,
            bid_liquidity: liquidity_report.bid_liquidity,
            ask_liquidity: liquidity_report.ask_liquidity,
            total_liquidity: liquidity_report.total_liquidity,
            imbalance: liquidity_report.imbalance,
        }
    }

    /// 批量从订单簿列表生成快照。
    pub fn from_orderbooks(orderbooks: &[pm_models::OrderBook], now: DateTime<Local>) -> Vec<Self> {
        orderbooks
            .iter()
            .map(|ob| Self::from_orderbook(ob, now))
            .collect()
    }

    /// 追加写入 CSV（文件不存在时先写表头）。
    pub fn save_to_csv(snapshots: &[Self], path: &str) -> Result<()> {
        if snapshots.is_empty() {
            return Ok(());
        }

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        // 文件为空时写表头
        let meta = std::fs::metadata(path)?;
        if meta.len() == 0 {
            writeln!(file, "{}", SNAPSHOT_HEADER)?;
        }

        for s in snapshots {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                s.timestamp,
                csv_escape(&s.market_id),
                csv_escape(&s.provider),
                fmt_opt(s.best_bid),
                fmt_opt(s.best_ask),
                fmt_opt(s.spread),
                fmt_opt(s.mid_price),
                fmt_opt(s.spread_pct),
                s.bid_top1,
                s.bid_top5,
                s.bid_top10,
                s.ask_top1,
                s.ask_top5,
                s.ask_top10,
                s.bid_liquidity,
                s.ask_liquidity,
                s.total_liquidity,
                s.imbalance,
            )?;
        }

        tracing::info!(count = snapshots.len(), "订单簿快照已保存至 {}", path);
        Ok(())
    }

    /// 打印中文快照摘要。
    pub fn print_summary(&self) {
        println!("【订单簿快照】");
        println!();
        println!("时间            : {}", self.timestamp);
        println!("市场            : {}", self.market_id);
        println!("数据源          : {}", self.provider);
        println!();
        if let Some(bid) = self.best_bid {
            println!("Best Bid        : {:.4}", bid);
        }
        if let Some(ask) = self.best_ask {
            println!("Best Ask        : {:.4}", ask);
        }
        if let Some(spread) = self.spread {
            println!("Spread          : {:.4}", spread);
        }
        if let Some(pct) = self.spread_pct {
            println!("Spread %        : {:.2}%", pct);
        }
        println!();
        println!("买盘深度 Top1   : {:.2}", self.bid_top1);
        println!("买盘深度 Top5   : {:.2}", self.bid_top5);
        println!("买盘深度 Top10  : {:.2}", self.bid_top10);
        println!();
        println!("卖盘深度 Top1   : {:.2}", self.ask_top1);
        println!("卖盘深度 Top5   : {:.2}", self.ask_top5);
        println!("卖盘深度 Top10  : {:.2}", self.ask_top10);
        println!();
        println!("总流动性        : {:.2}", self.total_liquidity);
        println!("买卖失衡度      : {:.2}%", self.imbalance * 100.0);
        println!();
    }
}

/// 格式化 Option<f64> 为 CSV 安全的字符串。
fn fmt_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{}", x),
        None => String::new(),
    }
}

/// CSV 字段转义（含逗号或引号的字段用双引号包裹）。
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::PriceLevel;

    fn ob_with_depth(
        bid: f64,
        ask: f64,
        bid_sizes: &[f64],
        ask_sizes: &[f64],
    ) -> pm_models::OrderBook {
        let bid_levels: Vec<PriceLevel> = bid_sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| PriceLevel {
                price: bid - 0.01 * (i as f64),
                size: s,
                level: i + 1,
            })
            .collect();
        let ask_levels: Vec<PriceLevel> = ask_sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| PriceLevel {
                price: ask + 0.01 * (i as f64),
                size: s,
                level: i + 1,
            })
            .collect();

        pm_models::OrderBook {
            market_id: "snap-test".into(),
            best_bid: Some(bid),
            best_ask: Some(ask),
            spread: Some(ask - bid),
            bid_depth: Some(bid_sizes.iter().sum()),
            ask_depth: Some(ask_sizes.iter().sum()),
            bid_levels,
            ask_levels,
            bid_volume: bid_sizes.iter().sum(),
            ask_volume: ask_sizes.iter().sum(),
            timestamp: Utc::now(),
            provider: "test".into(),
        }
    }

    #[test]
    fn snapshot_from_orderbook_computes_all_fields() {
        let ob = ob_with_depth(0.45, 0.47, &[100.0, 200.0], &[80.0, 120.0]);
        let now = Local::now();
        let snap = OrderBookSnapshot::from_orderbook(&ob, now);

        assert_eq!(snap.market_id, "snap-test");
        assert_eq!(snap.best_bid, Some(0.45));
        assert_eq!(snap.best_ask, Some(0.47));
        assert!((snap.spread.unwrap() - 0.02).abs() < 1e-9);
        assert!((snap.bid_top1 - 100.0).abs() < 1e-9);
        assert!((snap.ask_top1 - 80.0).abs() < 1e-9);
        assert!((snap.total_liquidity - 500.0).abs() < 1e-9);
    }

    #[test]
    fn snapshot_from_empty_orderbook() {
        let ob = pm_models::OrderBook::empty("empty", "test");
        let now = Local::now();
        let snap = OrderBookSnapshot::from_orderbook(&ob, now);

        assert_eq!(snap.market_id, "empty");
        assert!(snap.best_bid.is_none());
        assert!((snap.bid_top1 - 0.0).abs() < 1e-9);
        assert!((snap.total_liquidity - 0.0).abs() < 1e-9);
    }

    #[test]
    fn save_snapshot_csv_writes_header_once() {
        let dir = std::env::temp_dir();
        let path = dir.join("pm_ob_snapshot_test.csv");
        let _ = std::fs::remove_file(&path);
        let path = path.to_str().unwrap();

        let ob = ob_with_depth(0.45, 0.47, &[100.0], &[100.0]);
        let now = Local::now();

        let snaps1 = OrderBookSnapshot::from_orderbooks(&[ob.clone()], now);
        let snaps2 = OrderBookSnapshot::from_orderbooks(&[ob], now);

        OrderBookSnapshot::save_to_csv(&snaps1, path).unwrap();
        OrderBookSnapshot::save_to_csv(&snaps2, path).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        // 表头只出现一次
        assert_eq!(content.matches("timestamp,market_id").count(), 1);
        // 两行数据
        assert_eq!(content.trim_end().lines().count(), 3);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_empty_snapshots_is_noop() {
        let result = OrderBookSnapshot::save_to_csv(&[], "/nonexistent/path.csv");
        assert!(result.is_ok());
    }
}
