//! 统一评分引擎（V1.04 第四节）。
//!
//! 综合评分 = 各维度加权求和，最终 clamp 到 0~100。
//! Strategy 不得自行计算评分——统一由本模块计算。

/// 评分维度权重配置。
#[derive(Debug, Clone)]
pub struct ScoreWeights {
    /// Spread 价差权重（默认 0.25）。
    pub spread: f64,
    /// Liquidity 流动性权重（默认 0.20）。
    pub liquidity: f64,
    /// Depth 深度权重（默认 0.20）。
    pub depth: f64,
    /// Volume 成交量权重（默认 0.15）。
    pub volume: f64,
    /// Volatility 波动率权重（默认 0.10）。
    pub volatility: f64,
    /// Confidence 置信度权重（默认 0.20）。
    pub confidence: f64,
    /// Risk 风险扣分权重（默认 0.10，从总分中减去）。
    pub risk_penalty: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            spread: 0.25,
            liquidity: 0.20,
            depth: 0.20,
            volume: 0.15,
            volatility: 0.10,
            confidence: 0.20,
            risk_penalty: 0.10,
        }
    }
}

/// 综合评分器。
///
/// 计算各维度子分数，然后加权求和得到 0~100 的总分。
pub struct OpportunityScore {
    weights: ScoreWeights,
}

impl OpportunityScore {
    /// 使用默认权重创建。
    pub fn new() -> Self {
        Self {
            weights: ScoreWeights::default(),
        }
    }

    /// 使用自定义权重创建。
    pub fn with_weights(weights: ScoreWeights) -> Self {
        Self { weights }
    }

    /// 计算价差分数（0~100）。
    ///
    /// SUM = YES + NO，偏离 1.0 越多分数越高。
    /// - SUM ≤ 0.80 → 100
    /// - SUM ≥ 1.00 → 0
    /// - 中间线性映射
    pub fn compute_spread_score(sum: f64) -> f64 {
        if sum >= 1.0 {
            return 0.0;
        }
        if sum <= 0.80 {
            return 100.0;
        }
        ((1.0 - sum) / 0.20 * 100.0).clamp(0.0, 100.0)
    }

    /// 计算流动性分数（0~100，对数映射）。
    ///
    /// `log10(liquidity + 1)` 映射到 0~100。
    /// - liquidity ≥ 100_000 → ~100
    /// - liquidity = 0 → 0
    pub fn compute_liquidity_score(liquidity: f64) -> f64 {
        if liquidity <= 0.0 {
            return 0.0;
        }
        let log_val = (liquidity + 1.0).log10();
        // log10(100_000 + 1) ≈ 5.0
        (log_val / 5.0 * 100.0).clamp(0.0, 100.0)
    }

    /// 计算深度分数（0~100，对数映射）。
    ///
    /// 取 bid_depth 与 ask_depth 的平均值，对数映射到 0~100。
    pub fn compute_depth_score(bid_depth: Option<f64>, ask_depth: Option<f64>) -> f64 {
        let bid = bid_depth.unwrap_or(0.0);
        let ask = ask_depth.unwrap_or(0.0);
        let avg = (bid + ask) / 2.0;
        if avg <= 0.0 {
            return 0.0;
        }
        let log_val = (avg + 1.0).log10();
        (log_val / 5.0 * 100.0).clamp(0.0, 100.0)
    }

    /// 计算成交量分数（0~100，对数映射）。
    pub fn compute_volume_score(volume: f64) -> f64 {
        if volume <= 0.0 {
            return 0.0;
        }
        let log_val = (volume + 1.0).log10();
        (log_val / 5.0 * 100.0).clamp(0.0, 100.0)
    }

    /// 计算波动率分数（0~100）。
    ///
    /// YES 价格偏离 0.5 越远，波动率越高（分数越低）。
    /// - yes_price = 0.5 → 100（中性）
    /// - yes_price → 0 或 1 → 0（极端）
    pub fn compute_volatility_score(yes_price: f64) -> f64 {
        let dev = (yes_price - 0.5).abs();
        ((1.0 - dev * 2.0) * 100.0).clamp(0.0, 100.0)
    }

    /// 计算风险分数（0~100，越低越好）。
    ///
    /// 综合以下因素：
    /// - 价差过大（SUM < 0.90 → 高风险）
    /// - 流动性不足
    /// - 缺乏深度
    pub fn compute_risk_score(
        sum: f64,
        liquidity_score: f64,
        depth_score: f64,
    ) -> f64 {
        let mut risk = 0.0f64;

        // SUM 异常（偏离 1.0 超过 0.1）
        let sum_dev = (1.0 - sum).abs();
        if sum_dev > 0.1 {
            risk += ((sum_dev - 0.1) / 0.4 * 40.0).min(40.0);
        }

        // 流动性不足
        if liquidity_score < 30.0 {
            risk += (30.0 - liquidity_score) * 0.5;
        }

        // 深度不足
        if depth_score < 30.0 {
            risk += (30.0 - depth_score) * 0.5;
        }

        risk.clamp(0.0, 100.0)
    }

    /// 计算综合评分及各维度分项。
    ///
    /// 返回 `(total_score, spread, liquidity, depth, volume, volatility, risk)`。
    pub fn compute(
        &self,
        sum: f64,
        yes_price: f64,
        liquidity: f64,
        volume: f64,
        bid_depth: Option<f64>,
        ask_depth: Option<f64>,
        confidence: f64,
    ) -> ScoreResult {
        let spread_s = Self::compute_spread_score(sum);
        let liquidity_s = Self::compute_liquidity_score(liquidity);
        let depth_s = Self::compute_depth_score(bid_depth, ask_depth);
        let volume_s = Self::compute_volume_score(volume);
        let volatility_s = Self::compute_volatility_score(yes_price);
        let risk_s = Self::compute_risk_score(sum, liquidity_s, depth_s);

        let w = &self.weights;
        let total = spread_s * w.spread
            + liquidity_s * w.liquidity
            + depth_s * w.depth
            + volume_s * w.volume
            + volatility_s * w.volatility
            + confidence * 100.0 * w.confidence
            - risk_s * w.risk_penalty;

        let total = total.clamp(0.0, 100.0);

        ScoreResult {
            total,
            spread: spread_s,
            liquidity: liquidity_s,
            depth: depth_s,
            volume: volume_s,
            volatility: volatility_s,
            risk: risk_s,
        }
    }
}

impl Default for OpportunityScore {
    fn default() -> Self {
        Self::new()
    }
}

/// 评分计算结果。
#[derive(Debug, Clone)]
pub struct ScoreResult {
    /// 综合评分（0~100）。
    pub total: f64,
    /// 价差分数。
    pub spread: f64,
    /// 流动性分数。
    pub liquidity: f64,
    /// 深度分数。
    pub depth: f64,
    /// 成交量分数。
    pub volume: f64,
    /// 波动率分数。
    pub volatility: f64,
    /// 风险分数（越低越好，不直接计入总分而是扣分项）。
    pub risk: f64,
}

impl ScoreResult {
    /// 中文分解说明（供 explain 使用）。
    pub fn breakdown_zh(&self) -> Vec<(String, f64)> {
        vec![
            ("价差".to_string(), self.spread),
            ("流动性".to_string(), self.liquidity),
            ("深度".to_string(), self.depth),
            ("成交量".to_string(), self.volume),
            ("波动率".to_string(), self.volatility),
            ("置信度".to_string(), self.risk), // 风险扣分用负数表示
        ]
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_score_sum_low_is_high() {
        let s = OpportunityScore::compute_spread_score(0.80);
        assert!(s >= 99.0, "SUM=0.80 → score should be ~100, got {s}");
    }

    #[test]
    fn spread_score_sum_high_is_low() {
        let s = OpportunityScore::compute_spread_score(0.99);
        assert!(s < 50.0, "SUM=0.99 → score should be low, got {s}");
    }

    #[test]
    fn spread_score_sum_at_1_is_zero() {
        assert_eq!(OpportunityScore::compute_spread_score(1.0), 0.0);
        assert_eq!(OpportunityScore::compute_spread_score(1.05), 0.0);
    }

    #[test]
    fn liquidity_score_log_scale() {
        let s_low = OpportunityScore::compute_liquidity_score(10.0);
        let s_high = OpportunityScore::compute_liquidity_score(100_000.0);
        assert!(s_high > s_low);
        assert!(s_high > 80.0);
    }

    #[test]
    fn liquidity_score_zero_is_zero() {
        assert_eq!(OpportunityScore::compute_liquidity_score(0.0), 0.0);
    }

    #[test]
    fn depth_score_zero_when_both_none() {
        assert_eq!(OpportunityScore::compute_depth_score(None, None), 0.0);
    }

    #[test]
    fn volatility_score_at_05_is_max() {
        assert!(OpportunityScore::compute_volatility_score(0.5) > 99.0);
    }

    #[test]
    fn volatility_score_extreme_is_low() {
        let s = OpportunityScore::compute_volatility_score(0.99);
        assert!(s < 10.0, "yes=0.99 → vol score should be low, got {s}");
    }

    #[test]
    fn risk_score_low_when_all_good() {
        let r = OpportunityScore::compute_risk_score(0.99, 80.0, 80.0);
        assert!(r < 20.0, "Good data → risk should be low, got {r}");
    }

    #[test]
    fn risk_score_high_when_sum_bad() {
        let r = OpportunityScore::compute_risk_score(0.50, 10.0, 10.0);
        assert!(r > 40.0, "Bad data → risk should be high, got {r}");
    }

    #[test]
    fn compute_total_score_in_range() {
        let scorer = OpportunityScore::new();
        let result = scorer.compute(0.90, 0.45, 5000.0, 10000.0, Some(2000.0), Some(3000.0), 0.85);
        assert!(result.total >= 0.0 && result.total <= 100.0);
        assert!(result.spread >= 0.0 && result.spread <= 100.0);
        assert!(result.liquidity >= 0.0 && result.liquidity <= 100.0);
        assert!(result.depth >= 0.0 && result.depth <= 100.0);
        assert!(result.volume >= 0.0 && result.volume <= 100.0);
        assert!(result.volatility >= 0.0 && result.volatility <= 100.0);
        assert!(result.risk >= 0.0 && result.risk <= 100.0);
    }

    #[test]
    fn custom_weights_affect_result() {
        let default_scorer = OpportunityScore::new();
        let custom = OpportunityScore::with_weights(ScoreWeights {
            spread: 0.5,
            liquidity: 0.0,
            depth: 0.0,
            volume: 0.0,
            volatility: 0.0,
            confidence: 0.5,
            risk_penalty: 0.0,
        });
        let r1 = default_scorer.compute(0.90, 0.45, 5000.0, 10000.0, Some(2000.0), Some(3000.0), 0.85);
        let r2 = custom.compute(0.90, 0.45, 5000.0, 10000.0, Some(2000.0), Some(3000.0), 0.85);
        assert_ne!(r1.total, r2.total, "Different weights → different totals");
    }
}
