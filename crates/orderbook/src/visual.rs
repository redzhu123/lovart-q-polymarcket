//! ASCII 订单簿可视化（V1.03 第十二节）。
//!
//! 将订单簿渲染为水平柱状图，便于在 CLI 中直观查看买卖盘深度分布。
//!
//! 输出格式示例：
//! ```text
//! Ask  0.48 ██████
//!      0.47 ████████
//!      0.46 ████████████████
//! ----------- 中间价 -----------
//!      0.45 ██████████████
//!      0.44 ████████
//! Bid  0.43 ██████
//! ```
//!
//! 禁止 GUI — 纯 ASCII 终端输出。

use pm_models::OrderBook;

/// ASCII 可视化渲染器（V1.03 第十二节）。
pub struct OrderBookVisualizer;

impl OrderBookVisualizer {
    /// 柱状图最大字符宽度（最深的档位用此宽度）。
    const MAX_BAR_WIDTH: usize = 40;

    /// 渲染一个订单簿为 ASCII 柱状图。
    ///
    /// 卖盘在上（价格从高到低），中间价分开，买盘在下（价格从高到低）。
    pub fn render(orderbook: &OrderBook) -> String {
        let mut buf = String::new();

        // 找到最大 size 用于缩放
        let max_size = orderbook
            .bid_levels
            .iter()
            .map(|l| l.size)
            .chain(orderbook.ask_levels.iter().map(|l| l.size))
            .fold(0.0_f64, f64::max);

        if max_size <= 0.0 {
            buf.push_str("（无盘口深度数据，无法渲染）\n");
            return buf;
        }

        let scale = Self::MAX_BAR_WIDTH as f64 / max_size;

        // 渲染卖盘（从最后一档到第一档，即价格从高到低）
        buf.push_str("【订单簿 ASCII】\n");
        buf.push('\n');
        if !orderbook.ask_levels.is_empty() {
            for level in orderbook.ask_levels.iter().rev() {
                Self::render_level(&mut buf, "Ask", level, scale);
            }
        }

        // 中间价分割线
        buf.push_str(&"─".repeat(Self::MAX_BAR_WIDTH + 20));
        buf.push('\n');

        // 渲染买盘（从第一档到最后一档，即价格从高到低）
        if !orderbook.bid_levels.is_empty() {
            for level in &orderbook.bid_levels {
                Self::render_level(&mut buf, "Bid", level, scale);
            }
        }

        buf.push('\n');
        buf
    }

    /// 渲染单档盘口到一行。
    fn render_level(buf: &mut String, label: &str, level: &pm_models::PriceLevel, scale: f64) {
        let bar_len = (level.size * scale).round() as usize;
        let bar_len = bar_len.min(Self::MAX_BAR_WIDTH).max(1);
        let bar = "█".repeat(bar_len);

        // 格式：Label  Price  Bar  Size
        let line = format!(
            "{}  {:.4}  {}  {:.1}\n",
            label,
            level.price,
            bar,
            level.size
        );
        buf.push_str(&line);
    }

    /// 渲染简化的订单簿摘要（单行买卖价 + 深度）。
    pub fn render_summary(orderbook: &OrderBook) -> String {
        let mut buf = String::new();

        buf.push_str("【订单簿摘要】\n");
        buf.push('\n');

        if let Some(bid) = orderbook.best_bid {
            let bid_depth = orderbook.bid_depth.unwrap_or(0.0);
            buf.push_str(&format!("Bid  {:.4}  深度 {:.2}\n", bid, bid_depth));
        } else {
            buf.push_str("Bid  （无）\n");
        }

        if let Some(ask) = orderbook.best_ask {
            let ask_depth = orderbook.ask_depth.unwrap_or(0.0);
            buf.push_str(&format!("Ask  {:.4}  深度 {:.2}\n", ask, ask_depth));
        } else {
            buf.push_str("Ask  （无）\n");
        }

        if let Some(spread) = orderbook.spread {
            buf.push_str(&format!("价差  {:.4}\n", spread));
        }

        buf.push('\n');
        buf
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
            market_id: "viz-test".into(),
            best_bid: bid_levels.first().map(|l| l.price),
            best_ask: ask_levels.first().map(|l| l.price),
            spread: None,
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
    fn render_basic_orderbook() {
        let ob = ob_with_levels(&[100.0, 200.0, 300.0], &[80.0, 120.0]);
        let output = OrderBookVisualizer::render(&ob);
        assert!(output.contains("【订单簿 ASCII】"));
        assert!(output.contains("Ask"));
        assert!(output.contains("Bid"));
        assert!(output.contains("─")); // 分割线
    }

    #[test]
    fn render_empty_orderbook() {
        let ob = OrderBook::empty("empty", "test");
        let output = OrderBookVisualizer::render(&ob);
        assert!(output.contains("无法渲染"));
    }

    #[test]
    fn render_summary() {
        let ob = ob_with_levels(&[100.0], &[100.0]);
        let output = OrderBookVisualizer::render_summary(&ob);
        assert!(output.contains("【订单簿摘要】"));
        assert!(output.contains("Bid"));
        assert!(output.contains("Ask"));
    }

    #[test]
    fn render_summary_empty() {
        let ob = OrderBook::empty("empty", "test");
        let output = OrderBookVisualizer::render_summary(&ob);
        assert!(output.contains("（无）"));
    }
}
