//! 统一拒绝原因模型。
//!
//! 所有 Candidate 被过滤时必须记录原因。本模块定义完整拒绝原因枚举及中文映射。
//!
//! 扩展现有的 `pm_scanner::stats::RejectionReason`（V1.0.1），增加引擎级拒绝类别。

/// 候选机会被拒绝的原因。
///
/// 按严重程度排序：数据问题 > 市场状态 > 策略过滤 > 重复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateRejection {
    // ---- 数据问题 ----
    /// 价格数据缺失或无效。
    PriceInvalid,
    /// 订单簿为空（无买卖盘）。
    BookEmpty,
    /// 无订单簿数据（Provider 不支持或获取失败）。
    NoOrderBook,
    /// 未知错误。
    UnknownError,

    // ---- 市场状态 ----
    /// 市场已关闭。
    MarketClosed,
    /// 市场不活跃。
    Inactive,

    // ---- 策略过滤 ----
    /// 价差太小（YES+NO >= 阈值）。
    SpreadTooSmall,
    /// 流动性太低。
    LiquidityTooLow,
    /// 成交量太低。
    VolumeTooLow,
    /// 评分低于最低阈值（引擎内部 min_score）。
    LowScore,

    // ---- 数据格式 ----
    /// 缺失价格（outcome_prices 为空）。
    MissingPrice,
    /// 数据无效（非二元市场，或 JSON 解析失败）。
    InvalidData,

    // ---- 重复 ----
    /// 已在跟踪中（同一市场已有活跃机会）。
    AlreadyExists,
}

impl CandidateRejection {
    /// 中文显示名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            CandidateRejection::PriceInvalid => "价格无效",
            CandidateRejection::BookEmpty => "订单簿为空",
            CandidateRejection::NoOrderBook => "无订单簿",
            CandidateRejection::UnknownError => "未知错误",
            CandidateRejection::MarketClosed => "市场已关闭",
            CandidateRejection::Inactive => "不活跃",
            CandidateRejection::SpreadTooSmall => "价差过小",
            CandidateRejection::LiquidityTooLow => "流动性过低",
            CandidateRejection::VolumeTooLow => "成交量过低",
            CandidateRejection::LowScore => "评分过低",
            CandidateRejection::MissingPrice => "缺失价格",
            CandidateRejection::InvalidData => "数据无效",
            CandidateRejection::AlreadyExists => "已存在",
        }
    }

    /// 简短标识符（用于 CSV / 日志 key）。
    pub fn as_key(&self) -> &'static str {
        match self {
            CandidateRejection::PriceInvalid => "price_invalid",
            CandidateRejection::BookEmpty => "book_empty",
            CandidateRejection::NoOrderBook => "no_orderbook",
            CandidateRejection::UnknownError => "unknown_error",
            CandidateRejection::MarketClosed => "market_closed",
            CandidateRejection::Inactive => "inactive",
            CandidateRejection::SpreadTooSmall => "spread_too_small",
            CandidateRejection::LiquidityTooLow => "liquidity_too_low",
            CandidateRejection::VolumeTooLow => "volume_too_low",
            CandidateRejection::LowScore => "low_score",
            CandidateRejection::MissingPrice => "missing_price",
            CandidateRejection::InvalidData => "invalid_data",
            CandidateRejection::AlreadyExists => "already_exists",
        }
    }

    /// 是否为数据类问题（非策略过滤）。
    pub fn is_data_issue(&self) -> bool {
        matches!(
            self,
            CandidateRejection::PriceInvalid
                | CandidateRejection::BookEmpty
                | CandidateRejection::NoOrderBook
                | CandidateRejection::MissingPrice
                | CandidateRejection::InvalidData
                | CandidateRejection::UnknownError
        )
    }

    /// 是否为市场状态类问题。
    pub fn is_market_state(&self) -> bool {
        matches!(
            self,
            CandidateRejection::MarketClosed | CandidateRejection::Inactive
        )
    }

    /// 是否为策略过滤类问题（含重复检测）。
    pub fn is_strategy_filter(&self) -> bool {
        matches!(
            self,
            CandidateRejection::SpreadTooSmall
                | CandidateRejection::LiquidityTooLow
                | CandidateRejection::VolumeTooLow
                | CandidateRejection::LowScore
                | CandidateRejection::AlreadyExists
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_have_zh() {
        let variants = [
            CandidateRejection::PriceInvalid,
            CandidateRejection::BookEmpty,
            CandidateRejection::NoOrderBook,
            CandidateRejection::UnknownError,
            CandidateRejection::MarketClosed,
            CandidateRejection::Inactive,
            CandidateRejection::SpreadTooSmall,
            CandidateRejection::LiquidityTooLow,
            CandidateRejection::VolumeTooLow,
            CandidateRejection::LowScore,
            CandidateRejection::MissingPrice,
            CandidateRejection::InvalidData,
            CandidateRejection::AlreadyExists,
        ];
        for v in &variants {
            assert!(!v.as_zh().is_empty(), "empty zh for {:?}", v);
            assert!(!v.as_key().is_empty(), "empty key for {:?}", v);
        }
    }

    #[test]
    fn categories_are_correct() {
        assert!(CandidateRejection::PriceInvalid.is_data_issue());
        assert!(!CandidateRejection::PriceInvalid.is_market_state());
        assert!(!CandidateRejection::PriceInvalid.is_strategy_filter());

        assert!(!CandidateRejection::MarketClosed.is_data_issue());
        assert!(CandidateRejection::MarketClosed.is_market_state());

        assert!(!CandidateRejection::SpreadTooSmall.is_data_issue());
        assert!(CandidateRejection::SpreadTooSmall.is_strategy_filter());
    }

    #[test]
    fn all_variants_classified() {
        // 确保每个 variant 至少属于一个类别
        let variants = [
            CandidateRejection::PriceInvalid,
            CandidateRejection::BookEmpty,
            CandidateRejection::NoOrderBook,
            CandidateRejection::UnknownError,
            CandidateRejection::MarketClosed,
            CandidateRejection::Inactive,
            CandidateRejection::SpreadTooSmall,
            CandidateRejection::LiquidityTooLow,
            CandidateRejection::VolumeTooLow,
            CandidateRejection::LowScore,
            CandidateRejection::MissingPrice,
            CandidateRejection::InvalidData,
            CandidateRejection::AlreadyExists,
        ];
        for v in &variants {
            assert!(
                v.is_data_issue() || v.is_market_state() || v.is_strategy_filter(),
                "{:?} is not classified",
                v
            );
        }
    }
}
