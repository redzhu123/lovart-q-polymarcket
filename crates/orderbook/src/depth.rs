//! 深度分析器（V1.03 第六节）。
//!
//! 统计订单簿的盘口深度：
//! - Top1 / Top5 / Top10 深度（买盘和卖盘分别统计）
//! - 深度失衡度（Depth Imbalance）
//!
//! 支持单订单簿分析与跨样本汇总。
//! 所有日志输出均为中文。

use pm_models::OrderBook;

/// 单个订单簿的深度分析报告。
#[derive(Debug, Clone)]
pub struct DepthReport {
    /// 市场标识。
    pub market_id: String,
    /// 买盘 Top1 深度（最优买价挂单量）。
    pub bid_top1: f64,
    /// 买盘 Top5 累计深度。
    pub bid_top5: f64,
    /// 买盘 Top10 累计深度。
    pub bid_top10: f64,
    /// 卖盘 Top1 深度。
    pub ask_top1: f64,
    /// 卖盘 Top5 累计深度。
    pub ask_top5: f64,
    /// 卖盘 Top10 累计深度。
    pub ask_top10: f64,
    /// 深度失衡度（-1 ~ 1）：正数表示买盘更深。
    pub imbalance: f64,
}

/// 深度汇总统计（跨多个市场）。
#[derive(Debug, Clone, Default)]
pub struct DepthSummary {
    /// 分析样本数。
    pub count: usize,
    /// 买盘平均 Top1 深度。
    pub avg_bid_top1: f64,
    /// 买盘平均 Top5 深度。
    pub avg_bid_top5: f64,
    /// 买盘平均 Top10 深度。
    pub avg_bid_top10: f64,
    /// 卖盘平均 Top1 深度。
    pub avg_ask_top1: f64,
    /// 卖盘平均 Top5 深度。
    pub avg_ask_top5: f64,
    /// 卖盘平均 Top10 深度。
    pub avg_ask_top10: f64,
    /// 平均失衡度。
    pub avg_imbalance: f64,
}

/// 深度分析器（V1.03 第六节）。
pub struct DepthAnalyzer;

impl DepthAnalyzer {
    /// 计算前 N 档的累计深度（从 level 1 开始）。
    fn top_n(levels: &[pm_models::PriceLevel], n: usize) -> f64 {
        levels.iter().take(n).map(|l| l.size).sum()
    }

    /// 分析单个订单簿的盘口深度。
    pub fn analyze(orderbook: &OrderBook) -> DepthReport {
        let bid_top1 = Self::top_n(&orderbook.bid_levels, 1);
        let bid_top5 = Self::top_n(&orderbook.bid_levels, 5);
        let bid_top10 = Self::top_n(&orderbook.bid_levels, 10);

        let ask_top1 = Self::top_n(&orderbook.ask_levels, 1);
        let ask_top5 = Self::top_n(&orderbook.ask_levels, 5);
        let ask_top10 = Self::top_n(&orderbook.ask_levels, 10);

        let total_top10 = bid_top10 + ask_top10;
        let imbalance = if total_top10 > 0.0 {
            (bid_top10 - ask_top10) / total_top10
        } else {
            0.0
        };

        DepthReport {
            market_id: orderbook.market_id.clone(),
            bid_top1,
            bid_top5,
            bid_top10,
            ask_top1,
            ask_top5,
            ask_top10,
            imbalance,
        }
    }

    /// 批量分析深度，返回报告列表。
    pub fn analyze_all(orderbooks: &[OrderBook]) -> Vec<DepthReport> {
        orderbooks.iter().map(|ob| Self::analyze(ob)).collect()
    }

    /// 从报告列表生成汇总统计。
    pub fn summarize(reports: &[DepthReport]) -> DepthSummary {
        let count = reports.len();
        if count == 0 {
            return DepthSummary::default();
        }

        let n = count as f64;
        DepthSummary {
            count,
            avg_bid_top1: reports.iter().map(|r| r.bid_top1).sum::<f64>() / n,
            avg_bid_top5: reports.iter().map(|r| r.bid_top5).sum::<f64>() / n,
            avg_bid_top10: reports.iter().map(|r| r.bid_top10).sum::<f64>() / n,
            avg_ask_top1: reports.iter().map(|r| r.ask_top1).sum::<f64>() / n,
            avg_ask_top5: reports.iter().map(|r| r.ask_top5).sum::<f64>() / n,
            avg_ask_top10: reports.iter().map(|r| r.ask_top10).sum::<f64>() / n,
            avg_imbalance: reports.iter().map(|r| r.imbalance).sum::<f64>() / n,
        }
    }

    /// 打印中文深度分析报告（单个订单簿）。
    pub fn print_report(report: &DepthReport) {
        println!("【深度分析】");
        println!();
        println!("市场            : {}", report.market_id);
        println!();
        println!("买盘：");
        println!("  Top 1         : {:.2}", report.bid_top1);
        println!("  Top 5         : {:.2}", report.bid_top5);
        println!("  Top 10        : {:.2}", report.bid_top10);
        println!();
        println!("卖盘：");
        println!("  Top 1         : {:.2}", report.ask_top1);
        println!("  Top 5         : {:.2}", report.ask_top5);
        println!("  Top 10        : {:.2}", report.ask_top10);
        println!();
        println!("深度失衡度      : {:.2}%", report.imbalance * 100.0);
        println!();
    }

    /// 打印中文深度汇总统计。
    pub fn print_summary(summary: &DepthSummary) {
        println!("【深度汇总】");
        println!();
        println!("分析市场数      : {}", summary.count);
        if summary.count == 0 {
            println!("（无数据）");
            println!();
            return;
        }
        println!();
        println!("买盘平均深度：");
        println!("  Top 1         : {:.2}", summary.avg_bid_top1);
        println!("  Top 5         : {:.2}", summary.avg_bid_top5);
        println!("  Top 10        : {:.2}", summary.avg_bid_top10);
        println!();
        println!("卖盘平均深度：");
        println!("  Top 1         : {:.2}", summary.avg_ask_top1);
        println!("  Top 5         : {:.2}", summary.avg_ask_top5);
        println!("  Top 10        : {:.2}", summary.avg_ask_top10);
        println!();
        println!("平均深度失衡度  : {:.2}%", summary.avg_imbalance * 100.0);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::PriceLevel;

    fn ob_with_levels(bid_sizes: &[f64], ask_sizes: &[f64]) -> OrderBook {
        let bid_levels: Vec<PriceLevel> = bid_sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| PriceLevel {
                price: 0.50 - 0.01 * (i as f64),
                size: s,
                level: i + 1,
            })
            .collect();
        let ask_levels: Vec<PriceLevel> = ask_sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| PriceLevel {
                price: 0.50 + 0.01 * (i as f64 + 1.0),
                size: s,
                level: i + 1,
            })
            .collect();

        OrderBook {
            market_id: "test-depth".into(),
            best_bid: bid_levels.first().map(|l| l.price),
            best_ask: ask_levels.first().map(|l| l.price),
            spread: None,
            bid_depth: None,
            ask_depth: None,
            bid_levels,
            ask_levels,
            bid_volume: bid_sizes.iter().sum(),
            ask_volume: ask_sizes.iter().sum(),
            timestamp: Utc::now(),
            provider: "test".into(),
        }
    }

    #[test]
    fn analyze_top1_top5_top10() {
        // 买盘：100, 200, 300, 50, 50, 50, 50, 50, 50, 50, 100
        let bid = vec![
            100.0, 200.0, 300.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 50.0, 100.0,
        ];
        // 卖盘：80, 120, 100
        let ask = vec![80.0, 120.0, 100.0];

        let ob = ob_with_levels(&bid, &ask);
        let report = DepthAnalyzer::analyze(&ob);

        assert!((report.bid_top1 - 100.0).abs() < 1e-9);
        // Top5 = 100+200+300+50+50 = 700
        assert!((report.bid_top5 - 700.0).abs() < 1e-9);
        // Top10 = 100+200+300+50*7 = 950
        assert!((report.bid_top10 - 950.0).abs() < 1e-9);

        assert!((report.ask_top1 - 80.0).abs() < 1e-9);
        assert!((report.ask_top5 - 300.0).abs() < 1e-9); // 80+120+100 = 300
        assert!((report.ask_top10 - 300.0).abs() < 1e-9); // only 3 levels
    }

    #[test]
    fn analyze_empty_levels() {
        let ob = OrderBook::empty("empty", "test");
        let report = DepthAnalyzer::analyze(&ob);
        assert!((report.bid_top1 - 0.0).abs() < 1e-9);
        assert!((report.bid_top10 - 0.0).abs() < 1e-9);
        assert!((report.ask_top10 - 0.0).abs() < 1e-9);
        assert!((report.imbalance - 0.0).abs() < 1e-9);
    }

    #[test]
    fn summarize_produces_averages() {
        let ob1 = ob_with_levels(&[100.0, 200.0], &[50.0, 50.0]);
        let ob2 = ob_with_levels(&[300.0, 100.0], &[100.0, 200.0]);
        let reports = DepthAnalyzer::analyze_all(&[ob1, ob2]);
        let summary = DepthAnalyzer::summarize(&reports);

        assert_eq!(summary.count, 2);
        // avg bid top1 = (100 + 300) / 2 = 200
        assert!((summary.avg_bid_top1 - 200.0).abs() < 1e-9);
        // avg ask top5 = (100 + 300) / 2 = 200 (both have <5 levels, all included)
        assert!((summary.avg_ask_top5 - 200.0).abs() < 1e-9);
    }
}
