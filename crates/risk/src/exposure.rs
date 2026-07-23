//! Exposure 实时统计（V1.05 第五节）。
//!
//! 统计维度：
//! - YES / NO 方向暴露
//! - 类别暴露
//! - Provider 暴露
//! - Market 暴露
//!
//! 输出中文。

use std::collections::HashMap;

/// 单一暴露条目。
#[derive(Debug, Clone, Default)]
pub struct Exposure {
    /// YES 方向总暴露（USDC）。
    pub yes_exposure: f64,
    /// NO 方向总暴露（USDC）。
    pub no_exposure: f64,
    /// 总暴露。
    pub total_exposure: f64,
    /// 持仓数量。
    pub position_count: usize,
}

impl Exposure {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加一笔暴露。
    pub fn add(&mut self, side: &str, notional: f64) {
        match side.to_uppercase().as_str() {
            "YES" | "BUY" => self.yes_exposure += notional,
            "NO" | "SELL" => self.no_exposure += notional,
            _ => {}
        }
        self.total_exposure += notional;
        self.position_count += 1;
    }
}

/// 完整暴露报告。
#[derive(Debug, Clone)]
pub struct ExposureReport {
    /// 按方向（YES/NO）。
    pub by_side: Exposure,
    /// 按类别。
    pub by_category: HashMap<String, Exposure>,
    /// 按 Provider。
    pub by_provider: HashMap<String, Exposure>,
    /// 按市场（market_id）。
    pub by_market: HashMap<String, Exposure>,
    /// 初始资金（用于计算比例）。
    pub initial_capital: f64,
}

impl ExposureReport {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            by_side: Exposure::new(),
            by_category: HashMap::new(),
            by_provider: HashMap::new(),
            by_market: HashMap::new(),
            initial_capital,
        }
    }

    /// 记录一笔持仓暴露。
    pub fn record_position(
        &mut self,
        side: &str,
        category: Option<&str>,
        provider: &str,
        market_id: &str,
        notional: f64,
    ) {
        self.by_side.add(side, notional);

        let cat_key = category.unwrap_or("未分类").to_string();
        self.by_category
            .entry(cat_key)
            .or_default()
            .add(side, notional);

        self.by_provider
            .entry(provider.to_string())
            .or_default()
            .add(side, notional);

        self.by_market
            .entry(market_id.to_string())
            .or_default()
            .add(side, notional);
    }

    /// 获取特定市场的暴露。
    pub fn market_exposure(&self, market_id: &str) -> f64 {
        self.by_market
            .get(market_id)
            .map(|e| e.total_exposure)
            .unwrap_or(0.0)
    }

    /// 获取特定类别的暴露。
    pub fn category_exposure(&self, category: &str) -> f64 {
        self.by_category
            .get(category)
            .map(|e| e.total_exposure)
            .unwrap_or(0.0)
    }

    /// 总暴露比例。
    pub fn total_exposure_ratio(&self) -> f64 {
        if self.initial_capital > 0.0 {
            self.by_side.total_exposure / self.initial_capital
        } else {
            0.0
        }
    }

    /// 中文报告。
    pub fn report_zh(&self) -> String {
        let mut lines = Vec::new();
        lines.push("【暴露报告】".to_string());
        lines.push(String::new());

        // 方向
        lines.push(format!(
            "  YES 暴露：{:.0} USDC（{:.1}%）",
            self.by_side.yes_exposure,
            if self.initial_capital > 0.0 {
                self.by_side.yes_exposure / self.initial_capital * 100.0
            } else {
                0.0
            }
        ));
        lines.push(format!(
            "  NO  暴露：{:.0} USDC（{:.1}%）",
            self.by_side.no_exposure,
            if self.initial_capital > 0.0 {
                self.by_side.no_exposure / self.initial_capital * 100.0
            } else {
                0.0
            }
        ));
        lines.push(format!(
            "  总  暴露：{:.0} USDC（{:.1}%）",
            self.by_side.total_exposure,
            self.total_exposure_ratio() * 100.0
        ));
        lines.push(String::new());

        // 按类别
        if !self.by_category.is_empty() {
            lines.push("  按类别：".to_string());
            let mut cats: Vec<_> = self.by_category.iter().collect();
            cats.sort_by(|a, b| b.1.total_exposure.partial_cmp(&a.1.total_exposure).unwrap_or(std::cmp::Ordering::Equal));
            for (cat, exp) in cats {
                let pct = if self.initial_capital > 0.0 {
                    exp.total_exposure / self.initial_capital * 100.0
                } else {
                    0.0
                };
                lines.push(format!("    {}: {:.0} USDC（{:.1}%）", cat, exp.total_exposure, pct));
            }
            lines.push(String::new());
        }

        // 按市场（Top 5）
        if !self.by_market.is_empty() {
            lines.push("  按市场（Top 5）：".to_string());
            let mut mkts: Vec<_> = self.by_market.iter().collect();
            mkts.sort_by(|a, b| b.1.total_exposure.partial_cmp(&a.1.total_exposure).unwrap_or(std::cmp::Ordering::Equal));
            for (mkt, exp) in mkts.iter().take(5) {
                let short: String = mkt.chars().take(30).collect();
                lines.push(format!("    {}: {:.0} USDC", short, exp.total_exposure));
            }
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_add_tracks_sides() {
        let mut exp = Exposure::new();
        exp.add("YES", 100.0);
        exp.add("BUY", 50.0);
        exp.add("NO", 80.0);
        assert!((exp.yes_exposure - 150.0).abs() < 1e-9);
        assert!((exp.no_exposure - 80.0).abs() < 1e-9);
        assert!((exp.total_exposure - 230.0).abs() < 1e-9);
        assert_eq!(exp.position_count, 3);
    }

    #[test]
    fn exposure_report_tracks_categories() {
        let mut report = ExposureReport::new(10000.0);
        report.record_position("YES", Some("政治"), "gamma", "mkt1", 100.0);
        report.record_position("NO", Some("体育"), "gamma", "mkt2", 200.0);
        report.record_position("YES", Some("政治"), "clob", "mkt3", 150.0);

        assert!((report.market_exposure("mkt1") - 100.0).abs() < 1e-9);
        assert!((report.category_exposure("政治") - 250.0).abs() < 1e-9);
        assert!((report.total_exposure_ratio() - 0.045).abs() < 1e-9);

        let zh = report.report_zh();
        assert!(zh.contains("暴露报告"));
        assert!(zh.contains("YES"));
        assert!(zh.contains("NO"));
        assert!(zh.contains("政治"));
    }
}
