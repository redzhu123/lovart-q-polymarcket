//! 缓存框架：统一的缓存 trait 及多种实现。
//!
//! 从 `pm-scanner::datasource::cache` 提取并统一。
//!
//! # 核心能力
//!
//! - [`Cache`] trait：统一的缓存接口
//! - [`MemoryCache`]：无界内存缓存
//! - [`TtlCache`]：带 TTL 过期时间的内存缓存
//! - [`LruCache`]：LRU 淘汰策略的内存缓存
//!
//! # 未来扩展
//!
//! - Redis 缓存（接口预留）

use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 缓存统计
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 淘汰次数
    pub evictions: u64,
    /// 当前条目数
    pub size: usize,
}

impl CacheStats {
    /// 命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.hits as f64 / total as f64) * 100.0
    }
}

/// 统一的缓存 trait
///
/// 使用 String 键和 serde_json::Value 值，避免 async-trait + 泛型的复杂性。
#[async_trait]
pub trait Cache: Send + Sync {
    /// 缓存名称
    fn name(&self) -> &str;

    /// 获取 JSON 值
    async fn get_json(&self, key: &str) -> Option<Value>;

    /// 设置 JSON 值
    async fn set_json(&self, key: &str, value: Value) -> anyhow::Result<()>;

    /// 删除键
    async fn remove(&self, key: &str) -> anyhow::Result<()>;

    /// 是否包含键
    async fn contains(&self, key: &str) -> bool;

    /// 清空缓存
    async fn clear(&self) -> anyhow::Result<()>;

    /// 当前条目数
    async fn size(&self) -> usize;

    /// 检查键是否新鲜（未过期）
    async fn is_fresh(&self, key: &str) -> bool;

    /// 获取统计
    fn stats(&self) -> CacheStats;
}

/// TTL 缓存条目
struct TtlEntry {
    value: Value,
    inserted_at: Instant,
}

/// 带 TTL 的内存缓存（从 MarketDataCache 提取）
pub struct TtlCache {
    name: String,
    ttl: Duration,
    entries: Arc<Mutex<HashMap<String, TtlEntry>>>,
    stats: Arc<Mutex<CacheStats>>,
}

impl TtlCache {
    /// 创建新的 TTL 缓存
    pub fn new(name: impl Into<String>, ttl: Duration) -> Self {
        Self {
            name: name.into(),
            ttl,
            entries: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(CacheStats::default())),
        }
    }
}

#[async_trait]
impl Cache for TtlCache {
    fn name(&self) -> &str {
        &self.name
    }

    async fn get_json(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.get(key) {
            if entry.inserted_at.elapsed() < self.ttl {
                stats.hits += 1;
                return Some(entry.value.clone());
            }
        }
        stats.misses += 1;
        None
    }

    async fn set_json(&self, key: &str, value: Value) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.insert(
            key.to_string(),
            TtlEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.remove(key);
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn contains(&self, key: &str) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.contains_key(key)
    }

    async fn clear(&self) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clear();
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = 0;
        Ok(())
    }

    async fn size(&self) -> usize {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.len()
    }

    async fn is_fresh(&self, key: &str) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.get(key) {
            entry.inserted_at.elapsed() < self.ttl
        } else {
            false
        }
    }

    fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        CacheStats {
            size: entries.len(),
            ..stats.clone()
        }
    }
}

/// 无界内存缓存
pub struct MemoryCache {
    name: String,
    entries: Arc<Mutex<HashMap<String, Value>>>,
    stats: Arc<Mutex<CacheStats>>,
}

impl MemoryCache {
    /// 创建新的内存缓存
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(CacheStats::default())),
        }
    }
}

#[async_trait]
impl Cache for MemoryCache {
    fn name(&self) -> &str {
        &self.name
    }

    async fn get_json(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = entries.get(key) {
            stats.hits += 1;
            return Some(v.clone());
        }
        stats.misses += 1;
        None
    }

    async fn set_json(&self, key: &str, value: Value) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.insert(key.to_string(), value);
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.remove(key);
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn contains(&self, key: &str) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.contains_key(key)
    }

    async fn clear(&self) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clear();
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = 0;
        Ok(())
    }

    async fn size(&self) -> usize {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.len()
    }

    async fn is_fresh(&self, _key: &str) -> bool {
        true // 内存缓存永不过期
    }

    fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        CacheStats {
            size: entries.len(),
            ..stats.clone()
        }
    }
}

/// LRU 淘汰缓存
pub struct LruCache {
    name: String,
    max_size: usize,
    entries: Arc<Mutex<HashMap<String, Value>>>,
    order: Arc<Mutex<VecDeque<String>>>,
    stats: Arc<Mutex<CacheStats>>,
}

impl LruCache {
    /// 创建新的 LRU 缓存
    pub fn new(name: impl Into<String>, max_size: usize) -> Self {
        Self {
            name: name.into(),
            max_size,
            entries: Arc::new(Mutex::new(HashMap::new())),
            order: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(Mutex::new(CacheStats::default())),
        }
    }

    fn evict_lru(&self) {
        if let Ok(mut order) = self.order.lock() {
            if let Some(oldest) = order.pop_front() {
                if let Ok(mut entries) = self.entries.lock() {
                    entries.remove(&oldest);
                }
                if let Ok(mut stats) = self.stats.lock() {
                    stats.evictions += 1;
                }
            }
        }
    }

    fn touch(&self, key: &str) {
        if let Ok(mut order) = self.order.lock() {
            order.retain(|k| k != key);
            order.push_back(key.to_string());
        }
    }
}

#[async_trait]
impl Cache for LruCache {
    fn name(&self) -> &str {
        &self.name
    }

    async fn get_json(&self, key: &str) -> Option<Value> {
        let hit = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(v) = entries.get(key) {
                stats.hits += 1;
                Some(v.clone())
            } else {
                stats.misses += 1;
                None
            }
        };
        if hit.is_some() {
            self.touch(key);
        }
        hit
    }

    async fn set_json(&self, key: &str, value: Value) -> anyhow::Result<()> {
        // 检查是否需要淘汰
        {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if entries.len() >= self.max_size && !entries.contains_key(key) {
                drop(entries);
                self.evict_lru();
            }
        }
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.insert(key.to_string(), value);
        }
        self.touch(key);
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.remove(key);
        let mut order = self.order.lock().unwrap_or_else(|e| e.into_inner());
        order.retain(|k| k != key);
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = entries.len();
        Ok(())
    }

    async fn contains(&self, key: &str) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.contains_key(key)
    }

    async fn clear(&self) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clear();
        let mut order = self.order.lock().unwrap_or_else(|e| e.into_inner());
        order.clear();
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.size = 0;
        Ok(())
    }

    async fn size(&self) -> usize {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.len()
    }

    async fn is_fresh(&self, _key: &str) -> bool {
        true
    }

    fn stats(&self) -> CacheStats {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        CacheStats {
            size: entries.len(),
            ..stats.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_cache_crud() {
        let cache = MemoryCache::new("test");
        assert_eq!(cache.size().await, 0);

        cache
            .set_json("k1", Value::String("v1".to_string()))
            .await
            .unwrap();
        assert_eq!(cache.size().await, 1);
        assert!(cache.contains("k1").await);

        let v = cache.get_json("k1").await;
        assert_eq!(v, Some(Value::String("v1".to_string())));

        cache.remove("k1").await.unwrap();
        assert!(!cache.contains("k1").await);
    }

    #[tokio::test]
    async fn memory_cache_hit_miss_stats() {
        let cache = MemoryCache::new("test");
        cache
            .set_json("a", Value::String("x".to_string()))
            .await
            .unwrap();
        let _ = cache.get_json("a").await; // hit
        let _ = cache.get_json("b").await; // miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn ttl_cache_expires() {
        let cache = TtlCache::new("test", Duration::from_millis(10));
        cache
            .set_json("k", Value::String("v".to_string()))
            .await
            .unwrap();
        assert!(cache.is_fresh("k").await);

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!cache.is_fresh("k").await);
        assert!(cache.get_json("k").await.is_none());
    }

    #[tokio::test]
    async fn lru_cache_evicts_oldest() {
        let cache = LruCache::new("test", 2);
        cache
            .set_json("a", Value::String("1".to_string()))
            .await
            .unwrap();
        cache
            .set_json("b", Value::String("2".to_string()))
            .await
            .unwrap();
        cache
            .set_json("c", Value::String("3".to_string()))
            .await
            .unwrap();
        // a 应该已被淘汰
        assert!(!cache.contains("a").await);
        assert!(cache.contains("b").await);
        assert!(cache.contains("c").await);
    }

    #[tokio::test]
    async fn lru_cache_touch_reorders() {
        let cache = LruCache::new("test", 2);
        cache
            .set_json("a", Value::String("1".to_string()))
            .await
            .unwrap();
        cache
            .set_json("b", Value::String("2".to_string()))
            .await
            .unwrap();
        // 访问 a，使其变为最近使用
        let _ = cache.get_json("a").await;
        cache
            .set_json("c", Value::String("3".to_string()))
            .await
            .unwrap();
        // b 应该被淘汰（a 被touch过）
        assert!(!cache.contains("b").await);
        assert!(cache.contains("a").await);
        assert!(cache.contains("c").await);
    }

    #[test]
    fn cache_stats_hit_rate() {
        let stats = CacheStats {
            hits: 75,
            misses: 25,
            evictions: 0,
            size: 0,
        };
        assert!((stats.hit_rate() - 75.0).abs() < 0.01);
    }
}
