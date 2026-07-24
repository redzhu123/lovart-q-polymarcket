//! Risk Explain（V1.05 第八节）。
//!
//! 所有风险决策附带中文解释。
//! 支持 `cargo run -- explain-risk` 查看。

use crate::engine::RiskDecision;
use crate::score::RiskScore;

/// 风险解释：包含决策、评分、逐条理由。
#[derive(Debug, Clone)]
pub struct RiskExplain {
    /// 决策结果。
    pub decision: RiskDecision,
    /// 风险评分。
    pub score: RiskScore,
    /// 逐条理由（中文）。
    pub reasons: Vec<String>,
    /// 建议措施（中文）。
    pub suggestions: Vec<String>,
}

impl RiskExplain {
    pub fn new(decision: RiskDecision, score: RiskScore) -> Self {
        Self {
            decision,
            score,
            reasons: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// 添加一条拒绝/警告理由。
    pub fn add_reason(&mut self, reason: String) {
        self.reasons.push(reason);
    }

    /// 添加一条建议。
    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }

    /// 中文完整解释。
    pub fn explain_zh(&self) -> String {
        let mut lines = Vec::new();

        lines.push("【风险审核结果】".to_string());
        lines.push(String::new());

        // 决策
        let decision_zh = match self.decision {
            RiskDecision::Accept => "✅ 接受",
            RiskDecision::Review => "⚠️ 需审核",
            RiskDecision::Reject => "❌ 拒绝",
        };
        lines.push(format!("  决策：{}", decision_zh));
        lines.push(String::new());

        // 评分
        lines.push(format!("  {}", self.score.summary_zh()));
        lines.push(String::new());

        // 理由
        if !self.reasons.is_empty() {
            lines.push("  理由：".to_string());
            for r in &self.reasons {
                lines.push(format!("    - {}", r));
            }
            lines.push(String::new());
        }

        // 建议
        if !self.suggestions.is_empty() {
            lines.push("  建议：".to_string());
            for s in &self.suggestions {
                lines.push(format!("    - {}", s));
            }
            lines.push(String::new());
        }

        // 通过时
        if self.decision == RiskDecision::Accept && self.reasons.is_empty() {
            lines.push("  全部风险检查通过，交易建议已批准。".to_string());
        }

        lines.join("\n")
    }

    /// 简短单行解释。
    pub fn one_line_zh(&self) -> String {
        match self.decision {
            RiskDecision::Accept => format!("✅ 接受 — 风险评分 {:.0}/100", self.score.total),
            RiskDecision::Review => {
                let reason = self
                    .reasons
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("需人工审核");
                format!(
                    "⚠️ 需审核 — {} — 风险评分 {:.0}/100",
                    reason, self.score.total
                )
            }
            RiskDecision::Reject => {
                let reason = self
                    .reasons
                    .first()
                    .map(|s| s.as_str())
                    .unwrap_or("未通过风险检查");
                format!(
                    "❌ 拒绝 — {} — 风险评分 {:.0}/100",
                    reason, self.score.total
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RiskConfig;
    use crate::context::RiskContext;
    use crate::score::RiskScore;
    use chrono::Local;

    #[test]
    fn explain_accept_contains_score() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        let explain = RiskExplain::new(RiskDecision::Accept, score);
        let zh = explain.explain_zh();
        assert!(zh.contains("接受"));
        assert!(zh.contains("风险评分"));
    }

    #[test]
    fn explain_reject_with_reasons() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);
        let mut explain = RiskExplain::new(RiskDecision::Reject, score);
        explain.add_reason("连续亏损达到限制".to_string());
        explain.add_reason("市场流动性不足".to_string());
        explain.add_suggestion("暂停交易30分钟后重试".to_string());
        let zh = explain.explain_zh();
        assert!(zh.contains("拒绝"));
        assert!(zh.contains("连续亏损达到限制"));
        assert!(zh.contains("市场流动性不足"));
        assert!(zh.contains("暂停交易30分钟后重试"));
    }

    #[test]
    fn one_line_zh_all_variants() {
        let ctx = RiskContext::minimal(10000.0, 10000.0, Local::now());
        let config = RiskConfig::default();
        let score = RiskScore::compute(&ctx, &config);

        let accept = RiskExplain::new(RiskDecision::Accept, score.clone());
        assert!(accept.one_line_zh().contains("接受"));

        let mut review = RiskExplain::new(RiskDecision::Review, score.clone());
        review.add_reason("资金利用率偏高".to_string());
        assert!(review.one_line_zh().contains("需审核"));

        let mut reject = RiskExplain::new(RiskDecision::Reject, score.clone());
        reject.add_reason("回撤超过限制".to_string());
        assert!(reject.one_line_zh().contains("拒绝"));
    }
}
