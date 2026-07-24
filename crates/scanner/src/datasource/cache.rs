//! Market Data Cache（V1.02 第八节）。
//!
//! 仅内存缓存（无 Redis / 无数据库）。Scanner 优先读缓存，Provider 负责刷新。
//! TTL 默认 10 秒（来自 `config.datasource.cache_ttl`）。

use std::time::{Duration, Instant};

use pm_models::UnifiedMarket;

use crate::stats::FetchStats;

/// 缓存条目：市场列表 + 上次拉取统计 + 拉取时刻。
struct CachedEntry {
    markets: Vec<UnifiedMarket>,
    stats: FetchStats,
    fetched_at: Instant,
}

/// 内存缓存。命中（未过 TTL）时返回上次结果，否则需 Provider 刷新。
pub struct MarketDataCache {
    ttl: Duration,
    entry: Option<CachedEntry>,
}

impl MarketDataCache {
    /// 构造：指定 TTL。
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, entry: None }
    }

    /// TTL（秒）。
    pub fn ttl_secs(&self) -> u64 {
        self.ttl.as_secs()
    }

    /// 当前缓存的市场数（未缓存为 0）。
    pub fn size(&self) -> usize {
        self.entry.as_ref().map(|e| e.markets.len()).unwrap_or(0)
    }

    /// 是否存在且未过 TTL。
    pub fn is_fresh(&self) -> bool {
        match &self.entry {
            Some(e) => e.fetched_at.elapsed() < self.ttl,
            None => false,
        }
    }

    /// 取出新鲜缓存（克隆返回，避免借用冲突）。过期或空返回 None。
    pub fn get_fresh(&self) -> Option<(Vec<UnifiedMarket>, FetchStats)> {
        let e = self.entry.as_ref()?;
        if e.fetched_at.elapsed() < self.ttl {
            Some((e.markets.clone(), e.stats.clone()))
        } else {
            None
        }
    }

    /// 写入缓存（覆盖旧条目）。
    pub fn set(&mut self, markets: Vec<UnifiedMarket>, stats: FetchStats) {
        self.entry = Some(CachedEntry {
            markets,
            stats,
            fetched_at: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn sample_market(id: &str) -> UnifiedMarket {
        UnifiedMarket {
            market_id: id.into(),
            question: id.into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: Some(0.4),
            no_price: Some(0.5),
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 2,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_cache_not_fresh() {
        let c = MarketDataCache::new(Duration::from_secs(10));
        assert!(!c.is_fresh());
        assert_eq!(c.size(), 0);
        assert!(c.get_fresh().is_none());
    }

    #[test]
    fn set_then_get_fresh() {
        let mut c = MarketDataCache::new(Duration::from_secs(10));
        c.set(vec![sample_market("m1")], FetchStats::default());
        assert!(c.is_fresh());
        assert_eq!(c.size(), 1);
        let (m, _s) = c.get_fresh().expect("fresh");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].market_id, "m1");
    }

    #[test]
    fn expired_not_fresh() {
        // TTL=0 -> 立即过期
        let mut c = MarketDataCache::new(Duration::from_secs(0));
        c.set(vec![sample_market("m1")], FetchStats::default());
        // elapsed >= ttl(0) 视为过期
        assert!(!c.is_fresh());
        assert!(c.get_fresh().is_none());
    }

    #[test]
    fn set_overwrites() {
        let mut c = MarketDataCache::new(Duration::from_secs(10));
        c.set(vec![sample_market("m1")], FetchStats::default());
        c.set(
            vec![sample_market("m2"), sample_market("m3")],
            FetchStats::default(),
        );
        assert_eq!(c.size(), 2);
        let (m, _) = c.get_fresh().expect("fresh");
        assert_eq!(m.len(), 2);
    }
}
