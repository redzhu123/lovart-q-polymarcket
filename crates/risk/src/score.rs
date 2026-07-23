//! RiskScore（V1.05 第七节）。
//!
//! 所有交易建议附带 0~100 的风险评分。
//! Risk Engine 根据 RiskScore 决定 Accept / Review / Reject。

use crate::config::RiskConfig;
use crate::context::RiskContext;

/// 风险评分（0~100）。
///
/// 综合以下维度：
/// - 仓位风险（持仓数 / 上限）
/// - 资金风险（资金利用率）
/// - 市场风险（流动性 / 价差）
/// - 回撤风险（当前回撤程度）
/// - 亏损风险（当日亏损 / 连续亏损）
/// - 暴露风险（方向 / 市场 / 类别集中度）
#[derive(Debug, Clone)]
pub struct RiskScore {
    /// 总分（0~100，越高越安全）。
    pub total: f64,
    /// 仓位维度。
    pub position_score: f64,
    /// 资金维度。
    pub capital_score: f64,
    /// 市场维度。
    pub market_score: f64,
    /// 回撤维度。
    pub drawdown_score: f64,
    /// 亏损维度。
    pub loss_score: f64,
    /// 暴露维度。
    pub exposure_score: f64,
}

impl RiskScore {
    /// 计算风险评分。
    pub fn compute(ctx: &RiskContext, config: &RiskConfig) -> Self {
        let position_score = Self::score_position(ctx, config);
        let capital_score = Self::score_capital(ctx, config);
        let market_score = Self::score_market(ctx, config);
        let drawdown_score = Self::score_drawdown(ctx, config);
        let loss_score = Self::score_loss(ctx, config);
        let exposure_score = Self::score_exposure(ctx, config);

        // 加权平均（仓位 20%、资金 15%、市场 25%、回撤 15%、亏损 15%、暴露 10%）
        let total = position_score * 0.20
            + capital_score * 0.15
            + market_score * 0.25
            + drawdown_score * 0.15
            + loss_score * 0.15
            + exposure_score * 0.10;

        Self {
            total: total.clamp(0.0, 100.0),
            position_score,
            capital_score,
            market_score,
            drawdown_score,
            loss_score,
            exposure_score,
        }
    }

    /// 仓位评分：持仓数越少分越高。
    fn score_position(ctx: &RiskContext, config: &RiskConfig) -> f64 {
        if config.max_positions == 0 {
            return 100.0;
        }
        let ratio = ctx.open_position_count as f64 / config.max_positions as f64;
        ((1.0 - ratio) * 100.0).clamp(0.0, 100.0)
    }

    /// 资金评分：资金利用率越低分越高。
    fn score_capital(ctx: &RiskContext, _config: &RiskConfig) -> f64 {
        let usage = ctx.capital_usage();
        ((1.0 - usage) * 100.0).clamp(0.0, 100.0)
    }

    /// 市场评分：流动性高、价差低分高。
    fn score_market(ctx: &RiskContext, config: &RiskConfig) -> f64 {
        let mut score: f64 = 50.0; // 基准

        // 流动性加分
        if ctx.market_liquidity > config.min_liquidity {
            let liq_ratio = (ctx.market_liquidity / config.min_liquidity).min(10.0);
            score += (liq_ratio * 5.0).min(30.0);
        } else {
            score -= 20.0;
        }

        // 价差减分
        if let Some(spread) = ctx.spread {
            if spread > config.max_slippage {
                score -= 30.0;
            } else if spread > config.max_slippage * 0.5 {
                score -= 10.0;
            }
        }

        score.clamp(0.0, 100.0)
    }

    /// 回撤评分：回撤越小分越高。
    fn score_drawdown(ctx: &RiskContext, config: &RiskConfig) -> f64 {
        if config.max_drawdown <= 0.0 {
            return 100.0;
        }
        let ratio = ctx.current_drawdown / config.max_drawdown;
        ((1.0 - ratio) * 100.0).clamp(0.0, 100.0)
    }

    /// 亏损评分：亏损越少分越高。
    fn score_loss(ctx: &RiskContext, config: &RiskConfig) -> f64 {
        let mut score: f64 = 100.0;

        // 当日亏损
        if config.max_daily_loss > 0.0 && ctx.daily_realized_pnl < 0.0 {
            let ratio = (-ctx.daily_realized_pnl) / config.max_daily_loss;
            score -= (ratio * 50.0).min(50.0);
        }

        // 连续亏损
        if config.max_consecutive_losses > 0 {
            let ratio = ctx.consecutive_losses as f64 / config.max_consecutive_losses as f64;
            score -= (ratio * 30.0).min(30.0);
        }

        score.clamp(0.0, 100.0)
    }

    /// 暴露评分：集中度越低分越高。
    fn score_exposure(ctx: &RiskContext, config: &RiskConfig) -> f64 {
        let mut score: f64 = 100.0;

        let total_exp = ctx.total_exposure_ratio();
        if total_exp > 0.5 {
            score -= 30.0;
        } else if total_exp > 0.3 {
            score -= 15.0;
        }

        // 单市场集中度
        let market_ratio = ctx.market_exposure / ctx.initial_capital.max(1.0);
        if market_ratio > config.max_market_exposure {
            score -= 20.0;
        }

        score.clamp(0.0, 100.0)
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "风险评分：{:.0}/100（仓位{:.0} 资金{:.0} 市场{:.0} 回撤{:.0} 亏损{:.0} 暴露{:.0}）",
            self.total,
            self.position_score,
            self.capital_score,
            self.market_score,
            self.drawdown_score,
            self.loss_score,
            self.exposure_score,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn perfect_score_for_idle_portfolio() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        assert!(score.total > 80.0, "idle portfolio should score > 80, got {}", score.total);
    }

    #[test]
    fn low_score_for_highly_utilized_portfolio() {
        let mut ctx = RiskContext::minimal(10000.0, 2000.0, Local::now());
        ctx.locked_cash = 8000.0;
        ctx.open_position_count = 9;
        ctx.daily_realized_pnl = -800.0;
        ctx.consecutive_losses = 4;
        ctx.current_drawdown = 0.15;
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        assert!(score.total < 70.0, "stressed portfolio should score < 70, got {}", score.total);
    }

    #[test]
    fn score_ranges_are_valid() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        assert!((0.0..=100.0).contains(&score.position_score));
        assert!((0.0..=100.0).contains(&score.capital_score));
        assert!((0.0..=100.0).contains(&score.market_score));
        assert!((0.0..=100.0).contains(&score.drawdown_score));
        assert!((0.0..=100.0).contains(&score.loss_score));
        assert!((0.0..=100.0).contains(&score.exposure_score));
    }

    #[test]
    fn summary_zh_contains_all_dimensions() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        let summary = score.summary_zh();
        assert!(summary.contains("仓位"));
        assert!(summary.contains("资金"));
        assert!(summary.contains("市场"));
        assert!(summary.contains("回撤"));
        assert!(summary.contains("亏损"));
        assert!(summary.contains("暴露"));
    }
}
