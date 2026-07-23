//! Portfolio Risk 实时统计（V1.05 第六节）。
//!
//! 统计维度：
//! - 资金利用率
//! - 风险暴露
//! - 现金比例
//! - 最大回撤
//! - 连续亏损
//! - 当前风险等级
//!
//! 输出中文。

use crate::config::RiskConfig;
use crate::context::RiskContext;

/// 风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// 低风险：一切正常。
    Low,
    /// 中等风险：部分指标接近上限。
    Medium,
    /// 高风险：多项指标达到或接近上限。
    High,
    /// 紧急：触发硬限制，禁止新交易。
    Critical,
}

impl RiskLevel {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RiskLevel::Low => "低",
            RiskLevel::Medium => "中",
            RiskLevel::High => "高",
            RiskLevel::Critical => "紧急",
        }
    }

    pub fn as_color(&self) -> &'static str {
        match self {
            RiskLevel::Low => "🟢",
            RiskLevel::Medium => "🟡",
            RiskLevel::High => "🟠",
            RiskLevel::Critical => "🔴",
        }
    }
}

/// 组合风险快照。
#[derive(Debug, Clone)]
pub struct PortfolioRisk {
    /// 当前风险等级。
    pub risk_level: RiskLevel,
    /// 资金利用率（0.0~1.0）。
    pub capital_usage: f64,
    /// 风险暴露比例（0.0~1.0）。
    pub exposure_ratio: f64,
    /// 现金比例（0.0~1.0）。
    pub cash_ratio: f64,
    /// 最大回撤（0.0~1.0）。
    pub max_drawdown: f64,
    /// 当前回撤（0.0~1.0）。
    pub current_drawdown: f64,
    /// 连续亏损次数。
    pub consecutive_losses: usize,
    /// 当日已实现盈亏（USDC）。
    pub daily_pnl: f64,
    /// 总 ROI。
    pub roi: f64,
    /// 活跃警告数。
    pub warning_count: usize,
    /// 拒绝交易计数。
    pub rejection_count: usize,
    /// 风险事件计数。
    pub risk_event_count: usize,
}

/// 组合风险报告（含完整中文输出）。
#[derive(Debug, Clone)]
pub struct PortfolioRiskReport {
    pub risk: PortfolioRisk,
    /// 各维度详细说明。
    pub details: Vec<String>,
}

impl PortfolioRiskReport {
    /// 从 RiskContext + RiskConfig 计算。
    pub fn compute(ctx: &RiskContext, config: &RiskConfig) -> Self {
        let capital_usage = ctx.capital_usage();
        let exposure_ratio = ctx.total_exposure_ratio();
        let cash_ratio = ctx.cash_ratio();
        let current_drawdown = ctx.current_drawdown;

        let mut warnings = Vec::new();
        let mut risk_score = 0u32;

        // 资金利用率检查
        if capital_usage > config.max_capital_usage {
            risk_score += 3;
            warnings.push(format!(
                "资金利用率 {:.0}% 超过上限 {:.0}%",
                capital_usage * 100.0,
                config.max_capital_usage * 100.0
            ));
        } else if capital_usage > config.max_capital_usage * 0.7 {
            risk_score += 1;
            warnings.push(format!(
                "资金利用率 {:.0}% 接近上限 {:.0}%",
                capital_usage * 100.0,
                config.max_capital_usage * 100.0
            ));
        }

        // 回撤检查
        if current_drawdown > config.max_drawdown {
            risk_score += 3;
            warnings.push(format!(
                "回撤 {:.1}% 超过上限 {:.1}%",
                current_drawdown * 100.0,
                config.max_drawdown * 100.0
            ));
        } else if current_drawdown > config.max_drawdown * 0.7 {
            risk_score += 1;
            warnings.push(format!(
                "回撤 {:.1}% 接近上限 {:.1}%",
                current_drawdown * 100.0,
                config.max_drawdown * 100.0
            ));
        }

        // 当日亏损检查
        if ctx.daily_realized_pnl < 0.0 {
            let loss_ratio = (-ctx.daily_realized_pnl) / config.max_daily_loss;
            if loss_ratio > 1.0 {
                risk_score += 3;
                warnings.push(format!(
                    "当日亏损 {:.0} USDC 超过上限 {:.0} USDC",
                    -ctx.daily_realized_pnl,
                    config.max_daily_loss
                ));
            } else if loss_ratio > 0.7 {
                risk_score += 2;
                warnings.push(format!(
                    "当日亏损 {:.0} USDC 接近上限 {:.0} USDC",
                    -ctx.daily_realized_pnl,
                    config.max_daily_loss
                ));
            }
        }

        // 连续亏损检查
        if ctx.consecutive_losses >= config.max_consecutive_losses {
            risk_score += 3;
            warnings.push(format!(
                "连续亏损 {} 次达到上限 {} 次",
                ctx.consecutive_losses, config.max_consecutive_losses
            ));
        } else if ctx.consecutive_losses >= config.max_consecutive_losses.saturating_sub(2) {
            risk_score += 1;
            warnings.push(format!(
                "连续亏损 {} 次接近上限 {} 次",
                ctx.consecutive_losses, config.max_consecutive_losses
            ));
        }

        // 持仓数检查
        if ctx.open_position_count >= config.max_positions {
            risk_score += 2;
            warnings.push(format!(
                "持仓数 {} 达到上限 {}",
                ctx.open_position_count, config.max_positions
            ));
        }

        let risk_level = match risk_score {
            0 => RiskLevel::Low,
            1..=2 => RiskLevel::Medium,
            3..=5 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };

        let risk = PortfolioRisk {
            risk_level,
            capital_usage,
            exposure_ratio,
            cash_ratio,
            max_drawdown: config.max_drawdown,
            current_drawdown,
            consecutive_losses: ctx.consecutive_losses,
            daily_pnl: ctx.daily_realized_pnl,
            roi: ctx.roi(),
            warning_count: warnings.len(),
            rejection_count: 0,    // 由 engine 更新
            risk_event_count: 0,   // 由 engine 更新
        };

        Self {
            risk,
            details: warnings,
        }
    }

    /// 中文仪表盘输出。
    pub fn dashboard_zh(&self) -> String {
        let r = &self.risk;
        let mut lines = Vec::new();

        lines.push("【风险仪表盘】".to_string());
        lines.push(String::new());
        lines.push(format!(
            "  风险等级：    {} {}",
            r.risk_level.as_color(),
            r.risk_level.as_zh()
        ));
        lines.push(format!(
            "  资金利用率：  {:.0}%",
            r.capital_usage * 100.0
        ));
        lines.push(format!(
            "  最大回撤：    {:.1}%（当前 {:.1}%）",
            r.max_drawdown * 100.0,
            r.current_drawdown * 100.0
        ));
        lines.push(format!(
            "  风险暴露：    {:.0}%",
            r.exposure_ratio * 100.0
        ));
        lines.push(format!(
            "  现金比例：    {:.0}%",
            r.cash_ratio * 100.0
        ));
        lines.push(format!(
            "  连续亏损：    {} 次",
            r.consecutive_losses
        ));
        lines.push(format!(
            "  当日盈亏：    {:.0} USDC",
            r.daily_pnl
        ));
        lines.push(format!(
            "  总 ROI：      {:.2}%",
            r.roi * 100.0
        ));
        lines.push(format!(
            "  拒绝交易：    {} 笔",
            r.rejection_count
        ));
        lines.push(format!(
            "  风险事件：    {} 个",
            r.risk_event_count
        ));

        if !self.details.is_empty() {
            lines.push(String::new());
            lines.push("  ⚠️ 警告：".to_string());
            for d in &self.details {
                lines.push(format!("    - {}", d));
            }
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn idle_portfolio_is_low_risk() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let report = PortfolioRiskReport::compute(&ctx, &config);
        assert_eq!(report.risk.risk_level, RiskLevel::Low);
        assert!(report.details.is_empty());
    }

    #[test]
    fn stressed_portfolio_is_high_risk() {
        let mut ctx = RiskContext::minimal(10000.0, 2000.0, Local::now());
        ctx.locked_cash = 8000.0;
        ctx.open_position_count = 10;
        ctx.daily_realized_pnl = -1200.0;
        ctx.consecutive_losses = 5;
        ctx.current_drawdown = 0.25;
        let config = RiskConfig::default();
        let report = PortfolioRiskReport::compute(&ctx, &config);
        assert!(report.risk.risk_level >= RiskLevel::High);
        assert!(!report.details.is_empty());
    }

    #[test]
    fn dashboard_zh_contains_key_metrics() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let report = PortfolioRiskReport::compute(&ctx, &config);
        let dash = report.dashboard_zh();
        assert!(dash.contains("风险仪表盘"));
        assert!(dash.contains("风险等级"));
        assert!(dash.contains("资金利用率"));
        assert!(dash.contains("最大回撤"));
        assert!(dash.contains("连续亏损"));
    }
}
