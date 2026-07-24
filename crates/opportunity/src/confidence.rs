//! 置信度引擎（V1.04 第五节）。
//!
//! 根据数据完整度、盘口质量、流动性、历史一致性计算置信度（0.0~1.0）。
//! 方便以后接入 AI 决策。

use pm_models::UnifiedMarket;

/// 置信度引擎。
///
/// 综合以下因素：
/// - 数据完整度（有 YES/NO 价格）→ 最高 +0.30
/// - 订单簿可用（有 bid/ask）→ 最高 +0.25
/// - 流动性充分 → 最高 +0.20
/// - 价差异常（偏离 1.0 越远）→ 最高 +0.15
/// - 历史一致性（多轮持续）→ 最高 +0.10
pub struct ConfidenceEngine;

impl ConfidenceEngine {
    /// 创建新的置信度引擎。
    pub fn new() -> Self {
        Self
    }

    /// 计算置信度（0.0~1.0）。
    ///
    /// `historical_rounds`：该机会在历史中持续出现的轮数（新机会为 0）。
    /// `has_orderbook`：是否有订单簿数据可用。
    pub fn evaluate(
        &self,
        market: &UnifiedMarket,
        has_orderbook: bool,
        has_bid_ask: bool,
        historical_rounds: u64,
    ) -> f64 {
        let mut confidence = 0.0f64;

        // 1. 数据完整度（最高 +0.30）
        confidence += Self::data_completeness(market);

        // 2. 订单簿可用（最高 +0.25）
        confidence += Self::orderbook_quality(has_orderbook, has_bid_ask);

        // 3. 流动性充分（最高 +0.20）
        confidence += Self::liquidity_factor(market.liquidity);

        // 4. 价差异常（最高 +0.15）
        if let Some((y, n)) = market.yes_no_prices() {
            confidence += Self::spread_anomaly(y + n);
        }

        // 5. 历史一致性（最高 +0.10）
        confidence += Self::historical_consistency(historical_rounds);

        confidence.clamp(0.0, 1.0)
    }

    /// 数据完整度：YES/NO 均有价格则满分。
    fn data_completeness(market: &UnifiedMarket) -> f64 {
        let mut score = 0.0;

        // 有 YES 价格
        if market.yes_price.is_some() {
            score += 0.10;
        }
        // 有 NO 价格
        if market.no_price.is_some() {
            score += 0.10;
        }
        // 是二元市场
        if market.outcome_count == 2 {
            score += 0.05;
        }
        // 问题非空
        if !market.question.is_empty() {
            score += 0.05;
        }

        score
    }

    /// 订单簿质量。
    fn orderbook_quality(has_orderbook: bool, has_bid_ask: bool) -> f64 {
        let mut score = 0.0;
        if has_orderbook {
            score += 0.15;
        }
        if has_bid_ask {
            score += 0.10;
        }
        score
    }

    /// 流动性因素（对数映射到 0~0.20）。
    fn liquidity_factor(liquidity: f64) -> f64 {
        if liquidity <= 0.0 {
            return 0.0;
        }
        let log_val = (liquidity + 1.0).log10();
        // log10(1_000_000 + 1) ≈ 6.0
        (log_val / 6.0 * 0.20).clamp(0.0, 0.20)
    }

    /// 价差异常（SUM 偏离 1.0 越远，置信度越高——说明越可能是真实机会而非数据错误）。
    fn spread_anomaly(sum: f64) -> f64 {
        let dev = (1.0 - sum).abs();
        if dev <= 0.01 {
            return 0.0; // 太正常了，可能是归一化数据
        }
        // dev 在 0.01~0.50 之间线性映射到 0~0.15
        ((dev - 0.01) / 0.49 * 0.15).clamp(0.0, 0.15)
    }

    /// 历史一致性：持续轮数越多置信度越高（对数增长，上限 +0.10）。
    fn historical_consistency(rounds: u64) -> f64 {
        if rounds == 0 {
            return 0.0;
        }
        let log_val = (rounds as f64 + 1.0).log10();
        // log10(100 + 1) ≈ 2.0
        (log_val / 2.0 * 0.10).clamp(0.0, 0.10)
    }
}

impl Default for ConfidenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn engine() -> ConfidenceEngine {
        ConfidenceEngine::new()
    }

    fn test_market(
        yes: Option<f64>,
        no: Option<f64>,
        liquidity: f64,
        outcome_count: usize,
    ) -> UnifiedMarket {
        UnifiedMarket {
            market_id: "m1".into(),
            question: "测试问题".into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: yes,
            no_price: no,
            volume: 1000.0,
            liquidity,
            category: None,
            outcome_count,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn full_data_gives_high_confidence() {
        let m = test_market(Some(0.40), Some(0.55), 50_000.0, 2);
        let c = engine().evaluate(&m, true, true, 10);
        assert!(c > 0.6, "Rich data → high confidence, got {c}");
    }

    #[test]
    fn poor_data_gives_low_confidence() {
        let m = test_market(None, None, 0.0, 0);
        let c = engine().evaluate(&m, false, false, 0);
        assert!(c < 0.3, "Poor data → low confidence, got {c}");
    }

    #[test]
    fn confidence_in_range() {
        let m = test_market(Some(0.5), Some(0.5), 1000.0, 2);
        let c = engine().evaluate(&m, false, false, 0);
        assert!(c >= 0.0 && c <= 1.0, "Confidence must be in [0,1], got {c}");
    }

    #[test]
    fn historical_rounds_increase_confidence() {
        let m = test_market(Some(0.40), Some(0.50), 1000.0, 2);
        let c0 = engine().evaluate(&m, false, false, 0);
        let c10 = engine().evaluate(&m, false, false, 10);
        assert!(c10 >= c0, "More rounds → same or higher confidence");
    }

    #[test]
    fn spread_anomaly_increases_confidence() {
        // SUM=0.90 → 偏离 0.10
        let m_low = test_market(Some(0.40), Some(0.50), 1000.0, 2); // SUM=0.90
        let m_normal = test_market(Some(0.50), Some(0.50), 1000.0, 2); // SUM=1.00
        let c_low = engine().evaluate(&m_low, false, false, 0);
        let c_normal = engine().evaluate(&m_normal, false, false, 0);
        assert!(c_low > c_normal, "Abnormal spread → higher confidence");
    }
}
