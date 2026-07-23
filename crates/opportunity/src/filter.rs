//! 统一机会过滤器（V1.04 第九节）。
//!
//! 所有过滤规则集中在 Engine 层，Strategy 不再自行过滤。

use crate::model::Opportunity;

/// 过滤条件配置。
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// 最低流动性。
    pub min_liquidity: f64,
    /// 最低买盘深度。
    pub min_bid_depth: f64,
    /// 最低卖盘深度。
    pub min_ask_depth: f64,
    /// 最低成交量。
    pub min_volume: f64,
    /// 最低评分。
    pub min_score: f64,
    /// 最低置信度（0~1）。
    pub min_confidence: f64,
    /// 最高风险分数（超过则排除）。
    pub max_risk: f64,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_liquidity: 0.0,
            min_bid_depth: 0.0,
            min_ask_depth: 0.0,
            min_volume: 0.0,
            min_score: 20.0,
            min_confidence: 0.1,
            max_risk: 100.0,
        }
    }
}

/// 机会过滤器。
pub struct OpportunityFilter {
    config: FilterConfig,
}

impl OpportunityFilter {
    /// 使用默认配置创建。
    pub fn new() -> Self {
        Self {
            config: FilterConfig::default(),
        }
    }

    /// 使用自定义配置创建。
    pub fn with_config(config: FilterConfig) -> Self {
        Self { config }
    }

    /// 判断一个机会是否通过所有过滤条件。
    pub fn accept(&self, opp: &Opportunity) -> bool {
        // 最低流动性
        if opp.liquidity < self.config.min_liquidity {
            return false;
        }

        // 最低买盘深度
        if let Some(bd) = opp.bid_depth {
            if bd < self.config.min_bid_depth {
                return false;
            }
        }

        // 最低卖盘深度
        if let Some(ad) = opp.ask_depth {
            if ad < self.config.min_ask_depth {
                return false;
            }
        }

        // 最低成交量
        if opp.volume < self.config.min_volume {
            return false;
        }

        // 最低评分
        if opp.score < self.config.min_score {
            return false;
        }

        // 最低置信度
        if opp.confidence < self.config.min_confidence {
            return false;
        }

        // 最高风险
        if opp.risk_score > self.config.max_risk {
            return false;
        }

        true
    }

    /// 过滤一批机会，返回通过的和被拒绝的。
    pub fn filter(&self, opps: &[Opportunity]) -> FilterResult {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();

        for opp in opps {
            if self.accept(opp) {
                accepted.push(opp.clone());
            } else {
                rejected.push(RejectedOpportunity {
                    id: opp.id.clone(),
                    question: opp.question.clone(),
                    reason: self.rejection_reason(opp),
                });
            }
        }

        FilterResult { accepted, rejected }
    }

    /// 获取拒绝原因（用于日志）。
    pub fn rejection_reason(&self, opp: &Opportunity) -> String {
        if opp.liquidity < self.config.min_liquidity {
            format!(
                "流动性不足（{:.2} < {:.2}）",
                opp.liquidity, self.config.min_liquidity
            )
        } else if opp.score < self.config.min_score {
            format!("评分过低（{:.1} < {:.1}）", opp.score, self.config.min_score)
        } else if opp.confidence < self.config.min_confidence {
            format!(
                "置信度不足（{:.0}% < {:.0}%）",
                opp.confidence * 100.0,
                self.config.min_confidence * 100.0
            )
        } else if opp.risk_score > self.config.max_risk {
            format!(
                "风险过高（{:.1} > {:.1}）",
                opp.risk_score, self.config.max_risk
            )
        } else if opp.volume < self.config.min_volume {
            format!(
                "成交量不足（{:.2} < {:.2}）",
                opp.volume, self.config.min_volume
            )
        } else {
            "未知原因".into()
        }
    }
}

impl Default for OpportunityFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// 被拒绝的机会记录。
#[derive(Debug, Clone)]
pub struct RejectedOpportunity {
    pub id: String,
    pub question: String,
    pub reason: String,
}

/// 过滤结果。
#[derive(Debug, Clone)]
pub struct FilterResult {
    pub accepted: Vec<Opportunity>,
    pub rejected: Vec<RejectedOpportunity>,
}

impl FilterResult {
    /// 通过数。
    pub fn accepted_count(&self) -> usize {
        self.accepted.len()
    }

    /// 拒绝数。
    pub fn rejected_count(&self) -> usize {
        self.rejected.len()
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

    fn make_opp(score: f64, confidence: f64, liquidity: f64, risk: f64) -> Opportunity {
        Opportunity::new(
            "m1".into(), "Q".into(), "test".into(), Utc::now(),
            OpportunityType::Unknown,
            score, confidence, (score * confidence) as u8,
            score * 0.25, score * 0.20, 0.0, 0.0, 0.0, risk,
            0.01, 1.0,
            0.5, 0.5, 1.0,
            None, 1000.0, liquidity,
            None, None,
        )
    }

    #[test]
    fn default_filter_accepts_good_opportunity() {
        let filter = OpportunityFilter::new();
        let opp = make_opp(80.0, 0.9, 5000.0, 10.0);
        assert!(filter.accept(&opp));
    }

    #[test]
    fn filter_rejects_low_score() {
        let filter = OpportunityFilter::new();
        let opp = make_opp(10.0, 0.9, 5000.0, 10.0);
        assert!(!filter.accept(&opp));
    }

    #[test]
    fn filter_rejects_low_confidence() {
        let filter = OpportunityFilter::new();
        let opp = make_opp(80.0, 0.05, 5000.0, 10.0);
        assert!(!filter.accept(&opp));
    }

    #[test]
    fn filter_rejects_high_risk() {
        let mut config = FilterConfig::default();
        config.max_risk = 50.0;
        let filter = OpportunityFilter::with_config(config);
        let opp = make_opp(80.0, 0.9, 5000.0, 80.0);
        assert!(!filter.accept(&opp));
    }

    #[test]
    fn filter_result_counts_are_correct() {
        let filter = OpportunityFilter::new();
        let opps = vec![
            make_opp(90.0, 0.9, 5000.0, 10.0), // accepted
            make_opp(10.0, 0.9, 5000.0, 10.0), // rejected: low score
            make_opp(80.0, 0.05, 5000.0, 10.0), // rejected: low confidence
        ];
        let result = filter.filter(&opps);
        assert_eq!(result.accepted_count(), 1);
        assert_eq!(result.rejected_count(), 2);
    }

    #[test]
    fn rejection_reason_is_descriptive() {
        let filter = OpportunityFilter::new();
        let opp = make_opp(10.0, 0.9, 5000.0, 10.0);
        let reason = filter.rejection_reason(&opp);
        assert!(reason.contains("评分过低"));
    }
}
