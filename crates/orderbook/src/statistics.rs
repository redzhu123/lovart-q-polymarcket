//! 市场统计累加器（V1.03 第十节）。
//!
//! 跨轮累计订单簿统计：
//! - Market Count / OrderBook Count
//! - Average Spread / Median Spread
//! - Average Liquidity / Average Depth
//! - Spread Distribution / Depth Distribution
//!
//! 所有日志输出均为中文。

use crate::depth::{DepthAnalyzer, DepthReport};
use crate::liquidity::LiquidityAnalyzer;
use crate::spread::{SpreadAnalyzer, SpreadReport};

/// 价差分布区间（用于统计分布直方图）。
#[derive(Debug, Clone, Default)]
pub struct SpreadDistribution {
    /// 价差 < 0.005（极窄）。
    pub very_tight: usize,
    /// 价差 0.005 ~ 0.02（窄）。
    pub tight: usize,
    /// 价差 0.02 ~ 0.05（中等）。
    pub medium: usize,
    /// 价差 0.05 ~ 0.10（宽）。
    pub wide: usize,
    /// 价差 > 0.10（极宽）。
    pub very_wide: usize,
}

impl SpreadDistribution {
    /// 从一个 SpreadReport 归类（使用四舍五入避免浮点精度问题）。
    fn classify(&mut self, report: &SpreadReport) {
        let spread = match report.spread {
            Some(s) => (s * 1_000_000.0).round() / 1_000_000.0,
            None => return,
        };
        if spread < 0.005 {
            self.very_tight += 1;
        } else if spread < 0.02 {
            self.tight += 1;
        } else if spread < 0.05 {
            self.medium += 1;
        } else if spread < 0.10 {
            self.wide += 1;
        } else {
            self.very_wide += 1;
        }
    }
}

/// 深度分布区间。
#[derive(Debug, Clone, Default)]
pub struct DepthDistribution {
    /// Top10 深度 < 100（极浅）。
    pub very_shallow: usize,
    /// Top10 深度 100 ~ 500（浅）。
    pub shallow: usize,
    /// Top10 深度 500 ~ 2000（中等）。
    pub medium: usize,
    /// Top10 深度 2000 ~ 10000（深）。
    pub deep: usize,
    /// Top10 深度 > 10000（极深）。
    pub very_deep: usize,
}

impl DepthDistribution {
    /// 从一个 DepthReport 归类（使用 Top10 总深度）。
    fn classify(&mut self, report: &DepthReport) {
        let total = report.bid_top10 + report.ask_top10;
        if total < 100.0 {
            self.very_shallow += 1;
        } else if total < 500.0 {
            self.shallow += 1;
        } else if total < 2000.0 {
            self.medium += 1;
        } else if total < 10000.0 {
            self.deep += 1;
        } else {
            self.very_deep += 1;
        }
    }
}

/// 市场统计数据累加器（V1.03 第十节）。
///
/// 跨轮累加订单簿的价差、流动性和深度指标，支持打印中文统计报告。
#[derive(Debug, Clone, Default)]
pub struct MarketStatistics {
    /// 累计市场数（按 market_id 去重后的数量）。
    pub market_count: usize,
    /// 累计订单簿快照数。
    pub orderbook_count: usize,
    /// 有效订单簿数（有 bid/ask 的）。
    pub valid_orderbook_count: usize,

    // --- 价差统计 ---
    /// 所有有效价差的累加和（用于计算均值）。
    pub spread_sum: f64,
    /// 最大价差。
    pub spread_max: f64,
    /// 最小价差。
    pub spread_min: f64,
    /// 价差分布。
    pub spread_distribution: SpreadDistribution,

    // --- 深度统计 ---
    /// 买盘 Top5 累加和。
    pub bid_top5_sum: f64,
    /// 卖盘 Top5 累加和。
    pub ask_top5_sum: f64,
    /// 买盘 Top10 累加和。
    pub bid_top10_sum: f64,
    /// 卖盘 Top10 累加和。
    pub ask_top10_sum: f64,
    /// 深度分布。
    pub depth_distribution: DepthDistribution,

    // --- 流动性统计 ---
    /// 买盘流动性累加和。
    pub bid_liquidity_sum: f64,
    /// 卖盘流动性累加和。
    pub ask_liquidity_sum: f64,
    /// 总流动性累加和。
    pub total_liquidity_sum: f64,
}

impl MarketStatistics {
    /// 创建空统计累加器。
    pub fn new() -> Self {
        Self {
            spread_min: f64::INFINITY,
            ..Default::default()
        }
    }

    /// 累加一轮订单簿数据。
    ///
    /// 每次调用传入本轮获取的订单簿列表，累加到累计统计中。
    pub fn accumulate(&mut self, orderbooks: &[pm_models::OrderBook]) {
        if orderbooks.is_empty() {
            return;
        }

        self.orderbook_count += orderbooks.len();

        // 价差分析
        let spread_reports: Vec<SpreadReport> = orderbooks
            .iter()
            .map(|ob| SpreadAnalyzer::analyze(ob))
            .collect();
        let valid_spreads: Vec<f64> = spread_reports.iter().filter_map(|r| r.spread).collect();

        for s in &valid_spreads {
            self.spread_sum += s;
            if *s > self.spread_max {
                self.spread_max = *s;
            }
            if *s < self.spread_min {
                self.spread_min = *s;
            }
        }
        for report in &spread_reports {
            self.spread_distribution.classify(report);
        }
        self.valid_orderbook_count += valid_spreads.len();

        // 深度分析
        let depth_reports = DepthAnalyzer::analyze_all(orderbooks);
        for report in &depth_reports {
            self.depth_distribution.classify(report);
            self.bid_top5_sum += report.bid_top5;
            self.ask_top5_sum += report.ask_top5;
            self.bid_top10_sum += report.bid_top10;
            self.ask_top10_sum += report.ask_top10;
        }

        // 流动性分析
        let liquidity_reports = LiquidityAnalyzer::analyze_all(orderbooks);
        for report in &liquidity_reports {
            self.bid_liquidity_sum += report.bid_liquidity;
            self.ask_liquidity_sum += report.ask_liquidity;
            self.total_liquidity_sum += report.total_liquidity;
        }
    }

    /// 平均价差。
    pub fn average_spread(&self) -> f64 {
        if self.valid_orderbook_count == 0 {
            0.0
        } else {
            self.spread_sum / self.valid_orderbook_count as f64
        }
    }

    /// 平均买盘 Top5 深度。
    pub fn average_bid_top5(&self) -> f64 {
        if self.orderbook_count == 0 {
            0.0
        } else {
            self.bid_top5_sum / self.orderbook_count as f64
        }
    }

    /// 平均卖盘 Top5 深度。
    pub fn average_ask_top5(&self) -> f64 {
        if self.orderbook_count == 0 {
            0.0
        } else {
            self.ask_top5_sum / self.orderbook_count as f64
        }
    }

    /// 平均总流动性。
    pub fn average_total_liquidity(&self) -> f64 {
        if self.orderbook_count == 0 {
            0.0
        } else {
            self.total_liquidity_sum / self.orderbook_count as f64
        }
    }

    /// 打印中文市场统计报告（V1.03 第十节）。
    pub fn print_report(&self) {
        let avg_spread = self.average_spread();
        let avg_bid_top5 = self.average_bid_top5();
        let avg_ask_top5 = self.average_ask_top5();
        let avg_total_liq = self.average_total_liquidity();

        println!("【市场统计】");
        println!();
        println!("市场数量        : {}", self.market_count);
        println!("订单簿快照数    : {}", self.orderbook_count);
        println!("有效订单簿数    : {}", self.valid_orderbook_count);
        println!();
        println!("--- 价差 ---");
        println!();
        println!("平均价差        : {:.4}", avg_spread);
        if self.valid_orderbook_count > 0 {
            println!("最大价差        : {:.4}", self.spread_max);
            if self.spread_min != f64::INFINITY {
                println!("最小价差        : {:.4}", self.spread_min);
            }
        }
        println!();
        println!("价差分布：");
        let sd = &self.spread_distribution;
        println!("  极窄 (<0.005)  : {}", sd.very_tight);
        println!("  窄   (0.005~.02): {}", sd.tight);
        println!("  中等 (0.02~.05) : {}", sd.medium);
        println!("  宽   (0.05~.10) : {}", sd.wide);
        println!("  极宽 (>0.10)    : {}", sd.very_wide);
        println!();
        println!("--- 深度 ---");
        println!();
        println!("平均买盘 Top5   : {:.2}", avg_bid_top5);
        println!("平均卖盘 Top5   : {:.2}", avg_ask_top5);
        println!();
        println!("深度分布（Top10 总深度）：");
        let dd = &self.depth_distribution;
        println!("  极浅 (<100)     : {}", dd.very_shallow);
        println!("  浅   (100~500)  : {}", dd.shallow);
        println!("  中等 (500~2000) : {}", dd.medium);
        println!("  深   (2000~10k) : {}", dd.deep);
        println!("  极深 (>10000)   : {}", dd.very_deep);
        println!();
        println!("--- 流动性 ---");
        println!();
        println!("平均总流动性    : {:.2}", avg_total_liq);
        println!();
    }

    /// 重置所有统计数据。
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::PriceLevel;

    fn ob_with_data(
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
            market_id: format!("mkt-{}", bid as usize),
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
    fn accumulate_and_compute_averages() {
        let mut stats = MarketStatistics::new();

        let obs = vec![
            ob_with_data(0.45, 0.47, &[100.0, 200.0], &[80.0, 120.0]),
            ob_with_data(0.40, 0.42, &[300.0, 100.0], &[50.0, 50.0]),
        ];
        stats.accumulate(&obs);

        assert_eq!(stats.orderbook_count, 2);
        assert_eq!(stats.valid_orderbook_count, 2);

        // avg spread = (0.02 + 0.02) / 2 = 0.02
        assert!((stats.average_spread() - 0.02).abs() < 1e-9);
        // spread distribution: both are 0.02 → medium range (0.02 ~ 0.05)
        assert_eq!(stats.spread_distribution.medium, 2);
    }

    #[test]
    fn accumulate_empty_is_noop() {
        let mut stats = MarketStatistics::new();
        stats.accumulate(&[]);
        assert_eq!(stats.orderbook_count, 0);
        assert_eq!(stats.valid_orderbook_count, 0);
    }

    #[test]
    fn spread_distribution_classification() {
        let mut dist = SpreadDistribution::default();
        // 极窄
        dist.classify(&SpreadReport {
            market_id: "a".into(),
            best_bid: Some(0.45),
            best_ask: Some(0.452),
            spread: Some(0.002),
            mid_price: Some(0.451),
            relative_spread: None,
            spread_pct: None,
        });
        assert_eq!(dist.very_tight, 1);

        // 极宽
        dist.classify(&SpreadReport {
            market_id: "b".into(),
            best_bid: Some(0.30),
            best_ask: Some(0.50),
            spread: Some(0.20),
            mid_price: Some(0.40),
            relative_spread: None,
            spread_pct: None,
        });
        assert_eq!(dist.very_wide, 1);
    }

    #[test]
    fn depth_distribution_classification() {
        let mut dist = DepthDistribution::default();
        // 极浅 (total < 100)
        dist.classify(&DepthReport {
            market_id: "a".into(),
            bid_top1: 10.0,
            bid_top5: 30.0,
            bid_top10: 30.0,
            ask_top1: 10.0,
            ask_top5: 20.0,
            ask_top10: 20.0,
            imbalance: 0.0,
        });
        assert_eq!(dist.very_shallow, 1);

        // 极深 (total > 10000)
        dist.classify(&DepthReport {
            market_id: "b".into(),
            bid_top1: 5000.0,
            bid_top5: 6000.0,
            bid_top10: 6000.0,
            ask_top1: 5000.0,
            ask_top5: 6000.0,
            ask_top10: 6000.0,
            imbalance: 0.0,
        });
        assert_eq!(dist.very_deep, 1);
    }

    #[test]
    fn reset_clears_all() {
        let mut stats = MarketStatistics::new();
        let obs = vec![ob_with_data(0.45, 0.47, &[100.0], &[100.0])];
        stats.accumulate(&obs);
        assert_eq!(stats.orderbook_count, 1);

        stats.reset();
        assert_eq!(stats.orderbook_count, 0);
        assert_eq!(stats.valid_orderbook_count, 0);
        assert_eq!(stats.spread_sum, 0.0);
    }
}
