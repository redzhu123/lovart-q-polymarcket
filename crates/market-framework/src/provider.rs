//! 市场数据供应商 Trait（P3.0）。
//!
//! 定义市场数据层的统一接口。
//! 每个市场的 Provider 负责从该市场的 API 拉取原始数据。

use async_trait::async_trait;

use crate::error::MarketFrameworkResult;
use crate::metadata::MarketMetadata;
use crate::{AmmPoolState, DexPoolQuote, VenueQuote};

/// Typed market-data boundary for centralized exchanges.
#[async_trait]
pub trait CexMarketDataProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn fetch_quotes(&self) -> MarketFrameworkResult<Vec<VenueQuote>>;
    async fn health_check(&self) -> MarketFrameworkResult<()>;
}

/// Typed market-data boundary for decentralized exchanges.
///
/// A DEX provider exposes AMM state and quantity-specific swap quotes. It is
/// intentionally separate from the order-book provider contract.
#[async_trait]
pub trait DexMarketDataProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn fetch_pool_states(&self) -> MarketFrameworkResult<Vec<AmmPoolState>>;
    async fn quote_pool(
        &self,
        pool_id: &str,
        base_quantity: f64,
    ) -> MarketFrameworkResult<DexPoolQuote>;
    async fn health_check(&self) -> MarketFrameworkResult<()>;
}

// ============================================================================
// MarketDataProvider Trait
// ============================================================================

/// 市场数据供应商 Trait。
///
/// 负责从指定市场拉取行情、订单簿、成交记录等数据。
/// 每个市场必须实现此 Trait 提供数据访问能力。
///
/// # 实现指南
///
/// ```ignore
/// struct PolymarketDataProvider { ... }
///
/// #[async_trait]
/// impl MarketDataProvider for PolymarketDataProvider {
///     fn provider_name(&self) -> &str { "Polymarket" }
///     // ...
/// }
/// ```
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    /// 供应商名称（中文）。
    fn provider_name(&self) -> &str;

    /// 供应商版本。
    fn provider_version(&self) -> &str {
        "1.0.0"
    }

    /// 获取市场元数据。
    fn metadata(&self) -> &MarketMetadata;

    /// 获取市场列表（原始格式）。
    ///
    /// 返回该市场所有可交易品种的原始数据。
    /// 调用方应使用 [`MarketAdapter`](crate::adapter::MarketAdapter) 转换为统一格式。
    async fn fetch_markets_raw(&self) -> MarketFrameworkResult<String>;

    /// 获取指定品种的订单簿（原始格式）。
    async fn fetch_orderbook_raw(
        &self,
        symbol: &str,
        depth: usize,
    ) -> MarketFrameworkResult<String>;

    /// 获取指定品种的最新成交（原始格式）。
    async fn fetch_trades_raw(&self, symbol: &str, limit: usize) -> MarketFrameworkResult<String>;

    /// 检查供应商健康状态。
    async fn health_check(&self) -> MarketFrameworkResult<()>;

    /// 供应商的速率限制（每秒最大请求数，0 表示无限制）。
    fn rate_limit(&self) -> u32 {
        0
    }

    /// 供应商信息摘要（中文）。
    fn summary_zh(&self) -> String {
        format!(
            "【数据供应商】{}\n  版本: {}\n  市场: {}\n  速率限制: {}",
            self.provider_name(),
            self.provider_version(),
            self.metadata().exchange,
            if self.rate_limit() > 0 {
                format!("{} 次/秒", self.rate_limit())
            } else {
                "无限制".to_string()
            }
        )
    }
}

// ============================================================================
// MockMarketDataProvider
// ============================================================================

/// Mock 市场数据供应商（用于测试和开发）。
pub struct MockMarketDataProvider {
    name: String,
    metadata: MarketMetadata,
}

impl MockMarketDataProvider {
    /// 创建新的 Mock Provider。
    pub fn new(name: impl Into<String>, metadata: MarketMetadata) -> Self {
        Self {
            name: name.into(),
            metadata,
        }
    }

    /// 创建预测市场的 Mock Provider。
    pub fn prediction_market() -> Self {
        Self::new(
            "Mock 预测市场数据源",
            MarketMetadata::prediction_market("MockExchange", "MOCK"),
        )
    }

    /// 创建现货市场的 Mock Provider。
    pub fn spot_market() -> Self {
        Self::new(
            "Mock 现货市场数据源",
            MarketMetadata::spot_market("MockExchange", "MOCK", "USDT"),
        )
    }
}

#[async_trait]
impl MarketDataProvider for MockMarketDataProvider {
    fn provider_name(&self) -> &str {
        &self.name
    }

    fn metadata(&self) -> &MarketMetadata {
        &self.metadata
    }

    async fn fetch_markets_raw(&self) -> MarketFrameworkResult<String> {
        Ok(r#"{"markets":[]}"#.to_string())
    }

    async fn fetch_orderbook_raw(
        &self,
        _symbol: &str,
        _depth: usize,
    ) -> MarketFrameworkResult<String> {
        Ok(r#"{"bids":[],"asks":[]}"#.to_string())
    }

    async fn fetch_trades_raw(
        &self,
        _symbol: &str,
        _limit: usize,
    ) -> MarketFrameworkResult<String> {
        Ok(r#"{"trades":[]}"#.to_string())
    }

    async fn health_check(&self) -> MarketFrameworkResult<()> {
        Ok(())
    }

    fn rate_limit(&self) -> u32 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_health() {
        let provider = MockMarketDataProvider::prediction_market();
        assert!(provider.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn mock_provider_fetch_markets() {
        let provider = MockMarketDataProvider::spot_market();
        let result = provider.fetch_markets_raw().await;
        assert!(result.is_ok());
    }

    #[test]
    fn mock_provider_summary() {
        let provider = MockMarketDataProvider::prediction_market();
        let summary = provider.summary_zh();
        assert!(summary.contains("Mock"));
        assert!(summary.contains("数据供应商"));
    }
}
