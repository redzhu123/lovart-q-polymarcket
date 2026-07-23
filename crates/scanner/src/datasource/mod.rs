//! pm-scanner::datasource：统一数据源层（V1.02 / V1.03 CLOB 扩展）。
//!
//! 重构整个数据层：所有市场数据经 [`MarketDataProvider`] Trait 统一获取，
//! Scanner 不再直接访问 HTTP，只依赖 Trait + [`DataSourceManager`]。
//!
//! - [`MarketDataProvider`]：统一数据源接口（第一节）。
//! - [`gamma::GammaProvider`]：Gamma API Provider（只提供市场/流动性，无订单簿/买卖价）。
//! - [`clob::ClobProvider`]：CLOB API Provider（V1.03 新增 -- 订单簿/多档盘口/买卖价/成交记录）。
//! - [`mock::MockProvider`]：测试用 Provider。
//! - [`manager::DataSourceManager`]：统一管理 Provider + Cache，按 config 切换。
//! - [`validator::Validator`]：数据合法性校验（第七节）。
//! - [`cache::MarketDataCache`]：内存缓存，TTL 默认 10 秒（第八节）。
//! - [`snapshot::MarketSnapshot`]：每轮市场快照（第九节）。
//! - [`statistics::DataStatistics`]：每轮市场数据统计（第十节）。
//! - [`diagnose`]：`datasource` 诊断模式（第十一节）。
//!
//! 现有交易/策略/Shadow/Execution 逻辑完全不感知本层 -- 它们仍只消费 `OppSnapshot`。

pub mod cache;
pub mod clob;
pub mod diagnose;
pub mod gamma;
pub mod manager;
pub mod mock;
pub mod snapshot;
pub mod statistics;
pub mod validator;

pub use cache::MarketDataCache;
pub use clob::ClobProvider;
pub use diagnose::run_datasource_diagnose;
pub use gamma::GammaProvider;
pub use manager::{CacheInfo, DataSourceManager, FetchOutcome};
pub use mock::MockProvider;
pub use snapshot::MarketSnapshot;
pub use statistics::DataStatistics;
pub use validator::{Validator, ValidatorReport};

use anyhow::Result;
use async_trait::async_trait;
use pm_models::{OrderBook, PriceQuote, ProviderCapability};

use crate::stats::FetchResult;

/// 健康探测结果（供 `health_check` 返回 + 启动检查复用）。
#[derive(Debug, Clone)]
pub struct HealthProbe {
    /// 是否成功（HTTP 2xx 且 JSON 可解析）。
    pub ok: bool,
    /// HTTP 状态码（请求未到达服务端为 0）。
    pub status: u16,
    /// 探测返回/解析得到的市场数。
    pub market_count: usize,
    /// 探测耗时（毫秒）。
    pub latency_ms: u128,
    /// 人类可读细节（错误信息或"HTTP 200, 1 个市场"）。
    pub detail: String,
}

/// 所有数据源必须实现的统一接口（V1.02 第一节）。
///
/// Scanner 只依赖本 Trait，不知道具体 Provider。新增数据源只需实现本 Trait
/// 并在 [`DataSourceManager`] 工厂注册一行。
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    /// Provider 名称（"gamma" / "clob" / "mock"）。
    fn name(&self) -> &str;

    /// 声明能力（第六节）。
    fn capability(&self) -> ProviderCapability;

    /// 拉取市场列表，统一为 `UnifiedMarket`，并返回拉取可观测性统计。
    async fn fetch_markets(&self) -> Result<FetchResult>;

    /// 拉取订单簿。**不支持的 Provider 返回空 Vec，绝不伪造数据**（第五节）。
    async fn fetch_orderbooks(&self, market_ids: &[String]) -> Result<Vec<OrderBook>>;

    /// 拉取价格。**不支持的 Provider 返回空 Vec**。
    async fn fetch_prices(&self, market_ids: &[String]) -> Result<Vec<PriceQuote>>;

    /// 健康检查（最小请求 + JSON 解析）。
    async fn health_check(&self) -> Result<HealthProbe>;
}

#[cfg(test)]
mod tests {
    //! Trait 对象分发的基础测试（跨 Provider 通用）。
    //! 具体 Provider 的行为测试见各 Provider 模块。

    use super::*;

    /// 任意 Provider 都应能以 `Box<dyn MarketDataProvider>` 持有并调用 name/capability（对象安全）。
    #[tokio::test]
    async fn trait_object_dispatch_works() {
        let p: Box<dyn MarketDataProvider> = Box::new(MockProvider::default());
        assert_eq!(p.name(), "mock");
        let cap = p.capability();
        assert!(cap.supports_markets);
        // fetch_markets 经 dyn 分发可调用
        let r = p.fetch_markets().await.expect("mock fetch");
        assert!(!r.markets.is_empty());
    }
}
