//! 解释引擎（V1.04 第十二节）。
//!
//! 对任意 Opportunity 生成中文解释：为什么评分=92？为什么风险=23？为什么优先级第一？

use crate::model::Opportunity;

/// 解释引擎：为 Opportunity 生成中文评分分解。
pub struct ExplainEngine;

impl ExplainEngine {
    /// 生成中文解释文本。
    pub fn explain(opp: &Opportunity) -> String {
        let mut lines: Vec<String> = Vec::new();

        // 标题
        lines.push("========================================".into());
        lines.push(format!("机会解释：{}", opp.id));
        lines.push("========================================".into());
        lines.push(String::new());

        // 基本信息
        lines.push(format!("  市场     : {}", opp.market_id));
        lines.push(format!("  问题     : {}", opp.question));
        lines.push(format!("  类型     : {}", opp.opportunity_type.as_zh()));
        lines.push(format!("  状态     : {}", opp.status.as_zh()));
        lines.push(format!("  数据来源 : {}", opp.provider));
        lines.push(String::new());

        // 市场数据
        lines.push("── 市场数据 ──".into());
        lines.push(format!("  YES 价格 : {:.4}", opp.yes_price));
        lines.push(format!("  NO  价格 : {:.4}", opp.no_price));
        lines.push(format!(
            "  SUM      : {:.4}  (1.0 - SUM = {:.4})",
            opp.sum,
            1.0 - opp.sum
        ));
        if let Some(spread) = opp.spread {
            lines.push(format!("  价差     : {:.4}", spread));
        } else {
            lines.push("  价差     : 无数据".into());
        }
        lines.push(format!("  成交量   : {:.2}", opp.volume));
        lines.push(format!("  流动性   : {:.2}", opp.liquidity));
        if let Some(bd) = opp.bid_depth {
            lines.push(format!("  买盘深度 : {:.2}", bd));
        }
        if let Some(ad) = opp.ask_depth {
            lines.push(format!("  卖盘深度 : {:.2}", ad));
        }
        lines.push(String::new());

        // 评分分解
        lines.push("── 评分分解 ──".into());
        lines.push(format!("  综合评分 : {:.1} / 100", opp.score));
        lines.push(format!("  置信度   : {:.0}%", opp.confidence * 100.0));
        lines.push(format!("  优先级   : {} / 100", opp.priority));
        lines.push(String::new());

        // 各维度
        lines.push("  维度贡献：".into());
        lines.push(format!(
            "    价差     : +{:.1}  (权重 25%%, 贡献 +{:.1})",
            opp.spread_score,
            opp.spread_score * 0.25
        ));
        lines.push(format!(
            "    流动性   : +{:.1}  (权重 20%%, 贡献 +{:.1})",
            opp.liquidity_score,
            opp.liquidity_score * 0.20
        ));
        lines.push(format!(
            "    深度     : +{:.1}  (权重 20%%, 贡献 +{:.1})",
            opp.depth_score,
            opp.depth_score * 0.20
        ));
        lines.push(format!(
            "    成交量   : +{:.1}  (权重 15%%, 贡献 +{:.1})",
            opp.volume_score,
            opp.volume_score * 0.15
        ));
        lines.push(format!(
            "    波动率   : +{:.1}  (权重 10%%, 贡献 +{:.1})",
            opp.volatility_score,
            opp.volatility_score * 0.10
        ));
        lines.push(format!(
            "    置信度   : +{:.1}  (权重 20%%, 贡献 +{:.1})",
            opp.confidence * 100.0,
            opp.confidence * 100.0 * 0.20
        ));
        lines.push(format!(
            "    风险扣分 : -{:.1}  (权重 10%%, 贡献 -{:.1})",
            opp.risk_score,
            opp.risk_score * 0.10
        ));
        lines.push(String::new());

        // 收益预估
        lines.push("── 收益预估 ──".into());
        lines.push(format!("  预期收益率 : {:.2}%", opp.expected_roi * 100.0));
        lines.push(format!(
            "  预期利润   : {:.2} USDC (按 100 USDC 名义本金估算)",
            opp.expected_profit
        ));
        lines.push(String::new());

        // 判定理由
        lines.push("── 类型判定 ──".into());
        lines.push(format!("  类型       : {}", opp.opportunity_type.as_zh()));
        lines.push(match opp.opportunity_type {
            crate::model::OpportunityType::Arbitrage => {
                format!(
                    "  原因       : SUM = {:.4}，低于 0.90，存在显著套利空间",
                    opp.sum
                )
            }
            crate::model::OpportunityType::Spread => {
                format!(
                    "  原因       : SUM = {:.4}，在 [0.90, 0.98] 区间，价差套利",
                    opp.sum
                )
            }
            crate::model::OpportunityType::PriceGap => {
                format!(
                    "  原因       : 价差 = {:.4}，超过 0.05 阈值",
                    opp.spread.unwrap_or(0.0)
                )
            }
            crate::model::OpportunityType::Liquidity => {
                format!(
                    "  原因       : 流动性 = {:.2}，超过 10,000 阈值",
                    opp.liquidity
                )
            }
            _ => "  原因       : 综合判定（数据不足以归入特定类型）".into(),
        });
        lines.push(String::new());

        // 优先级解释
        lines.push("── 优先级判定 ──".into());
        if opp.priority >= 80 {
            lines.push(format!(
                "  高优先级（{}）：评分 {:.0} + 置信度 {:.0}%，综合表现优异",
                opp.priority,
                opp.score,
                opp.confidence * 100.0
            ));
        } else if opp.priority >= 50 {
            lines.push(format!(
                "  中等优先级（{}）：各项指标中等，可选择性参与",
                opp.priority
            ));
        } else {
            lines.push(format!(
                "  低优先级（{}）：评分或置信度偏低，建议观望",
                opp.priority
            ));
        }
        lines.push(String::new());

        lines.push("========================================".into());

        lines.join("\n")
    }

    /// 生成简短摘要（用于列表中的提示）。
    pub fn summary_reason(opp: &Opportunity) -> String {
        match opp.opportunity_type {
            crate::model::OpportunityType::Arbitrage => {
                format!(
                    "套利：SUM={:.4}，偏离 1.0 达 {:.2}%",
                    opp.sum,
                    (1.0 - opp.sum) * 100.0
                )
            }
            crate::model::OpportunityType::Spread => {
                format!(
                    "价差：SUM={:.4}，价差空间 {:.2}%",
                    opp.sum,
                    (1.0 - opp.sum) * 100.0
                )
            }
            crate::model::OpportunityType::PriceGap => {
                format!("价格缺口：价差 = {:.4}", opp.spread.unwrap_or(0.0))
            }
            crate::model::OpportunityType::Liquidity => {
                format!("流动性：流动性 = {:.2}", opp.liquidity)
            }
            _ => format!("未知类型：评分 = {:.0}", opp.score),
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OpportunityType;
    use chrono::Utc;

    fn make_opp(opp_type: OpportunityType, sum: f64, spread: Option<f64>) -> Opportunity {
        Opportunity::new(
            "m1".into(),
            "测试问题".into(),
            "test".into(),
            Utc::now(),
            opp_type,
            85.0,
            0.9,
            85,
            25.0,
            20.0,
            18.0,
            12.0,
            5.0,
            5.0,
            0.02,
            2.0,
            0.42,
            0.50,
            sum,
            spread,
            5000.0,
            8000.0,
            Some(2000.0),
            Some(2500.0),
        )
    }

    #[test]
    fn explain_contains_all_sections() {
        let opp = make_opp(OpportunityType::Arbitrage, 0.85, Some(0.08));
        let text = ExplainEngine::explain(&opp);
        assert!(text.contains("机会解释"));
        assert!(text.contains("市场数据"));
        assert!(text.contains("评分分解"));
        assert!(text.contains("收益预估"));
        assert!(text.contains("类型判定"));
        assert!(text.contains("优先级判定"));
        assert!(text.contains("套利"));
    }

    #[test]
    fn explain_for_spread_type() {
        let opp = make_opp(OpportunityType::Spread, 0.95, Some(0.03));
        let text = ExplainEngine::explain(&opp);
        assert!(text.contains("价差套利"));
    }

    #[test]
    fn summary_reason_is_not_empty() {
        let opp = make_opp(OpportunityType::Arbitrage, 0.85, Some(0.08));
        let reason = ExplainEngine::summary_reason(&opp);
        assert!(!reason.is_empty());
        assert!(reason.contains("SUM"));
    }

    #[test]
    fn explain_priority_rationale() {
        let high = make_opp(OpportunityType::Arbitrage, 0.85, Some(0.08));
        // high priority opp
        let text = ExplainEngine::explain(&high);
        assert!(
            text.contains("高优先级") || text.contains("中等优先级") || text.contains("低优先级")
        );
    }
}
