//! 套利机会数据模型（V1.04 Opportunity Engine）。
//!
//! 定义 [`Opportunity`] 及其关联类型（类型、状态、ID），
//! 是 Strategy 层唯一处理的输入。Strategy 不再直接分析 Market。
//!
//! 纯数据、行为轻量（派生判断方法），不带 HTTP / async。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// OpportunityId
// ============================================================================

/// 机会唯一标识。
/// 格式：`OPP-{timestamp_ms}-{short_hash}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpportunityId(pub String);

impl OpportunityId {
    /// 从时间戳与 market_id 的哈希生成。
    pub fn generate(market_id: &str, now: DateTime<Utc>) -> Self {
        let ts = now.timestamp_millis();
        // 取 market_id 前 8 字符作为短标识
        let short: String = market_id.chars().take(8).collect();
        Self(format!("OPP-{}-{}", ts, short))
    }
}

impl std::fmt::Display for OpportunityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ============================================================================
// OpportunityType
// ============================================================================

/// 套利机会类型。
///
/// 策略根据 Type 决定是否参与。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpportunityType {
    /// 无风险套利（SUM 远低于 1.0，存在确定收益）。
    Arbitrage,
    /// 价差套利（利用买卖价差获利）。
    Spread,
    /// 动量策略（价格趋势延续）。
    Momentum,
    /// 均值回归（价格偏离均值后回归）。
    MeanReversion,
    /// 流动性套利（利用流动性溢价）。
    Liquidity,
    /// 跨市场套利（不同平台间价差）。
    CrossMarket,
    /// 价格缺口（单市场 sudden price gap）。
    PriceGap,
    /// 未知类型（默认兜底）。
    Unknown,
}

impl OpportunityType {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            OpportunityType::Arbitrage => "套利",
            OpportunityType::Spread => "价差",
            OpportunityType::Momentum => "动量",
            OpportunityType::MeanReversion => "均值回归",
            OpportunityType::Liquidity => "流动性",
            OpportunityType::CrossMarket => "跨市场",
            OpportunityType::PriceGap => "价格缺口",
            OpportunityType::Unknown => "未知",
        }
    }

    /// 从市场数据判定机会类型。
    ///
    /// 判定规则（优先级从高到低）：
    /// - SUM < 0.90 → Arbitrage
    /// - 0.90 ≤ SUM < 0.98 → Spread
    /// - Spread > 0.05 → PriceGap
    /// - Liquidity > 10_000 → Liquidity
    /// - 默认 → Unknown
    pub fn classify(sum: f64, spread: Option<f64>, liquidity: f64) -> Self {
        if sum < 0.90 {
            return OpportunityType::Arbitrage;
        }
        if sum < 0.98 {
            return OpportunityType::Spread;
        }
        if let Some(s) = spread {
            if s > 0.05 {
                return OpportunityType::PriceGap;
            }
        }
        if liquidity > 10_000.0 {
            return OpportunityType::Liquidity;
        }
        OpportunityType::Unknown
    }
}

impl Default for OpportunityType {
    fn default() -> Self {
        Self::Unknown
    }
}

// ============================================================================
// OpportunityStatus
// ============================================================================

/// 机会生命周期状态。
///
/// 状态流转：
/// ```text
/// Created → Updated → Stable → Weak → Expired → Removed
///              ↑          |        |
///              +----------+--------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpportunityStatus {
    /// 首次发现（本轮新创建）。
    Created,
    /// 已更新（持续出现，数据有变化）。
    Updated,
    /// 稳定（多轮持续，数据稳定）。
    Stable,
    /// 衰减中（数据变差/接近过期）。
    Weak,
    /// 已过期（不再活跃，等待移除）。
    Expired,
    /// 已移除（从活跃队列移除）。
    Removed,
}

impl OpportunityStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            OpportunityStatus::Created => "新建",
            OpportunityStatus::Updated => "更新",
            OpportunityStatus::Stable => "稳定",
            OpportunityStatus::Weak => "衰减",
            OpportunityStatus::Expired => "过期",
            OpportunityStatus::Removed => "已移除",
        }
    }

    /// 是否处于活跃状态（仍可被策略处理）。
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            OpportunityStatus::Created
                | OpportunityStatus::Updated
                | OpportunityStatus::Stable
                | OpportunityStatus::Weak
        )
    }

    /// 是否处于终态（不再参与活跃队列）。
    pub fn is_terminal(&self) -> bool {
        matches!(self, OpportunityStatus::Expired | OpportunityStatus::Removed)
    }
}

impl Default for OpportunityStatus {
    fn default() -> Self {
        Self::Created
    }
}

// ============================================================================
// Opportunity
// ============================================================================

/// 套利机会完整模型（V1.04 核心 DTO）。
///
/// Opportunity 是 Strategy 层唯一处理的输入。
/// Strategy 不再直接分析 Market / UnifiedMarket / OppSnapshot。
///
/// 字段分组：
/// - 标识：id / market_id / question / provider
/// - 分类：opportunity_type / status
/// - 评分：score / confidence / priority / 各维度分项
/// - 收益：expected_roi / expected_profit / risk_score
/// - 市场快照：yes_price / no_price / sum / spread / volume / liquidity / bid_depth / ask_depth
/// - 元数据：detected_time / expire_time / snapshot_id / version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    // ---- 标识 ----
    pub id: String,
    pub market_id: String,
    pub question: String,
    pub provider: String,

    // ---- 时间 ----
    pub detected_time: DateTime<Utc>,
    pub expire_time: Option<DateTime<Utc>>,

    // ---- 分类 ----
    pub opportunity_type: OpportunityType,
    pub status: OpportunityStatus,

    // ---- 评分（0~100 制） ----
    /// 综合评分（0~100），越高越好。
    pub score: f64,
    /// 置信度（0.0~1.0），越高越可信。
    pub confidence: f64,
    /// 优先级（0~100），综合 score + confidence 映射。
    pub priority: u8,

    // ---- 维度分项（0~100） ----
    pub spread_score: f64,
    pub liquidity_score: f64,
    pub depth_score: f64,
    pub volume_score: f64,
    pub volatility_score: f64,
    pub risk_score: f64,

    // ---- 收益预估 ----
    /// 预期收益率（小数，如 0.023 表示 2.3%）。
    pub expected_roi: f64,
    /// 预期利润（绝对金额）。
    pub expected_profit: f64,

    // ---- 市场快照 ----
    pub yes_price: f64,
    pub no_price: f64,
    pub sum: f64,
    pub spread: Option<f64>,
    pub volume: f64,
    pub liquidity: f64,
    pub bid_depth: Option<f64>,
    pub ask_depth: Option<f64>,

    // ---- 元数据 ----
    pub snapshot_id: Option<String>,
    pub version: u32,
}

impl Opportunity {
    /// 创建新的 Opportunity（status=Created, version=1）。
    pub fn new(
        market_id: String,
        question: String,
        provider: String,
        detected_time: DateTime<Utc>,
        opportunity_type: OpportunityType,
        score: f64,
        confidence: f64,
        priority: u8,
        spread_score: f64,
        liquidity_score: f64,
        depth_score: f64,
        volume_score: f64,
        volatility_score: f64,
        risk_score: f64,
        expected_roi: f64,
        expected_profit: f64,
        yes_price: f64,
        no_price: f64,
        sum: f64,
        spread: Option<f64>,
        volume: f64,
        liquidity: f64,
        bid_depth: Option<f64>,
        ask_depth: Option<f64>,
    ) -> Self {
        let id = OpportunityId::generate(&market_id, detected_time).to_string();
        Self {
            id,
            market_id,
            question,
            provider,
            detected_time,
            expire_time: None,
            opportunity_type,
            status: OpportunityStatus::Created,
            score,
            confidence,
            priority,
            spread_score,
            liquidity_score,
            depth_score,
            volume_score,
            volatility_score,
            risk_score,
            expected_roi,
            expected_profit,
            yes_price,
            no_price,
            sum,
            spread,
            volume,
            liquidity,
            bid_depth,
            ask_depth,
            snapshot_id: None,
            version: 1,
        }
    }

    /// 中文单行摘要（用于列表展示）。
    pub fn summary_zh(&self) -> String {
        format!(
            "{} | 类型={} | 评分={:.0} | 置信度={:.0}% | ROI={:.1}% | SUM={:.4} | YES={:.4} NO={:.4}",
            self.question.chars().take(40).collect::<String>(),
            self.opportunity_type.as_zh(),
            self.score,
            (self.confidence * 100.0),
            self.expected_roi * 100.0,
            self.sum,
            self.yes_price,
            self.no_price,
        )
    }

    /// 是否高优先级（priority ≥ 80）。
    pub fn is_high_priority(&self) -> bool {
        self.priority >= 80
    }

    /// 是否高置信度（confidence ≥ 0.8）。
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.8
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn opportunity_id_generate_is_unique() {
        let now = Utc::now();
        let a = OpportunityId::generate("token_abc", now);
        let b = OpportunityId::generate("token_xyz", now);
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn type_classify_arbitrage() {
        assert_eq!(
            OpportunityType::classify(0.85, None, 100.0),
            OpportunityType::Arbitrage
        );
    }

    #[test]
    fn type_classify_spread() {
        assert_eq!(
            OpportunityType::classify(0.95, None, 500.0),
            OpportunityType::Spread
        );
    }

    #[test]
    fn type_classify_price_gap() {
        assert_eq!(
            OpportunityType::classify(0.99, Some(0.08), 100.0),
            OpportunityType::PriceGap
        );
    }

    #[test]
    fn type_classify_liquidity() {
        assert_eq!(
            OpportunityType::classify(0.99, None, 50_000.0),
            OpportunityType::Liquidity
        );
    }

    #[test]
    fn type_classify_unknown_as_default() {
        assert_eq!(
            OpportunityType::classify(0.99, None, 100.0),
            OpportunityType::Unknown
        );
    }

    #[test]
    fn status_is_active_and_terminal() {
        assert!(OpportunityStatus::Created.is_active());
        assert!(OpportunityStatus::Updated.is_active());
        assert!(OpportunityStatus::Stable.is_active());
        assert!(OpportunityStatus::Weak.is_active());
        assert!(!OpportunityStatus::Expired.is_active());
        assert!(!OpportunityStatus::Removed.is_active());

        assert!(OpportunityStatus::Expired.is_terminal());
        assert!(OpportunityStatus::Removed.is_terminal());
        assert!(!OpportunityStatus::Created.is_terminal());
    }

    #[test]
    fn type_as_zh_all_variants() {
        // 确保所有变体都有中文名
        let variants = [
            OpportunityType::Arbitrage,
            OpportunityType::Spread,
            OpportunityType::Momentum,
            OpportunityType::MeanReversion,
            OpportunityType::Liquidity,
            OpportunityType::CrossMarket,
            OpportunityType::PriceGap,
            OpportunityType::Unknown,
        ];
        for v in &variants {
            assert!(!v.as_zh().is_empty());
        }
    }

    #[test]
    fn status_as_zh_all_variants() {
        let variants = [
            OpportunityStatus::Created,
            OpportunityStatus::Updated,
            OpportunityStatus::Stable,
            OpportunityStatus::Weak,
            OpportunityStatus::Expired,
            OpportunityStatus::Removed,
        ];
        for v in &variants {
            assert!(!v.as_zh().is_empty());
        }
    }

    #[test]
    fn opportunity_new_has_expected_defaults() {
        let now = Utc::now();
        let opp = Opportunity::new(
            "m1".into(),
            "测试问题".into(),
            "test".into(),
            now,
            OpportunityType::Arbitrage,
            85.0,
            0.9,
            80,
            25.0, 20.0, 15.0, 10.0, 5.0, 10.0,
            0.05,
            5.0,
            0.40, 0.50, 0.90,
            Some(0.10),
            1000.0, 2000.0,
            Some(500.0), Some(600.0),
        );
        assert_eq!(opp.status, OpportunityStatus::Created);
        assert_eq!(opp.version, 1);
        assert!(opp.id.starts_with("OPP-"));
        assert!(opp.is_high_priority());
        assert!(opp.is_high_confidence());
    }
}
