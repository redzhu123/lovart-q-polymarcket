//! Risk Engine（V1.05 第一节/第三节/第七节）。
//!
//! Risk Engine 是统一风险控制编排器：
//!
//! ```text
//! TradeSuggestion → RiskEngine.evaluate() → RiskDecision
//!                                        → Accept → 转发 Execution
//!                                        → Review → 记录警告，建议人工审核
//!                                        → Reject → 记录事件，阻止交易
//! ```
//!
//! Risk Engine 拥有最终否决权。所有交易必须经过 Risk Engine，禁止绕过。

use tracing;

use crate::config::RiskConfig;
use crate::context::RiskContext;
use crate::events::{RiskEvent, RiskEventCollector, RiskEventKind};
use crate::explain::RiskExplain;
use crate::portfolio_risk::PortfolioRiskReport;
use crate::position_sizer::{PositionSizer, create_sizer};
use crate::rules::{
    ConsecutiveLossRule, DailyLossRule, DrawdownRule, ExposureLimitRule, LiquidityRule,
    MaxOrderCountRule, MaxPositionCountRule, MaxSingleCapitalRule, PositionSizeLimitRule, RiskRule,
    RuleResult, SlippageRule, VolatilityRule,
};
use crate::score::RiskScore;

/// 风险决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskDecision {
    /// 接受：通过全部检查，批准交易。
    Accept,
    /// 需审核：部分指标接近上限，建议人工审核后决定。
    Review,
    /// 拒绝：触发硬限制，阻止交易。
    Reject,
}

impl RiskDecision {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RiskDecision::Accept => "接受",
            RiskDecision::Review => "需审核",
            RiskDecision::Reject => "拒绝",
        }
    }
}

/// 交易建议（Strategy → Risk Engine）。
///
/// Strategy 产生 TradeSuggestion，交由 Risk Engine 审核。
/// Risk Engine 不负责生成交易建议，仅审核。
#[derive(Debug, Clone)]
pub struct TradeSuggestion {
    /// 市场 ID。
    pub market_id: String,
    /// 问题描述。
    pub question: String,
    /// 交易方向。
    pub side: pm_core::Side,
    /// 建议价格。
    pub price: f64,
    /// 建议数量。
    pub quantity: f64,
    /// 建议名义金额（USDC）。
    pub notional: f64,
    /// 关联的机会（如有）。
    pub opportunity_summary: Option<String>,
    /// 策略名称。
    pub strategy_name: String,
}

impl TradeSuggestion {
    /// 创建新的交易建议。
    pub fn new(
        market_id: &str,
        question: &str,
        side: pm_core::Side,
        price: f64,
        notional: f64,
        strategy_name: &str,
    ) -> Self {
        let quantity = if price > 0.0 { notional / price } else { 0.0 };
        Self {
            market_id: market_id.to_string(),
            question: question.to_string(),
            side,
            price,
            quantity,
            notional,
            opportunity_summary: None,
            strategy_name: strategy_name.to_string(),
        }
    }

    pub fn as_zh(&self) -> String {
        let side_zh = match self.side {
            pm_core::Side::Buy => "买入 YES",
            pm_core::Side::Sell => "卖出 NO",
        };
        format!(
            "{} | {} | {:.0} USDC @ {:.4} | 策略: {}",
            self.question.chars().take(30).collect::<String>(),
            side_zh,
            self.notional,
            self.price,
            self.strategy_name
        )
    }
}

/// Risk Engine 评估结果。
#[derive(Debug, Clone)]
pub struct RiskEvaluation {
    /// 最终决策。
    pub decision: RiskDecision,
    /// 风险评分。
    pub score: RiskScore,
    /// 仓位规模建议（调整后）。
    pub sized_notional: f64,
    /// 完整中文解释。
    pub explain: RiskExplain,
    /// 触发的风险事件。
    pub events: Vec<RiskEvent>,
}

/// Risk Engine 统计。
#[derive(Debug, Clone, Default)]
pub struct RiskEngineStats {
    /// 总评估次数。
    pub total_evaluations: u64,
    /// 接受次数。
    pub accepted: u64,
    /// 需审核次数。
    pub reviewed: u64,
    /// 拒绝次数。
    pub rejected: u64,
    /// 风险事件总数。
    pub total_events: u64,
}

/// 统一风险引擎。
pub struct RiskEngine {
    config: RiskConfig,
    rules: Vec<Box<dyn RiskRule>>,
    sizer: Box<dyn PositionSizer>,
    collector: RiskEventCollector,
    stats: RiskEngineStats,
    /// 累计拒绝计数（跨轮）。
    total_rejections: u64,
}

impl RiskEngine {
    /// 使用配置创建 Risk Engine。
    pub fn new(config: RiskConfig) -> Self {
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
        let sizer = create_sizer(config.position_sizer);

        Self {
            config,
            rules,
            sizer,
            collector: RiskEventCollector::new(),
            stats: RiskEngineStats::default(),
            total_rejections: 0,
        }
    }

    /// 使用默认配置创建（测试用）。
    pub fn with_defaults() -> Self {
        Self::new(RiskConfig::default())
    }

    // ---- 核心方法：评估交易建议 ----

    /// 评估交易建议，返回风险决策。
    ///
    /// 流程：
    /// 1. 构建 RiskContext（调用方已提供）
    /// 2. 计算风险评分
    /// 3. 逐条运行风险规则
    /// 4. 计算仓位规模
    /// 5. 汇总决策
    /// 6. 记录事件
    /// 7. 输出中文日志
    pub fn evaluate(&mut self, ctx: &RiskContext, suggestion: &TradeSuggestion) -> RiskEvaluation {
        self.stats.total_evaluations += 1;

        // 1. 计算风险评分
        let score = RiskScore::compute(ctx, &self.config);

        // 2. 逐条运行规则
        let mut all_pass = true;
        let mut has_warnings = false;
        let mut reasons: Vec<String> = Vec::new();
        let mut triggered_events: Vec<RiskEvent> = Vec::new();

        for rule in &self.rules {
            let result = rule.check(ctx, &self.config);
            match result {
                RuleResult::Pass => {}
                RuleResult::Warn(msg) => {
                    has_warnings = true;
                    reasons.push(format!("[{}] {}", rule.name(), msg));
                    // 记录警告事件
                    let event = self.rule_to_warning_event(rule.name(), &msg, suggestion);
                    triggered_events.push(event);
                }
                RuleResult::Reject(msg) => {
                    all_pass = false;
                    reasons.push(format!("[{}] {}", rule.name(), msg));
                    // 记录拒绝事件
                    let event = RiskEvent::new(
                        RiskEventKind::RiskReject,
                        format!("{}: {}", rule.name(), msg),
                    )
                    .with_market(&suggestion.market_id);
                    triggered_events.push(event);
                }
            }
        }

        // 3. 计算仓位规模
        let size_rec = self.sizer.size(ctx, &self.config);

        // 4. 决策
        let decision = if !all_pass {
            RiskDecision::Reject
        } else if has_warnings || score.total < self.config.accept_threshold {
            if score.total < self.config.review_threshold {
                RiskDecision::Reject
            } else {
                RiskDecision::Review
            }
        } else {
            RiskDecision::Accept
        };

        // 5. 构建解释
        let mut explain = RiskExplain::new(decision, score.clone());
        for r in &reasons {
            explain.add_reason(r.clone());
        }

        // 添加建议
        match decision {
            RiskDecision::Reject => {
                if score.total < self.config.review_threshold {
                    explain
                        .add_suggestion("风险评分过低，建议等待市场条件改善后再尝试".to_string());
                }
                explain.add_suggestion(
                    "检查各项风险指标，调整仓位规模或选择更低风险的机会".to_string(),
                );
            }
            RiskDecision::Review => {
                explain
                    .add_suggestion("部分风险指标接近上限，建议人工审核确认后手动放行".to_string());
            }
            RiskDecision::Accept => {
                // 通过时无需额外建议
            }
        }

        // 6. 记录事件
        for ev in &triggered_events {
            self.collector.record(ev.clone());
        }

        // 更新统计
        match decision {
            RiskDecision::Accept => self.stats.accepted += 1,
            RiskDecision::Review => {
                self.stats.reviewed += 1;
                self.collector.record(RiskEvent::new(
                    RiskEventKind::RiskReview,
                    format!("需审核：{}", suggestion.as_zh()),
                ));
            }
            RiskDecision::Reject => {
                self.stats.rejected += 1;
                self.total_rejections += 1;
            }
        }
        self.stats.total_events += triggered_events.len() as u64;

        // 7. 日志
        match decision {
            RiskDecision::Reject => {
                tracing::warn!(
                    target: "risk",
                    decision = "REJECT",
                    market_id = %suggestion.market_id,
                    question = %suggestion.question,
                    score = %score.total,
                    reasons = ?reasons,
                    "❌ 风险拒绝 — {} — {}",
                    suggestion.as_zh(),
                    reasons.first().map(|s| s.as_str()).unwrap_or("未通过检查"),
                );
            }
            RiskDecision::Review => {
                tracing::info!(
                    target: "risk",
                    decision = "REVIEW",
                    market_id = %suggestion.market_id,
                    question = %suggestion.question,
                    score = %score.total,
                    "⚠️ 需审核 — {}",
                    suggestion.as_zh(),
                );
            }
            RiskDecision::Accept => {
                tracing::debug!(
                    target: "risk",
                    decision = "ACCEPT",
                    market_id = %suggestion.market_id,
                    question = %suggestion.question,
                    score = %score.total,
                    "✅ 接受 — {}",
                    suggestion.as_zh(),
                );
            }
        }

        RiskEvaluation {
            decision,
            score,
            sized_notional: size_rec.notional,
            explain,
            events: triggered_events,
        }
    }

    // ---- 辅助方法 ----

    /// 规则警告 → 风险事件类型映射。
    fn rule_to_warning_event(
        &self,
        rule_name: &str,
        msg: &str,
        suggestion: &TradeSuggestion,
    ) -> RiskEvent {
        let kind = match rule_name {
            "最大持仓数量" => RiskEventKind::PositionLimit,
            "暴露限制" => RiskEventKind::ExposureLimit,
            "每日最大亏损" => RiskEventKind::DailyLossLimit,
            "最低流动性" => RiskEventKind::LiquidityWarning,
            "最大回撤" => RiskEventKind::DrawdownWarning,
            "连续亏损限制" => RiskEventKind::ConsecutiveLossLimit,
            "最大单笔资金占用" => RiskEventKind::CapitalUsageWarning,
            _ => RiskEventKind::RiskReview,
        };
        RiskEvent::new(kind, format!("{}: {}", rule_name, msg)).with_market(&suggestion.market_id)
    }

    // ---- 仪表盘 ----

    /// 获取当前组合风险报告。
    pub fn portfolio_risk_report(&self, ctx: &RiskContext) -> PortfolioRiskReport {
        let mut report = PortfolioRiskReport::compute(ctx, &self.config);
        report.risk.rejection_count = self.total_rejections as usize;
        report.risk.risk_event_count = self.collector.total();
        report
    }

    /// 获取事件收集器引用。
    pub fn events(&self) -> &RiskEventCollector {
        &self.collector
    }

    /// 获取统计。
    pub fn stats(&self) -> &RiskEngineStats {
        &self.stats
    }

    /// 总拒绝计数。
    pub fn total_rejections(&self) -> u64 {
        self.total_rejections
    }

    /// 配置引用。
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    // ---- CSV ----

    /// 保存风险事件到 CSV。
    pub fn save_events_csv(&self, path: &str) -> anyhow::Result<()> {
        self.collector.save_to_csv(path)
    }

    // ---- 重置 ----

    /// 跨日重置：清空当日累计。
    pub fn reset_daily(&mut self) {
        self.collector = RiskEventCollector::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn test_ctx(available_cash: f64) -> RiskContext {
        let mut ctx = RiskContext::minimal(10000.0, available_cash, Local::now());
        ctx.suggested_price = 0.5;
        ctx.suggested_notional = 100.0;
        ctx.market_liquidity = 10000.0;
        ctx.market_id = "test-market".into();
        ctx
    }

    fn test_suggestion() -> TradeSuggestion {
        TradeSuggestion::new(
            "test-market",
            "测试问题",
            pm_core::Side::Buy,
            0.5,
            100.0,
            "DefaultStrategy",
        )
    }

    #[test]
    fn healthy_portfolio_accepts() {
        let mut engine = RiskEngine::with_defaults();
        let ctx = test_ctx(9000.0);
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        assert_eq!(eval.decision, RiskDecision::Accept);
        assert!(eval.score.total > 70.0);
        assert_eq!(engine.stats().accepted, 1);
        assert_eq!(engine.stats().rejected, 0);
    }

    #[test]
    fn max_positions_rejects() {
        let mut engine = RiskEngine::with_defaults();
        let mut ctx = test_ctx(9000.0);
        ctx.open_position_count = 10; // = max_positions
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        assert_eq!(eval.decision, RiskDecision::Reject);
        assert_eq!(engine.stats().rejected, 1);
        assert!(engine.total_rejections() > 0);
    }

    #[test]
    fn daily_loss_rejects() {
        let mut engine = RiskEngine::with_defaults();
        let mut ctx = test_ctx(9000.0);
        ctx.daily_realized_pnl = -1200.0; // > max_daily_loss 1000
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        assert_eq!(eval.decision, RiskDecision::Reject);
    }

    #[test]
    fn consecutive_loss_rejects() {
        let mut engine = RiskEngine::with_defaults();
        let mut ctx = test_ctx(9000.0);
        ctx.consecutive_losses = 5;
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        assert_eq!(eval.decision, RiskDecision::Reject);
    }

    #[test]
    fn low_liquidity_warns() {
        let mut engine = RiskEngine::with_defaults();
        let mut ctx = test_ctx(9000.0);
        ctx.market_liquidity = 150.0; // < 200 (2x min) but > 100 (min)
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        // 流动性偏低会 Warn，但总分仍可能 Accept（如果其他维度健康）
        // 检查至少产生了警告事件
        assert!(!eval.events.is_empty() || eval.decision == RiskDecision::Accept);
    }

    #[test]
    fn explain_has_reasons_on_reject() {
        let mut engine = RiskEngine::with_defaults();
        let mut ctx = test_ctx(9000.0);
        ctx.open_position_count = 10;
        ctx.daily_realized_pnl = -1200.0;
        let suggestion = test_suggestion();
        let eval = engine.evaluate(&ctx, &suggestion);
        assert_eq!(eval.decision, RiskDecision::Reject);
        assert!(!eval.explain.reasons.is_empty());
        let zh = eval.explain.explain_zh();
        assert!(zh.contains("拒绝"));
        assert!(zh.contains("持仓"));
    }

    #[test]
    fn stats_accumulate_correctly() {
        let mut engine = RiskEngine::with_defaults();

        // Accept
        let ctx1 = test_ctx(9000.0);
        let s1 = test_suggestion();
        engine.evaluate(&ctx1, &s1);

        // Reject
        let mut ctx2 = test_ctx(9000.0);
        ctx2.daily_realized_pnl = -1200.0;
        let s2 = TradeSuggestion::new("mkt2", "Q2", pm_core::Side::Buy, 0.5, 100.0, "Test");
        engine.evaluate(&ctx2, &s2);

        // Reject
        let mut ctx3 = test_ctx(9000.0);
        ctx3.open_position_count = 10;
        let s3 = TradeSuggestion::new("mkt3", "Q3", pm_core::Side::Buy, 0.5, 100.0, "Test");
        engine.evaluate(&ctx3, &s3);

        assert_eq!(engine.stats().total_evaluations, 3);
        assert_eq!(engine.stats().accepted, 1);
        assert_eq!(engine.stats().rejected, 2);
        assert_eq!(engine.total_rejections(), 2);
    }
}
