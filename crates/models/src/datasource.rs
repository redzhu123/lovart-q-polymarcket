//! 统一数据源模型（V1.02 数据层重构）。
//!
//! 所有 `MarketDataProvider` 最终都把原始 API 数据转换为本模块的 [`UnifiedMarket`]，
//! Scanner 此后只认识 [`UnifiedMarket`]，不再直接依赖任何具体 API 的 JSON 结构。
//!
//! 纯数据、行为轻量（仅少量派生判断方法），无 HTTP / async，故归 `pm-models`。
//! Trait、Provider、Manager 等带行为的部分在 `pm-scanner::datasource`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 市场状态。
///
/// 映射 Polymarket 语义：已关闭市场通常 `active=true && closed=true`，
/// 故 [`MarketStatus::Closed`] 与 [`MarketStatus::Active`] 都视为"活跃过"，
/// 仅 [`MarketStatus::Inactive`]（`active=false`）不算活跃（见 [`UnifiedMarket::active`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus {
    /// 活跃：可交易（`active && !closed`）。
    Active,
    /// 已关闭：已结算 / 不可交易（`closed`）。
    Closed,
    /// 不活跃：`active=false`。
    Inactive,
}

impl MarketStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            MarketStatus::Active => "活跃",
            MarketStatus::Closed => "已关闭",
            MarketStatus::Inactive => "不活跃",
        }
    }
}

/// 统一市场模型 -- 所有 Provider 最终产出的标准形态（V1.02 第四节）。
///
/// 字段对齐 spec：MarketId / Question / Description / Status / YES / NO /
/// Volume / Liquidity / Category / OutcomeCount / Provider / 更新时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMarket {
    /// 市场唯一标识（Gamma 取 conditionId ?? id ?? question；CLOB 取 token_id）。
    pub market_id: String,
    /// 市场问题。
    pub question: String,
    /// 描述（部分 Provider 提供；无则 None，不伪造）。
    pub description: Option<String>,
    /// 市场状态。
    pub status: MarketStatus,
    /// YES 结果价格（0~1）。归一化中间价或真实买一价，取决于 Provider 能力；缺失为 None。
    pub yes_price: Option<f64>,
    /// NO 结果价格（0~1）。缺失为 None。
    pub no_price: Option<f64>,
    /// 成交额。
    pub volume: f64,
    /// 流动性。
    pub liquidity: f64,
    /// 分类（部分 Provider 提供；无则 None）。
    pub category: Option<String>,
    /// 结果数量（二元=2）。
    pub outcome_count: usize,
    /// 数据来源 Provider 名称（"gamma" / "clob" / "mock"）。
    pub provider: String,
    /// 数据更新时间（UTC）。
    pub updated_at: DateTime<Utc>,
}

impl UnifiedMarket {
    /// 是否活跃（含已关闭，匹配 Gamma 的 `active` 布尔语义：Active 与 Closed 均 active=true）。
    pub fn active(&self) -> bool {
        !matches!(self.status, MarketStatus::Inactive)
    }

    /// 是否已关闭。
    pub fn closed(&self) -> bool {
        matches!(self.status, MarketStatus::Closed)
    }

    /// 二元市场的 (YES, NO) 价格；仅当结果数为 2 且两价都存在时返回 Some。
    ///
    /// 与原 `pm_models::Market::yes_no_prices` 同语义（len!=2 视为非二元 -> None），
    /// 供机会识别复用。注意：多结果市场的 `yes_price`/`no_price` 可能存放前两个结果价，
    /// 但本方法仍返回 None（非二元不参与 YES/NO 套利判定）。
    pub fn yes_no_prices(&self) -> Option<(f64, f64)> {
        if self.outcome_count != 2 {
            return None;
        }
        match (self.yes_price, self.no_price) {
            (Some(y), Some(n)) => Some((y, n)),
            _ => None,
        }
    }

    /// 是否有可用价格（YES 与 NO 均存在）。
    pub fn has_prices(&self) -> bool {
        self.yes_no_prices().is_some()
    }
}

/// 订单簿模型（V1.02 第五节）。
///
/// Provider 不支持订单簿时返回 `None` / 空，**绝不伪造**。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// 对应市场标识。
    pub market_id: String,
    /// 最优买价（best bid）。不支持 / 无深度时为 None。
    pub best_bid: Option<f64>,
    /// 最优卖价（best ask）。不支持 / 无深度时为 None。
    pub best_ask: Option<f64>,
    /// 买卖价差（best_ask - best_bid）；两端齐全时才计算。
    pub spread: Option<f64>,
    /// 买盘深度（累计量；Provider 决定口径）。不支持为 None。
    pub bid_depth: Option<f64>,
    /// 卖盘深度。不支持为 None。
    pub ask_depth: Option<f64>,
    /// 快照时间（UTC）。
    pub timestamp: DateTime<Utc>,
    /// 数据来源 Provider 名称。
    pub provider: String,
}

impl OrderBook {
    /// 由 best_bid / best_ask 计算价差；两端缺失则 None。
    pub fn compute_spread(best_bid: Option<f64>, best_ask: Option<f64>) -> Option<f64> {
        match (best_bid, best_ask) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        }
    }
}

/// 单市场价格快照（供 `fetch_prices` 用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceQuote {
    pub market_id: String,
    pub yes_price: Option<f64>,
    pub no_price: Option<f64>,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
}

/// Provider 能力声明（V1.02 第六节）。
///
/// 每个 Provider 声明自己支持的数据维度；程序启动时打印，便于一眼看出
/// 当前数据源能否支撑真实套利（需 OrderBook / BidAsk）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapability {
    /// 市场列表。
    pub supports_markets: bool,
    /// 订单簿。
    pub supports_orderbook: bool,
    /// 成交记录。
    pub supports_trades: bool,
    /// 最优买卖价。
    pub supports_bid_ask: bool,
    /// 流动性。
    pub supports_liquidity: bool,
}

impl ProviderCapability {
    /// 是否能支撑真实套利（需订单簿 + 最优买卖价）。
    pub fn supports_real_arbitrage(&self) -> bool {
        self.supports_orderbook && self.supports_bid_ask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn um(yes: Option<f64>, no: Option<f64>, status: MarketStatus) -> UnifiedMarket {
        UnifiedMarket {
            market_id: "m1".into(),
            question: "Q".into(),
            description: None,
            status,
            yes_price: yes,
            no_price: no,
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 2,
            provider: "gamma".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn status_active_closed_both_active_semantics() {
        // Active 与 Closed 都算 active=true（匹配 Gamma 语义）；Inactive 不算。
        assert!(um(Some(0.4), Some(0.5), MarketStatus::Active).active());
        assert!(um(Some(0.4), Some(0.5), MarketStatus::Closed).active());
        assert!(!um(Some(0.4), Some(0.5), MarketStatus::Inactive).active());
        // closed() 仅 Closed 为真
        assert!(!um(Some(0.4), Some(0.5), MarketStatus::Active).closed());
        assert!(um(Some(0.4), Some(0.5), MarketStatus::Closed).closed());
    }

    #[test]
    fn yes_no_prices_requires_both() {
        assert_eq!(um(Some(0.4), Some(0.5), MarketStatus::Active).yes_no_prices(), Some((0.4, 0.5)));
        assert_eq!(um(Some(0.4), None, MarketStatus::Active).yes_no_prices(), None);
        assert_eq!(um(None, None, MarketStatus::Active).yes_no_prices(), None);
        assert!(um(Some(0.4), Some(0.5), MarketStatus::Active).has_prices());
        assert!(!um(Some(0.4), None, MarketStatus::Active).has_prices());
    }

    #[test]
    fn yes_no_prices_requires_binary_outcome_count() {
        // 多结果市场即使前两价存在，yes_no_prices 也为 None（非二元不参与套利）
        let multi = UnifiedMarket {
            outcome_count: 3,
            yes_price: Some(0.1),
            no_price: Some(0.2),
            ..um(Some(0.1), Some(0.2), MarketStatus::Active)
        };
        assert_eq!(multi.yes_no_prices(), None);
        assert!(!multi.has_prices());
    }

    #[test]
    fn spread_computed_only_when_both_present() {
        let s = OrderBook::compute_spread(Some(0.40), Some(0.45)).unwrap();
        assert!((s - 0.05).abs() < 1e-9);
        assert_eq!(OrderBook::compute_spread(Some(0.40), None), None);
        assert_eq!(OrderBook::compute_spread(None, None), None);
    }

    #[test]
    fn capability_real_arbitrage_needs_orderbook_and_bidask() {
        let gamma = ProviderCapability {
            supports_markets: true,
            supports_orderbook: false,
            supports_trades: false,
            supports_bid_ask: false,
            supports_liquidity: true,
        };
        assert!(!gamma.supports_real_arbitrage());
        let clob = ProviderCapability {
            supports_markets: true,
            supports_orderbook: true,
            supports_trades: true,
            supports_bid_ask: true,
            supports_liquidity: true,
        };
        assert!(clob.supports_real_arbitrage());
    }
}
