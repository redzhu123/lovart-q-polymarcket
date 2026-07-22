//! DataSourceManager：统一管理 Provider + Cache（V1.02 第三节）。
//!
//! 按 `config.datasource.provider` 选择具体 Provider，Scanner 只持有本 Manager，
//! **不知道**具体 Provider。未来切换 gamma->clob 只改配置，Scanner 无需修改。
//!
//! V1.02：
//! - 阶段一：工厂 + 能力打印 + health_check。
//! - 阶段二：`fetch_markets` 接入 [`MarketDataCache`]（优先读缓存，命中返回上次结果），
//!   返回 [`FetchOutcome`]（含 `cached` 标记，供统计区分命中/未命中）。

use std::time::Duration;

use anyhow::Result;
use pm_models::{Config, LogLevel, ProviderCapability, UnifiedMarket};

use crate::datasource::cache::MarketDataCache;
use crate::datasource::{HealthProbe, MarketDataProvider};
use crate::datasource::{GammaProvider, MockProvider};
use crate::display::{DASH, SEP};
use crate::stats::{FetchResult, FetchStats};

/// 数据源管理器：持有当前活跃 Provider（trait 对象）+ 内存缓存。
pub struct DataSourceManager {
    provider: Box<dyn MarketDataProvider>,
    cache: MarketDataCache,
}

/// `fetch_markets` 的返回：市场列表 + 拉取统计 + 是否来自缓存。
pub struct FetchOutcome {
    pub markets: Vec<UnifiedMarket>,
    pub stats: FetchStats,
    /// 本次结果是否来自缓存命中（true=命中，未触网；false=Provider 新拉取）。
    pub cached: bool,
}

/// 缓存状态信息（数据源诊断用）。
#[derive(Debug, Clone)]
pub struct CacheInfo {
    /// 当前缓存市场数。
    pub size: usize,
    /// TTL（秒）。
    pub ttl_secs: u64,
    /// 当前是否新鲜（未过 TTL）。
    pub fresh: bool,
}

impl DataSourceManager {
    /// 按 `cfg.datasource.provider` 构造对应 Provider。
    ///
    /// - `gamma`：GammaProvider（自建 reqwest::Client，debug 由 log_level 决定）。
    /// - `mock`：MockProvider（内置市场，离线/测试）。
    /// - `clob`：尚未实现，返回明确错误（不静默回退）。
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let provider: Box<dyn MarketDataProvider> = match cfg.datasource.provider.as_str() {
            "gamma" => {
                let client = reqwest::Client::builder()
                    .user_agent("polymarket-scanner/1.0")
                    .timeout(Duration::from_secs(60))
                    .build()?;
                let debug = cfg.effective_log_level() >= LogLevel::Debug;
                Box::new(GammaProvider::new(client, debug))
            }
            "mock" => Box::new(MockProvider::default()),
            "clob" => {
                anyhow::bail!("CLOB Provider 尚未实现（V1.02 仅实现 gamma/mock）")
            }
            other => {
                anyhow::bail!(
                    "未知数据源 provider: {}（支持 gamma / clob / mock）",
                    other
                )
            }
        };
        let cache = MarketDataCache::new(Duration::from_secs(cfg.datasource.cache_ttl.max(1)));
        Ok(Self { provider, cache })
    }

    /// 当前 Provider 名称。
    pub fn name(&self) -> &str {
        self.provider.name()
    }

    /// 当前 Provider 能力。
    pub fn capability(&self) -> ProviderCapability {
        self.provider.capability()
    }

    /// 借用底层 Provider trait 对象（供 health 检查等复用）。
    pub fn provider(&self) -> &dyn MarketDataProvider {
        self.provider.as_ref()
    }

    /// 缓存状态信息（数据源诊断用）。
    pub fn cache_info(&self) -> CacheInfo {
        CacheInfo {
            size: self.cache.size(),
            ttl_secs: self.cache.ttl_secs(),
            fresh: self.cache.is_fresh(),
        }
    }

    /// 拉取市场：优先读缓存（命中则返回上次结果，未触网）；未命中则 Provider 拉取并写入缓存。
    ///
    /// 返回 [`FetchOutcome`]，`cached` 标记本次是否命中缓存。
    pub async fn fetch_markets(&mut self) -> Result<FetchOutcome> {
        if let Some((markets, stats)) = self.cache.get_fresh() {
            tracing::debug!(n = markets.len(), "数据源缓存命中");
            return Ok(FetchOutcome {
                markets,
                stats,
                cached: true,
            });
        }
        let FetchResult { markets, stats } = self.provider.fetch_markets().await?;
        tracing::debug!(n = markets.len(), "数据源缓存未命中，已刷新");
        self.cache.set(markets.clone(), stats.clone());
        Ok(FetchOutcome {
            markets,
            stats,
            cached: false,
        })
    }

    /// 健康检查。
    pub async fn health_check(&self) -> Result<HealthProbe> {
        self.provider.health_check().await
    }

    /// 打印【数据源能力】区块（第六节）。程序启动时调用。
    pub fn print_capability_block(&self) {
        let cap = self.capability();
        let real_arb = cap.supports_real_arbitrage();
        let title = format!("{} Provider", capitalize(self.name()));
        println!("{}", SEP);
        println!();
        println!("数据源能力");
        println!();
        println!("{}", DASH);
        println!();
        println!("{}", title);
        println!();
        println!("市场          : {}", yn(cap.supports_markets));
        println!("订单簿        : {}", yn(cap.supports_orderbook));
        println!("成交记录      : {}", yn(cap.supports_trades));
        println!("最优买卖价    : {}", yn(cap.supports_bid_ask));
        println!("流动性        : {}", yn(cap.supports_liquidity));
        println!("真实套利支持  : {}", yn(real_arb));
        if !real_arb {
            println!();
            println!("提示：当前数据源不支持真实套利（需订单簿 + 最优买卖价）。");
        }
        println!();
    }
}

/// ✅ / ❌ 文本。
fn yn(ok: bool) -> &'static str {
    if ok {
        "✅"
    } else {
        "❌"
    }
}

/// 首字母大写（用于标题）。
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_mock_builds_mock_provider() {
        let mut cfg = Config::default();
        cfg.datasource.provider = "mock".into();
        let m = DataSourceManager::from_config(&cfg).expect("mock manager");
        assert_eq!(m.name(), "mock");
        assert!(m.capability().supports_orderbook);
    }

    #[test]
    fn from_config_gamma_builds_gamma_provider() {
        let mut cfg = Config::default();
        cfg.datasource.provider = "gamma".into();
        let m = DataSourceManager::from_config(&cfg).expect("gamma manager");
        assert_eq!(m.name(), "gamma");
        assert!(!m.capability().supports_orderbook);
    }

    #[test]
    fn from_config_clob_errors_clearly() {
        let mut cfg = Config::default();
        cfg.datasource.provider = "clob".into();
        let err = DataSourceManager::from_config(&cfg).err().expect("应返回错误");
        assert!(format!("{:#}", err).contains("CLOB"));
    }

    #[test]
    fn from_config_unknown_errors_clearly() {
        let mut cfg = Config::default();
        cfg.datasource.provider = "polygon".into();
        let err = DataSourceManager::from_config(&cfg).err().expect("应返回错误");
        assert!(format!("{:#}", err).contains("未知数据源"));
    }
}
