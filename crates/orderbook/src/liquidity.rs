//! 流动性分析器（V1.03 第五节）。
//!
//! 统计订单簿的买卖盘流动性：
//! - Bid Liquidity（买盘流动性 — 累计买盘量）
//! - Ask Liquidity（卖盘流动性 — 累计卖盘量）
//! - Total Liquidity（总流动性）
//! - Imbalance（买卖失衡度）
//! - Liquidity Score（流动性评分）
//!
//! 所有计算独立于 Scanner，所有日志输出均为中文。

use pm_models::OrderBook;

/// 单个订单簿的流动性分析报告。
#[derive(Debug, Clone)]
pub struct LiquidityReport {
    /// 市场标识。
    pub market_id: String,
    /// 买盘流动性（买盘累计量）。
    pub bid_liquidity: f64,
    /// 卖盘流动性（卖盘累计量）。
    pub ask_liquidity: f64,
    /// 总流动性（Bid + Ask）。
    pub total_liquidity: f64,
    /// 买卖失衡度（-1 ~ 1）：正数表示买盘更强，负数表示卖盘更强。
    /// 公式：(Bid - Ask) / (Bid + Ask)，无流动性时为 0。
    pub imbalance: f64,
    /// 流动性评分（0~100 粗略评分，越高流动性越好）。
    pub liquidity_score: f64,
}

/// 流动性分析器（V1.03 第五节）。
pub struct LiquidityAnalyzer;

impl LiquidityAnalyzer {
    /// 分析单个订单簿的流动性。
    ///
    /// 优先使用多档盘口累计量（`bid_volume` / `ask_volume`），
    /// 若无则回退到 `bid_depth` / `ask_depth`，均无则返回 0。
    pub fn analyze(orderbook: &OrderBook) -> LiquidityReport {
        // 买盘流动性：优先多档累计，回退单档深度
        let bid_liq = if orderbook.bid_volume > 0.0 {
            orderbook.bid_volume
        } else {
            orderbook.bid_depth.unwrap_or(0.0)
        };
        // 卖盘流动性：优先多档累计，回退单档深度
        let ask_liq = if orderbook.ask_volume > 0.0 {
            orderbook.ask_volume
        } else {
            orderbook.ask_depth.unwrap_or(0.0)
        };

        let total = bid_liq + ask_liq;
        let imbalance = if total > 0.0 {
            (bid_liq - ask_liq) / total
        } else {
            0.0
        };

        // 流动性评分：基于总流动性的对数映射到 0~100
        // 总流动性 0 -> 0 分；10000+ -> 100 分（粗略近似）
        let score = if total > 0.0 {
            let log_val = (total + 1.0).ln();
            let scaled = log_val / (10001.0_f64).ln() * 100.0;
            scaled.clamp(0.0, 100.0)
        } else {
            0.0
        };

        LiquidityReport {
            market_id: orderbook.market_id.clone(),
            bid_liquidity: bid_liq,
            ask_liquidity: ask_liq,
            total_liquidity: total,
            imbalance,
            liquidity_score: score,
        }
    }

    /// 批量分析流动性，返回报告列表。
    pub fn analyze_all(orderbooks: &[OrderBook]) -> Vec<LiquidityReport> {
        orderbooks.iter().map(|ob| Self::analyze(ob)).collect()
    }

    /// 打印中文流动性分析报告（单个订单簿）。
    pub fn print_report(report: &LiquidityReport) {
        println!("【流动性分析】");
        println!();
        println!("市场            : {}", report.market_id);
        println!("买盘流动性      : {:.2}", report.bid_liquidity);
        println!("卖盘流动性      : {:.2}", report.ask_liquidity);
        println!("总流动性        : {:.2}", report.total_liquidity);
        println!("买卖失衡度      : {:.2}%", report.imbalance * 100.0);
        println!(
            "流动性评分      : {:.1} / 100",
            report.liquidity_score
        );
        println!();
    }

    /// 打印中文流动性汇总（多个市场的聚合）。
    pub fn print_summary(reports: &[LiquidityReport]) {
        let n = reports.len();
        if n == 0 {
            println!("【流动性汇总】（无数据）");
            println!();
            return;
        }

        let total_bid: f64 = reports.iter().map(|r| r.bid_liquidity).sum();
        let total_ask: f64 = reports.iter().map(|r| r.ask_liquidity).sum();
        let total_liq = total_bid + total_ask;
        let avg_score: f64 = reports.iter().map(|r| r.liquidity_score).sum::<f64>() / n as f64;
        let total_imbalance = if total_liq > 0.0 {
            (total_bid - total_ask) / total_liq
        } else {
            0.0
        };

        println!("【流动性汇总】");
        println!();
        println!("分析市场数      : {}", n);
        println!("买盘流动性合计  : {:.2}", total_bid);
        println!("卖盘流动性合计  : {:.2}", total_ask);
        println!("总流动性        : {:.2}", total_liq);
        println!("整体失衡度      : {:.2}%", total_imbalance * 100.0);
        println!("平均流动性评分  : {:.1} / 100", avg_score);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn ob_with_volume(bid_vol: f64, ask_vol: f64) -> OrderBook {
        OrderBook {
            market_id: "test-liq".into(),
            best_bid: Some(0.45),
            best_ask: Some(0.47),
            spread: Some(0.02),
            bid_depth: Some(bid_vol),
            ask_depth: Some(ask_vol),
            bid_levels: vec![],
            ask_levels: vec![],
            bid_volume: bid_vol,
            ask_volume: ask_vol,
            timestamp: Utc::now(),
            provider: "test".into(),
        }
    }

    #[test]
    fn analyze_balanced_liquidity() {
        let ob = ob_with_volume(500.0, 500.0);
        let report = LiquidityAnalyzer::analyze(&ob);
        assert!((report.bid_liquidity - 500.0).abs() < 1e-9);
        assert!((report.ask_liquidity - 500.0).abs() < 1e-9);
        assert!((report.total_liquidity - 1000.0).abs() < 1e-9);
        assert!((report.imbalance - 0.0).abs() < 1e-9); // 完全均衡
    }

    #[test]
    fn analyze_bid_heavy() {
        let ob = ob_with_volume(800.0, 200.0);
        let report = LiquidityAnalyzer::analyze(&ob);
        // imbalance = (800 - 200) / 1000 = 0.6
        assert!((report.imbalance - 0.6).abs() < 1e-9);
        assert!(report.imbalance > 0.0); // 买盘更强
    }

    #[test]
    fn analyze_ask_heavy() {
        let ob = ob_with_volume(200.0, 800.0);
        let report = LiquidityAnalyzer::analyze(&ob);
        // imbalance = (200 - 800) / 1000 = -0.6
        assert!((report.imbalance + 0.6).abs() < 1e-9);
        assert!(report.imbalance < 0.0); // 卖盘更强
    }

    #[test]
    fn analyze_empty_orderbook() {
        let ob = OrderBook::empty("empty", "test");
        let report = LiquidityAnalyzer::analyze(&ob);
        assert!((report.bid_liquidity - 0.0).abs() < 1e-9);
        assert!((report.total_liquidity - 0.0).abs() < 1e-9);
        assert!((report.imbalance - 0.0).abs() < 1e-9);
        assert!((report.liquidity_score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn analyze_falls_back_to_depth() {
        // bid_volume=0 时回退到 bid_depth
        let mut ob = ob_with_volume(0.0, 0.0);
        ob.bid_depth = Some(300.0);
        ob.ask_depth = Some(200.0);
        let report = LiquidityAnalyzer::analyze(&ob);
        assert!((report.bid_liquidity - 300.0).abs() < 1e-9);
        assert!((report.ask_liquidity - 200.0).abs() < 1e-9);
    }

    #[test]
    fn analyze_all_returns_correct_count() {
        let obs = vec![
            ob_with_volume(100.0, 200.0),
            ob_with_volume(300.0, 100.0),
        ];
        let reports = LiquidityAnalyzer::analyze_all(&obs);
        assert_eq!(reports.len(), 2);
    }
}
