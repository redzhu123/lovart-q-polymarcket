//! 市场适配器 Trait（P3.0）。
//!
//! 定义市场数据格式转换的统一接口。
//! 每个市场的 Adapter 负责将该市场的原始数据格式转换为系统统一格式。

use async_trait::async_trait;
use serde_json::Value;

use crate::error::MarketFrameworkResult;

// ============================================================================
// Unified Types（适配器输出）
// ============================================================================

/// 统一的市场摘要（适配器输出格式）。
#[derive(Debug, Clone)]
pub struct UnifiedMarketSummary {
    /// 市场内部 ID。
    pub market_id: String,
    /// 交易对 / 问题。
    pub symbol: String,
    /// 基础资产。
    pub base_asset: String,
    /// 报价资产。
    pub quote_asset: String,
    /// 最新价格。
    pub last_price: Option<f64>,
    /// 24h 成交量。
    pub volume_24h: Option<f64>,
    /// 24h 最高价。
    pub high_24h: Option<f64>,
    /// 24h 最低价。
    pub low_24h: Option<f64>,
    /// 是否活跃。
    pub active: bool,
}

/// 统一的订单簿（适配器输出格式）。
#[derive(Debug, Clone)]
pub struct UnifiedOrderBook {
    /// 市场内部 ID。
    pub market_id: String,
    /// 交易对。
    pub symbol: String,
    /// 买盘（价格, 数量）。
    pub bids: Vec<(f64, f64)>,
    /// 卖盘（价格, 数量）。
    pub asks: Vec<(f64, f64)>,
    /// 时间戳。
    pub timestamp: String,
}

/// 统一的成交记录（适配器输出格式）。
#[derive(Debug, Clone)]
pub struct UnifiedTrade {
    /// 成交 ID。
    pub trade_id: String,
    /// 交易对。
    pub symbol: String,
    /// 价格。
    pub price: f64,
    /// 数量。
    pub quantity: f64,
    /// 方向（buy/sell）。
    pub side: String,
    /// 时间戳。
    pub timestamp: String,
}

// ============================================================================
// MarketAdapter Trait
// ============================================================================

/// 市场适配器 Trait。
///
/// 负责将市场特定的原始数据格式转换为系统统一格式。
/// 每个市场必须实现此 Trait（如果它的数据格式与系统统一格式不同）。
///
/// # 实现指南
///
/// ```ignore
/// struct PolymarketAdapter { ... }
///
/// #[async_trait]
/// impl MarketAdapter for PolymarketAdapter {
///     fn adapter_name(&self) -> &str { "PolymarketAdapter" }
///     // ...
/// }
/// ```
#[async_trait]
pub trait MarketAdapter: Send + Sync {
    /// 适配器名称。
    fn adapter_name(&self) -> &str;

    /// 适配器版本。
    fn adapter_version(&self) -> &str {
        "1.0.0"
    }

    /// 将原始 JSON 市场数据转换为统一市场摘要列表。
    fn parse_markets(&self, raw_json: &str) -> MarketFrameworkResult<Vec<UnifiedMarketSummary>>;

    /// 将原始 JSON 订单簿数据转换为统一订单簿。
    fn parse_orderbook(
        &self,
        raw_json: &str,
        market_id: &str,
        symbol: &str,
    ) -> MarketFrameworkResult<UnifiedOrderBook>;

    /// 将原始 JSON 成交数据转换为统一成交记录列表。
    fn parse_trades(
        &self,
        raw_json: &str,
        symbol: &str,
    ) -> MarketFrameworkResult<Vec<UnifiedTrade>>;

    /// 检查原始 JSON 是否有效（不抛出异常的快速检查）。
    fn validate_raw_json(&self, raw_json: &str) -> bool {
        serde_json::from_str::<Value>(raw_json).is_ok()
    }

    /// 适配器信息摘要（中文）。
    fn summary_zh(&self) -> String {
        format!(
            "【适配器】{} v{}",
            self.adapter_name(),
            self.adapter_version()
        )
    }
}

// ============================================================================
// NoopAdapter
// ============================================================================

/// 空适配器（不做任何转换，返回空数据）。
///
/// 用于不需要数据适配的市场。
pub struct NoopAdapter;

impl NoopAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketAdapter for NoopAdapter {
    fn adapter_name(&self) -> &str {
        "NoopAdapter"
    }

    fn parse_markets(&self, _raw_json: &str) -> MarketFrameworkResult<Vec<UnifiedMarketSummary>> {
        Ok(Vec::new())
    }

    fn parse_orderbook(
        &self,
        _raw_json: &str,
        market_id: &str,
        symbol: &str,
    ) -> MarketFrameworkResult<UnifiedOrderBook> {
        Ok(UnifiedOrderBook {
            market_id: market_id.to_string(),
            symbol: symbol.to_string(),
            bids: Vec::new(),
            asks: Vec::new(),
            timestamp: String::new(),
        })
    }

    fn parse_trades(
        &self,
        _raw_json: &str,
        _symbol: &str,
    ) -> MarketFrameworkResult<Vec<UnifiedTrade>> {
        Ok(Vec::new())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_adapter_returns_empty() {
        let adapter = NoopAdapter::new();
        let markets = adapter.parse_markets("{}").unwrap();
        assert!(markets.is_empty());

        let ob = adapter.parse_orderbook("{}", "m1", "BTC/USDT").unwrap();
        assert!(ob.bids.is_empty());
        assert!(ob.asks.is_empty());

        let trades = adapter.parse_trades("{}", "BTC/USDT").unwrap();
        assert!(trades.is_empty());
    }

    #[test]
    fn noop_adapter_validate() {
        let adapter = NoopAdapter::new();
        assert!(adapter.validate_raw_json("{}"));
        assert!(!adapter.validate_raw_json("{invalid"));
    }

    #[test]
    fn adapter_summary_zh() {
        let adapter = NoopAdapter::new();
        let summary = adapter.summary_zh();
        assert!(summary.contains("NoopAdapter"));
    }
}
