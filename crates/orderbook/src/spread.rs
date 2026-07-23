//! 价差分析器（V1.03 第四节）。
//!
//! 计算订单簿的买卖价差相关指标：
//! - BestAsk - BestBid（绝对价差）
//! - MidPrice（中间价）
//! - Relative Spread（相对价差）
//! - Spread %（价差百分比）
//!
//! 支持单订单簿分析与跨样本聚合统计（均值 / 最大 / 最小）。
//! 所有日志输出均为中文。

use pm_models::OrderBook;

/// 单个订单簿的价差分析报告。
#[derive(Debug, Clone)]
pub struct SpreadReport {
    /// 市场标识。
    pub market_id: String,
    /// 最优买价。
    pub best_bid: Option<f64>,
    /// 最优卖价。
    pub best_ask: Option<f64>,
    /// 绝对价差（BestAsk - BestBid）。
    pub spread: Option<f64>,
    /// 中间价（(BestBid + BestAsk) / 2）。
    pub mid_price: Option<f64>,
    /// 相对价差（Spread / MidPrice）。
    pub relative_spread: Option<f64>,
    /// 价差百分比（Relative Spread × 100）。
    pub spread_pct: Option<f64>,
}

/// 跨样本价差汇总统计。
#[derive(Debug, Clone, Default)]
pub struct SpreadSummary {
    /// 分析样本数。
    pub count: usize,
    /// 有效价差样本数（竞买价和竞卖价都有值的样本数）。
    pub valid_count: usize,
    /// 平均绝对价差。
    pub average_spread: f64,
    /// 最大绝对价差。
    pub maximum_spread: f64,
    /// 最小绝对价差。
    pub minimum_spread: f64,
    /// 平均相对价差（百分比）。
    pub average_relative_spread_pct: f64,
    /// 平均中间价。
    pub average_mid_price: f64,
}

/// 价差分析器（V1.03 第四节）。
pub struct SpreadAnalyzer;

impl SpreadAnalyzer {
    /// 分析单个订单簿的价差。
    ///
    /// 当 BestBid 和 BestAsk 都存在时计算所有指标；否则相应字段为 None。
    pub fn analyze(orderbook: &OrderBook) -> SpreadReport {
        let spread = orderbook.spread;
        let mid_price = match (orderbook.best_bid, orderbook.best_ask) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            _ => None,
        };
        let relative_spread = match (spread, mid_price) {
            (Some(s), Some(m)) if m > 0.0 => Some(s / m),
            _ => None,
        };
        let spread_pct = relative_spread.map(|r| r * 100.0);

        SpreadReport {
            market_id: orderbook.market_id.clone(),
            best_bid: orderbook.best_bid,
            best_ask: orderbook.best_ask,
            spread,
            mid_price,
            relative_spread,
            spread_pct,
        }
    }

    /// 对多个订单簿进行价差分析，生成汇总统计。
    pub fn summarize(orderbooks: &[OrderBook]) -> SpreadSummary {
        let reports: Vec<SpreadReport> = orderbooks.iter().map(|ob| Self::analyze(ob)).collect();
        Self::summarize_reports(&reports)
    }

    /// 从已有的报告列表生成汇总统计。
    pub fn summarize_reports(reports: &[SpreadReport]) -> SpreadSummary {
        let valid: Vec<&SpreadReport> = reports.iter().filter(|r| r.spread.is_some()).collect();

        let count = reports.len();
        let valid_count = valid.len();

        if valid.is_empty() {
            return SpreadSummary {
                count,
                valid_count: 0,
                ..Default::default()
            };
        }

        let spreads: Vec<f64> = valid.iter().filter_map(|r| r.spread).collect();
        let rel_pcts: Vec<f64> = valid.iter().filter_map(|r| r.spread_pct).collect();
        let mid_prices: Vec<f64> = valid.iter().filter_map(|r| r.mid_price).collect();

        let n = spreads.len() as f64;
        let average_spread = spreads.iter().sum::<f64>() / n;
        let maximum_spread = spreads.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let minimum_spread = spreads.iter().cloned().fold(f64::INFINITY, f64::min);
        let average_relative_spread_pct = if rel_pcts.is_empty() {
            0.0
        } else {
            rel_pcts.iter().sum::<f64>() / rel_pcts.len() as f64
        };
        let average_mid_price = if mid_prices.is_empty() {
            0.0
        } else {
            mid_prices.iter().sum::<f64>() / mid_prices.len() as f64
        };

        SpreadSummary {
            count,
            valid_count,
            average_spread,
            maximum_spread,
            minimum_spread,
            average_relative_spread_pct,
            average_mid_price,
        }
    }

    /// 打印中文价差分析报告（单个订单簿）。
    pub fn print_report(report: &SpreadReport) {
        println!("【价差分析】");
        println!();
        println!("市场          : {}", report.market_id);
        if let Some(bid) = report.best_bid {
            println!("Best Bid      : {:.4}", bid);
        } else {
            println!("Best Bid      : （无）");
        }
        if let Some(ask) = report.best_ask {
            println!("Best Ask      : {:.4}", ask);
        } else {
            println!("Best Ask      : （无）");
        }
        if let Some(spread) = report.spread {
            println!("Spread        : {:.4}", spread);
        } else {
            println!("Spread        : （无法计算）");
        }
        if let Some(mid) = report.mid_price {
            println!("Mid Price     : {:.4}", mid);
        }
        if let Some(rel) = report.relative_spread {
            println!("Relative Spread: {:.6}", rel);
        }
        if let Some(pct) = report.spread_pct {
            println!("Spread %      : {:.2}%", pct);
        }
        println!();
    }

    /// 打印中文价差汇总统计。
    pub fn print_summary(summary: &SpreadSummary) {
        println!("【价差汇总】");
        println!();
        println!("分析样本数    : {}", summary.count);
        println!("有效样本数    : {}", summary.valid_count);
        if summary.valid_count > 0 {
            println!("平均绝对价差  : {:.4}", summary.average_spread);
            println!("最大绝对价差  : {:.4}", summary.maximum_spread);
            println!("最小绝对价差  : {:.4}", summary.minimum_spread);
            println!("平均相对价差  : {:.2}%", summary.average_relative_spread_pct);
            println!("平均中间价    : {:.4}", summary.average_mid_price);
        } else {
            println!("（无有效价差数据，无法计算统计指标）");
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn ob_with_spread(bid: f64, ask: f64) -> OrderBook {
        let spread = ask - bid;
        OrderBook {
            market_id: "test".into(),
            best_bid: Some(bid),
            best_ask: Some(ask),
            spread: Some(spread),
            bid_depth: Some(100.0),
            ask_depth: Some(100.0),
            bid_levels: vec![],
            ask_levels: vec![],
            bid_volume: 0.0,
            ask_volume: 0.0,
            timestamp: Utc::now(),
            provider: "test".into(),
        }
    }

    #[test]
    fn analyze_basic_spread() {
        let ob = ob_with_spread(0.45, 0.47);
        let report = SpreadAnalyzer::analyze(&ob);
        assert!((report.spread.unwrap() - 0.02).abs() < 1e-9);
        assert!((report.mid_price.unwrap() - 0.46).abs() < 1e-9);
        let rel = report.relative_spread.unwrap();
        assert!((rel - 0.02 / 0.46).abs() < 1e-6);
        let pct = report.spread_pct.unwrap();
        assert!((pct - (0.02 / 0.46 * 100.0)).abs() < 1e-4);
    }

    #[test]
    fn analyze_no_bid_ask() {
        let ob = OrderBook::empty("empty", "test");
        let report = SpreadAnalyzer::analyze(&ob);
        assert!(report.spread.is_none());
        assert!(report.mid_price.is_none());
        assert!(report.relative_spread.is_none());
    }

    #[test]
    fn summarize_multiple() {
        let obs = vec![
            ob_with_spread(0.40, 0.42),
            ob_with_spread(0.45, 0.48),
            ob_with_spread(0.50, 0.55),
        ];
        let summary = SpreadAnalyzer::summarize(&obs);
        assert_eq!(summary.count, 3);
        assert_eq!(summary.valid_count, 3);
        // avg spread = (0.02 + 0.03 + 0.05) / 3 = 0.0333...
        assert!((summary.average_spread - 0.03333).abs() < 0.001);
        assert!((summary.maximum_spread - 0.05).abs() < 1e-9);
        assert!((summary.minimum_spread - 0.02).abs() < 1e-9);
    }

    #[test]
    fn summarize_empty() {
        let summary = SpreadAnalyzer::summarize(&[]);
        assert_eq!(summary.count, 0);
        assert_eq!(summary.valid_count, 0);
    }

    #[test]
    fn summarize_mixed_empty() {
        let obs = vec![
            ob_with_spread(0.40, 0.42),
            OrderBook::empty("no_price", "test"),
        ];
        let summary = SpreadAnalyzer::summarize(&obs);
        assert_eq!(summary.count, 2);
        assert_eq!(summary.valid_count, 1);
        assert!((summary.average_spread - 0.02).abs() < 1e-9);
    }
}
