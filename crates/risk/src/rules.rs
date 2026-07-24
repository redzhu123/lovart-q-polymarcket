//! Risk Rules（V1.05 第三节）。
//!
//! 统一风险规则引擎。所有规则实现 [`RiskRule`] trait。
//! 每个规则接收 `&RiskContext` + `&RiskConfig`，返回通过/拒绝及理由。
//!
//! 内置规则：
//! - [`MaxPositionCountRule`]：最大持仓数量
//! - [`PositionSizeLimitRule`]：单笔最大资金
//! - [`MaxOrderCountRule`]：最大订单数量
//! - [`MaxSingleCapitalRule`]：最大单笔资金占用
//! - [`DailyLossRule`]：每日最大亏损
//! - [`ConsecutiveLossRule`]：连续亏损限制
//! - [`DrawdownRule`]：最大回撤限制
//! - [`ExposureLimitRule`]：暴露限制（市场/类别/方向）
//! - [`LiquidityRule`]：最低流动性
//! - [`SlippageRule`]：最大滑点
//! - [`VolatilityRule`]：最高波动率
//!
//! 不写死策略 —— 通过配置控制。

use crate::config::RiskConfig;
use crate::context::RiskContext;

/// 规则检查结果。
#[derive(Debug, Clone, PartialEq)]
pub enum RuleResult {
    /// 通过检查。
    Pass,
    /// 警告（不阻止，但记录）。
    Warn(String),
    /// 拒绝（阻止交易，附带中文原因）。
    Reject(String),
}

impl RuleResult {
    pub fn is_reject(&self) -> bool {
        matches!(self, RuleResult::Reject(_))
    }

    pub fn is_warn(&self) -> bool {
        matches!(self, RuleResult::Warn(_))
    }

    /// 拒绝原因文本（中文）。
    pub fn reason(&self) -> Option<&str> {
        match self {
            RuleResult::Reject(r) | RuleResult::Warn(r) => Some(r.as_str()),
            RuleResult::Pass => None,
        }
    }
}

/// 风险规则 trait：所有规则统一接口。
///
/// 每个规则接收完整的 RiskContext 和 RiskConfig，
/// 自行提取所需字段进行判断。
pub trait RiskRule: Send + Sync {
    /// 规则名称（中文）。
    fn name(&self) -> &'static str;

    /// 规则描述（中文）。
    fn description(&self) -> &'static str;

    /// 执行检查。
    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult;
}

// ============================================================================
// 内置规则实现
// ============================================================================

/// 最大持仓数量规则。
pub struct MaxPositionCountRule;

impl RiskRule for MaxPositionCountRule {
    fn name(&self) -> &'static str {
        "最大持仓数量"
    }

    fn description(&self) -> &'static str {
        "当前持仓数不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.open_position_count >= config.max_positions {
            RuleResult::Reject(format!(
                "持仓数量已达上限：当前 {} 个，上限 {} 个",
                ctx.open_position_count, config.max_positions
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 单笔持仓规模限制规则。
pub struct PositionSizeLimitRule;

impl RiskRule for PositionSizeLimitRule {
    fn name(&self) -> &'static str {
        "单笔持仓规模"
    }

    fn description(&self) -> &'static str {
        "单笔持仓金额不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.suggested_notional > config.max_position_size {
            RuleResult::Reject(format!(
                "单笔资金超过限制：建议 {:.0} USDC，上限 {:.0} USDC",
                ctx.suggested_notional, config.max_position_size
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 最大订单数量规则。
pub struct MaxOrderCountRule;

impl RiskRule for MaxOrderCountRule {
    fn name(&self) -> &'static str {
        "最大订单数量"
    }

    fn description(&self) -> &'static str {
        "待处理订单数不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.pending_order_count >= config.max_open_orders {
            RuleResult::Reject(format!(
                "待处理订单已达上限：当前 {} 个，上限 {} 个",
                ctx.pending_order_count, config.max_open_orders
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 最大单笔资金占用规则。
pub struct MaxSingleCapitalRule;

impl RiskRule for MaxSingleCapitalRule {
    fn name(&self) -> &'static str {
        "最大单笔资金占用"
    }

    fn description(&self) -> &'static str {
        "单笔资金占用不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        let total_lock = ctx.locked_cash + ctx.suggested_notional;
        let usage = total_lock / ctx.initial_capital;
        if usage > config.max_capital_usage {
            RuleResult::Reject(format!(
                "资金占用超过限制：当前占用 {:.0}%，上限 {:.0}%",
                usage * 100.0,
                config.max_capital_usage * 100.0
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 每日最大亏损规则。
pub struct DailyLossRule;

impl RiskRule for DailyLossRule {
    fn name(&self) -> &'static str {
        "每日最大亏损"
    }

    fn description(&self) -> &'static str {
        "当日已实现亏损不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if -ctx.daily_realized_pnl > config.max_daily_loss {
            RuleResult::Reject(format!(
                "当日亏损达到限制：累计亏损 {:.0} USDC，上限 {:.0} USDC",
                -ctx.daily_realized_pnl, config.max_daily_loss
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 连续亏损限制规则。
pub struct ConsecutiveLossRule;

impl RiskRule for ConsecutiveLossRule {
    fn name(&self) -> &'static str {
        "连续亏损限制"
    }

    fn description(&self) -> &'static str {
        "连续亏损次数不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.consecutive_losses >= config.max_consecutive_losses {
            RuleResult::Reject(format!(
                "连续亏损达到限制：已连续 {} 次亏损，上限 {} 次",
                ctx.consecutive_losses, config.max_consecutive_losses
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 最大回撤规则。
pub struct DrawdownRule;

impl RiskRule for DrawdownRule {
    fn name(&self) -> &'static str {
        "最大回撤"
    }

    fn description(&self) -> &'static str {
        "当前回撤不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.current_drawdown > config.max_drawdown {
            RuleResult::Reject(format!(
                "回撤超过限制：当前回撤 {:.1}%，上限 {:.1}%",
                ctx.current_drawdown * 100.0,
                config.max_drawdown * 100.0
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 暴露限制规则。
pub struct ExposureLimitRule;

impl RiskRule for ExposureLimitRule {
    fn name(&self) -> &'static str {
        "暴露限制"
    }

    fn description(&self) -> &'static str {
        "单一市场/类别/方向暴露不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        // 检查市场暴露
        let market_ratio = ctx.market_exposure / ctx.initial_capital;
        if market_ratio > config.max_market_exposure {
            return RuleResult::Reject(format!(
                "市场暴露超过限制：当前 {:.1}%，上限 {:.1}%",
                market_ratio * 100.0,
                config.max_market_exposure * 100.0
            ));
        }

        // 检查类别暴露
        let cat_ratio = ctx.category_exposure / ctx.initial_capital;
        if cat_ratio > config.max_category_exposure {
            return RuleResult::Reject(format!(
                "类别暴露超过限制：当前 {:.1}%，上限 {:.1}%",
                cat_ratio * 100.0,
                config.max_category_exposure * 100.0
            ));
        }

        // 检查方向暴露
        if let Some(side) = ctx.suggested_side {
            let side_exposure = match side {
                pm_core::Side::Buy => ctx.yes_exposure + ctx.suggested_notional,
                pm_core::Side::Sell => ctx.no_exposure + ctx.suggested_notional,
            };
            let side_ratio = side_exposure / ctx.initial_capital;
            if side_ratio > config.max_side_exposure {
                let side_zh = match side {
                    pm_core::Side::Buy => "YES",
                    pm_core::Side::Sell => "NO",
                };
                return RuleResult::Reject(format!(
                    "{}方向暴露超过限制：当前 {:.1}%，上限 {:.1}%",
                    side_zh,
                    side_ratio * 100.0,
                    config.max_side_exposure * 100.0
                ));
            }
        }

        RuleResult::Pass
    }
}

/// 最低流动性规则。
pub struct LiquidityRule;

impl RiskRule for LiquidityRule {
    fn name(&self) -> &'static str {
        "最低流动性"
    }

    fn description(&self) -> &'static str {
        "市场流动性不得低于配置下限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        if ctx.market_liquidity < config.min_liquidity {
            RuleResult::Reject(format!(
                "市场流动性不足：当前 {:.0} USDC，最低要求 {:.0} USDC",
                ctx.market_liquidity, config.min_liquidity
            ))
        } else if ctx.market_liquidity < config.min_liquidity * 2.0 {
            // 流动性偏低但尚未低于下限，发出警告
            RuleResult::Warn(format!(
                "市场流动性偏低：当前 {:.0} USDC，建议 ≥ {:.0} USDC",
                ctx.market_liquidity,
                config.min_liquidity * 2.0
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// 最大滑点规则。
pub struct SlippageRule;

impl RiskRule for SlippageRule {
    fn name(&self) -> &'static str {
        "最大滑点"
    }

    fn description(&self) -> &'static str {
        "预估滑点不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        // 用价差作为滑点代理
        if let Some(spread) = ctx.spread {
            if spread > config.max_slippage {
                return RuleResult::Reject(format!(
                    "滑点/价差过大：当前 {:.2}%，上限 {:.2}%",
                    spread * 100.0,
                    config.max_slippage * 100.0
                ));
            }
        }
        RuleResult::Pass
    }
}

/// 最高波动率规则。
pub struct VolatilityRule;

impl RiskRule for VolatilityRule {
    fn name(&self) -> &'static str {
        "最高波动率"
    }

    fn description(&self) -> &'static str {
        "市场波动率不得超过配置上限"
    }

    fn check(&self, ctx: &RiskContext, config: &RiskConfig) -> RuleResult {
        // 从机会的风险评分推算波动率估计
        if let Some(ref opp) = ctx.opportunity {
            let vol_estimate = opp.volatility_score / 100.0; // 归一化到 0~1
            if vol_estimate > config.max_volatility {
                return RuleResult::Reject(format!(
                    "波动率过高：当前估计 {:.1}%，上限 {:.1}%",
                    vol_estimate * 100.0,
                    config.max_volatility * 100.0
                ));
            }
        }
        RuleResult::Pass
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn test_config() -> RiskConfig {
        RiskConfig::default()
    }

    fn test_ctx() -> RiskContext {
        RiskContext::minimal(10000.0, 8000.0, Local::now())
    }

    #[test]
    fn max_position_count_rejects_at_limit() {
        let rule = MaxPositionCountRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.open_position_count = 10; // = max_positions
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn max_position_count_passes_below_limit() {
        let rule = MaxPositionCountRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.open_position_count = 5;
        assert_eq!(rule.check(&ctx, &config), RuleResult::Pass);
    }

    #[test]
    fn position_size_limit_rejects_oversized() {
        let rule = PositionSizeLimitRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.suggested_notional = 200.0; // > max_position_size 100
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn daily_loss_rejects_at_threshold() {
        let rule = DailyLossRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.daily_realized_pnl = -1100.0; // |loss| > max_daily_loss 1000
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn daily_loss_passes_when_ok() {
        let rule = DailyLossRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.daily_realized_pnl = -500.0;
        assert_eq!(rule.check(&ctx, &config), RuleResult::Pass);
    }

    #[test]
    fn consecutive_loss_rejects() {
        let rule = ConsecutiveLossRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.consecutive_losses = 5;
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn consecutive_loss_passes_below() {
        let rule = ConsecutiveLossRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.consecutive_losses = 3;
        assert_eq!(rule.check(&ctx, &config), RuleResult::Pass);
    }

    #[test]
    fn drawdown_rejects() {
        let rule = DrawdownRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.current_drawdown = 0.25; // > max_drawdown 0.2
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn liquidity_warns_at_margin() {
        let rule = LiquidityRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.market_liquidity = 150.0; // < min_liquidity*2 (200) but > min_liquidity (100)
        let result = rule.check(&ctx, &config);
        assert!(result.is_warn());
    }

    #[test]
    fn liquidity_rejects_below_min() {
        let rule = LiquidityRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.market_liquidity = 50.0;
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn slippage_rejects_high_spread() {
        let rule = SlippageRule;
        let config = test_config();
        let mut ctx = test_ctx();
        ctx.spread = Some(0.03); // > max_slippage 0.02
        assert!(rule.check(&ctx, &config).is_reject());
    }

    #[test]
    fn all_rules_have_names() {
        let rules: Vec<Box<dyn RiskRule>> = vec![
            Box::new(MaxPositionCountRule),
            Box::new(PositionSizeLimitRule),
            Box::new(MaxOrderCountRule),
            Box::new(MaxSingleCapitalRule),
            Box::new(DailyLossRule),
            Box::new(ConsecutiveLossRule),
            Box::new(DrawdownRule),
            Box::new(ExposureLimitRule),
            Box::new(LiquidityRule),
            Box::new(SlippageRule),
            Box::new(VolatilityRule),
        ];
        for r in &rules {
            assert!(!r.name().is_empty(), "rule {} has empty name", r.name());
            assert!(
                !r.description().is_empty(),
                "rule {} has empty description",
                r.name()
            );
        }
    }
}
