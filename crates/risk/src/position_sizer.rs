//! Position Sizer（V1.05 第四节）。
//!
//! 支持多种仓位规模策略，通过配置切换，默认 Fixed Size。
//!
//! 策略：
//! - [`FixedSizer`]：固定金额
//! - [`FixedRiskSizer`]：固定风险比例（基于止损距离）
//! - [`KellySizer`]：Kelly 公式（基于胜率与赔率）
//! - [`VolatilitySizer`]：基于波动率调整
//! - [`LiquiditySizer`]：基于流动性调整
//! - [`ConfidenceSizer`]：基于置信度调整

use crate::config::{PositionSizerKind, RiskConfig};
use crate::context::RiskContext;

/// 仓位规模建议。
#[derive(Debug, Clone)]
pub struct SizeRecommendation {
    /// 建议金额（USDC）。
    pub notional: f64,
    /// 建议数量（= notional / price）。
    pub quantity: f64,
    /// 使用的策略。
    pub sizer_kind: PositionSizerKind,
    /// 说明（中文）。
    pub explanation: String,
}

/// 仓位规模策略 trait。
pub trait PositionSizer: Send + Sync {
    /// 策略名称。
    fn kind(&self) -> PositionSizerKind;

    /// 计算建议仓位规模。
    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation;
}

// ============================================================================
// Fixed Size
// ============================================================================

pub struct FixedSizer;

impl PositionSizer for FixedSizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::Fixed
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        let notional = config.fixed_size.min(ctx.available_cash);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::Fixed,
            explanation: format!("固定金额：每笔 {:.0} USDC", notional),
        }
    }
}

// ============================================================================
// Fixed Risk
// ============================================================================

pub struct FixedRiskSizer;

impl PositionSizer for FixedRiskSizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::FixedRisk
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        // 风险金额 = 初始资金 * risk_ratio
        let risk_amount = ctx.initial_capital * config.risk_ratio;
        // 假设止损距离为价差的 2 倍或 5%
        let stop_distance = ctx.spread.unwrap_or(0.01).max(0.01).min(0.10);
        let notional = (risk_amount / stop_distance)
            .min(ctx.available_cash)
            .min(config.max_single_capital);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::FixedRisk,
            explanation: format!(
                "固定风险：风险金额 {:.0} USDC（{:.1}%），止损距离 {:.1}%，仓位 {:.0} USDC",
                risk_amount,
                config.risk_ratio * 100.0,
                stop_distance * 100.0,
                notional
            ),
        }
    }
}

// ============================================================================
// Kelly
// ============================================================================

pub struct KellySizer;

impl PositionSizer for KellySizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::Kelly
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        // Kelly f* = (p * b - (1-p)) / b
        // p = confidence, b = expected_roi（赔率）
        let p = ctx
            .opportunity
            .as_ref()
            .map(|o| o.confidence)
            .unwrap_or(0.5)
            .clamp(0.05, 0.95);
        let b = ctx
            .opportunity
            .as_ref()
            .map(|o| o.expected_roi)
            .unwrap_or(0.05)
            .max(0.01);

        let kelly_f = ((p * b - (1.0 - p)) / b).clamp(0.0, 0.25); // 上限 25%（半 Kelly）
        let half_kelly = kelly_f * 0.5; // 半 Kelly 更保守
        let notional = (ctx.initial_capital * half_kelly)
            .min(ctx.available_cash)
            .min(config.max_single_capital);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::Kelly,
            explanation: format!(
                "Kelly公式：胜率 {:.0}%，赔率 {:.2}，半Kelly比例 {:.1}%，仓位 {:.0} USDC",
                p * 100.0,
                b,
                half_kelly * 100.0,
                notional
            ),
        }
    }
}

// ============================================================================
// Volatility
// ============================================================================

pub struct VolatilitySizer;

impl PositionSizer for VolatilitySizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::Volatility
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        // 波动率越高仓位越小
        let vol = ctx
            .opportunity
            .as_ref()
            .map(|o| o.volatility_score / 100.0)
            .unwrap_or(0.3)
            .clamp(0.01, 1.0);

        // 基础仓位 = fixed_size，波动率调整
        let base = config.fixed_size;
        let adjusted = base * (1.0 - vol * 0.7); // 高波动时最多缩到 30%
        let notional = adjusted
            .max(10.0)
            .min(ctx.available_cash)
            .min(config.max_single_capital);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::Volatility,
            explanation: format!(
                "波动率调整：波动率估计 {:.0}%，基础 {:.0} USDC → 调整后 {:.0} USDC",
                vol * 100.0,
                base,
                notional
            ),
        }
    }
}

// ============================================================================
// Liquidity Based
// ============================================================================

pub struct LiquiditySizer;

impl PositionSizer for LiquiditySizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::Liquidity
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        // 仓位不超过市场流动性的 2%
        let max_from_liquidity = ctx.market_liquidity * 0.02;
        let notional = config
            .fixed_size
            .min(max_from_liquidity)
            .min(ctx.available_cash)
            .min(config.max_single_capital)
            .max(10.0);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::Liquidity,
            explanation: format!(
                "流动性调整：市场流动性 {:.0} USDC × 2% = {:.0} USDC，仓位 {:.0} USDC",
                ctx.market_liquidity, max_from_liquidity, notional
            ),
        }
    }
}

// ============================================================================
// Confidence Based
// ============================================================================

pub struct ConfidenceSizer;

impl PositionSizer for ConfidenceSizer {
    fn kind(&self) -> PositionSizerKind {
        PositionSizerKind::Confidence
    }

    fn size(&self, ctx: &RiskContext, config: &RiskConfig) -> SizeRecommendation {
        // 置信度越高仓位越大
        let confidence = ctx
            .opportunity
            .as_ref()
            .map(|o| o.confidence)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        // 映射：confidence 0.5 → 80%，1.0 → 120%，<0.3 → 50%
        let multiplier = 0.5 + confidence;
        let notional = (config.fixed_size * multiplier)
            .min(ctx.available_cash)
            .min(config.max_single_capital)
            .max(10.0);
        let price = ctx.suggested_price.max(f64::EPSILON);
        let quantity = notional / price;

        SizeRecommendation {
            notional,
            quantity,
            sizer_kind: PositionSizerKind::Confidence,
            explanation: format!(
                "置信度调整：置信度 {:.0}%，乘数 {:.2}，仓位 {:.0} USDC",
                confidence * 100.0,
                multiplier,
                notional
            ),
        }
    }
}

// ============================================================================
// Factory
// ============================================================================

/// 根据配置创建对应的 PositionSizer。
pub fn create_sizer(kind: PositionSizerKind) -> Box<dyn PositionSizer> {
    match kind {
        PositionSizerKind::Fixed => Box::new(FixedSizer),
        PositionSizerKind::FixedRisk => Box::new(FixedRiskSizer),
        PositionSizerKind::Kelly => Box::new(KellySizer),
        PositionSizerKind::Volatility => Box::new(VolatilitySizer),
        PositionSizerKind::Liquidity => Box::new(LiquiditySizer),
        PositionSizerKind::Confidence => Box::new(ConfidenceSizer),
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn test_ctx() -> RiskContext {
        let mut ctx = RiskContext::minimal(10000.0, 9000.0, Local::now());
        ctx.suggested_price = 0.5;
        ctx.market_liquidity = 10000.0;
        ctx
    }

    fn test_config() -> RiskConfig {
        RiskConfig::default()
    }

    #[test]
    fn fixed_sizer_uses_config_value() {
        let sizer = FixedSizer;
        let rec = sizer.size(&test_ctx(), &test_config());
        assert!((rec.notional - 100.0).abs() < 1e-9);
        assert_eq!(rec.sizer_kind, PositionSizerKind::Fixed);
    }

    #[test]
    fn fixed_sizer_capped_by_available_cash() {
        let mut ctx = test_ctx();
        ctx.available_cash = 50.0;
        let sizer = FixedSizer;
        let rec = sizer.size(&ctx, &test_config());
        assert!(rec.notional <= 50.0);
    }

    #[test]
    fn kelly_sizer_bounds() {
        let sizer = KellySizer;
        let mut ctx = test_ctx();
        // 设置有利可图的机会使 Kelly 给出正仓位
        // Kelly: f* = (p*b - (1-p)) / b = (0.8*0.5 - 0.2)/0.5 = 0.4, half=0.2 → 2000 USDC
        use pm_opportunity::Opportunity;
        let opp = Opportunity::new(
            "test".into(),
            "test".into(),
            "test".into(),
            chrono::Utc::now(),
            pm_opportunity::OpportunityType::Arbitrage,
            80.0,
            0.8,
            80,
            25.0,
            20.0,
            15.0,
            10.0,
            5.0,
            10.0,
            0.50,
            50.0,
            0.40,
            0.50,
            0.90,
            None,
            1000.0,
            2000.0,
            Some(500.0),
            Some(600.0),
        );
        ctx.opportunity = Some(opp);
        let rec = sizer.size(&ctx, &test_config());
        // 半Kelly上限25%，所以最大 2500
        assert!(rec.notional <= 2500.0);
        assert!(
            rec.notional > 0.0,
            "Kelly notional should be > 0 with positive edge"
        );
        assert_eq!(rec.sizer_kind, PositionSizerKind::Kelly);
    }

    #[test]
    fn all_sizers_produce_valid_recommendations() {
        let mut ctx = test_ctx();
        let config = test_config();

        // 给 Kelly/Confidence 等需要机会信息的策略提供有利可图的机会
        // Kelly: f* = (0.8*0.5 - 0.2)/0.5 = 0.4, half=0.2 → 2000 USDC > 0
        use pm_opportunity::Opportunity;
        let opp = Opportunity::new(
            "test".into(),
            "test".into(),
            "test".into(),
            chrono::Utc::now(),
            pm_opportunity::OpportunityType::Arbitrage,
            80.0,
            0.8,
            80,
            25.0,
            20.0,
            15.0,
            10.0,
            5.0,
            10.0,
            0.50,
            50.0,
            0.40,
            0.50,
            0.90,
            None,
            1000.0,
            2000.0,
            Some(500.0),
            Some(600.0),
        );
        ctx.opportunity = Some(opp);

        let kinds = [
            PositionSizerKind::Fixed,
            PositionSizerKind::FixedRisk,
            PositionSizerKind::Kelly,
            PositionSizerKind::Volatility,
            PositionSizerKind::Liquidity,
            PositionSizerKind::Confidence,
        ];
        for kind in &kinds {
            let sizer = create_sizer(*kind);
            let rec = sizer.size(&ctx, &config);
            assert!(
                rec.notional > 0.0,
                "sizer {:?} returned zero notional",
                kind
            );
            assert!(
                rec.notional.is_finite(),
                "sizer {:?} returned non-finite",
                kind
            );
            assert!(
                !rec.explanation.is_empty(),
                "sizer {:?} returned empty explanation",
                kind
            );
            assert_eq!(rec.sizer_kind, *kind);
        }
    }
}
