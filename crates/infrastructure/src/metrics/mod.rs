//! 指标收集模块：统一的指标原语和收集器。
//!
//! 从 `pm-gateway::metrics::prometheus` 提取并统一。
//!
//! # 核心能力
//!
//! - [`Counter`]：单调递增计数器（AtomicU64）
//! - [`Gauge`]：可增可减的仪表盘（AtomicI64）
//! - [`Histogram`]：分桶直方图
//! - [`MetricsCollector`] trait：统一的指标收集接口
//! - [`InfrastructureMetrics`]：默认实现
//! - Prometheus 文本格式输出

use chrono::{DateTime, Local};
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

pub mod prometheus;

/// 单调递增计数器
pub struct Counter {
    pub name: &'static str,
    pub help: &'static str,
    value: AtomicU64,
}

impl Counter {
    /// 创建新的计数器
    pub const fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            value: AtomicU64::new(0),
        }
    }

    /// 递增 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 递增指定值
    pub fn inc_by(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    /// 获取当前值
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// 重置为 0
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

/// 可增可减的仪表盘
pub struct Gauge {
    pub name: &'static str,
    pub help: &'static str,
    value: AtomicI64,
}

impl Gauge {
    /// 创建新的仪表盘
    pub const fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            value: AtomicI64::new(0),
        }
    }

    /// 设置当前值
    pub fn set(&self, v: i64) {
        self.value.store(v, Ordering::Relaxed);
    }

    /// 递增 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 递减 1
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// 获取当前值
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// 分桶直方图（毫秒级延迟默认分桶）
pub struct Histogram {
    pub name: &'static str,
    pub help: &'static str,
    /// 分桶边界（毫秒）
    pub buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
    sum: Mutex<f64>,
    count: AtomicU64,
}

/// 默认的毫秒级延迟分桶
pub const DEFAULT_LATENCY_BUCKETS: &[f64] = &[
    1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
];

impl Histogram {
    /// 创建新的直方图，使用默认分桶
    pub fn new(name: &'static str, help: &'static str) -> Self {
        let buckets = DEFAULT_LATENCY_BUCKETS.to_vec();
        let counts = buckets
            .iter()
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>();
        Self {
            name,
            help,
            buckets,
            counts,
            sum: Mutex::new(0.0),
            count: AtomicU64::new(0),
        }
    }

    /// 创建带自定义分桶的直方图
    pub fn with_buckets(name: &'static str, help: &'static str, buckets: &[f64]) -> Self {
        let buckets = buckets.to_vec();
        let counts = buckets
            .iter()
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>();
        Self {
            name,
            help,
            buckets,
            counts,
            sum: Mutex::new(0.0),
            count: AtomicU64::new(0),
        }
    }

    /// 观测一个值
    pub fn observe(&self, value: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut sum) = self.sum.lock() {
            *sum += value;
        }
        for (i, &bound) in self.buckets.iter().enumerate() {
            if value <= bound {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 观测总数
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// 平均值
    pub fn mean(&self) -> f64 {
        let c = self.count();
        if c == 0 {
            return 0.0;
        }
        let sum = self.sum.lock().unwrap_or_else(|e| e.into_inner());
        *sum / c as f64
    }

    /// 各桶计数
    pub fn bucket_counts(&self) -> Vec<(f64, u64)> {
        self.buckets
            .iter()
            .zip(self.counts.iter())
            .map(|(&b, c)| (b, c.load(Ordering::Relaxed)))
            .collect()
    }

    /// 总和
    pub fn sum(&self) -> f64 {
        *self.sum.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 重置
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        if let Ok(mut sum) = self.sum.lock() {
            *sum = 0.0;
        }
        for c in &self.counts {
            c.store(0, Ordering::Relaxed);
        }
    }
}

/// 指标快照
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// 时间戳
    pub timestamp: DateTime<Local>,
    /// API 调用总数
    pub total_api_calls: u64,
    /// 平均 API 延迟（毫秒）
    pub avg_api_latency_ms: f64,
    /// HTTP 成功率
    pub http_success_rate: f64,
    /// 提交订单数
    pub orders_submitted: u64,
    /// 成交订单数
    pub orders_filled: u64,
    /// 取消订单数
    pub orders_cancelled: u64,
    /// 拒绝订单数
    pub orders_rejected: u64,
    /// 事件总线已发布事件数
    pub events_published: u64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 调度器执行次数
    pub scheduler_executions: u64,
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self {
            timestamp: Local::now(),
            total_api_calls: 0,
            avg_api_latency_ms: 0.0,
            http_success_rate: 100.0,
            orders_submitted: 0,
            orders_filled: 0,
            orders_cancelled: 0,
            orders_rejected: 0,
            events_published: 0,
            cache_hit_rate: 0.0,
            scheduler_executions: 0,
        }
    }
}

/// 统一的指标收集 trait
pub trait MetricsCollector: Send + Sync {
    /// 收集器名称
    fn name(&self) -> &str;

    /// 记录一次 API 调用
    fn record_api_call(&self, latency_ms: u64, success: bool);

    /// 记录一次订单事件
    fn record_order_event(&self, event_type: &str);

    /// 记录缓存命中
    fn record_cache_hit(&self);

    /// 记录缓存未命中
    fn record_cache_miss(&self);

    /// 记录调度器执行
    fn record_scheduler_execution(&self);

    /// 记录事件发布
    fn record_event_published(&self);

    /// 获取快照
    fn snapshot(&self) -> MetricsSnapshot;

    /// 中文报告
    fn report_zh(&self) -> String;

    /// 中文摘要
    fn summary_zh(&self) -> String;
}

/// 基础设施指标默认实现
pub struct InfrastructureMetrics {
    name: String,
    pub api_calls: Counter,
    pub api_successes: Counter,
    pub api_failures: Counter,
    pub api_latency: Histogram,
    pub orders_submitted: Counter,
    pub orders_filled: Counter,
    pub orders_cancelled: Counter,
    pub orders_rejected: Counter,
    pub cache_hits: Counter,
    pub cache_misses: Counter,
    pub events_published: Counter,
    pub scheduler_executions: Counter,
}

impl InfrastructureMetrics {
    /// 创建新的基础设施指标收集器
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name,
            api_calls: Counter::new("infra_api_calls_total", "API 调用总数"),
            api_successes: Counter::new("infra_api_successes_total", "API 成功调用数"),
            api_failures: Counter::new("infra_api_failures_total", "API 失败调用数"),
            api_latency: Histogram::new("infra_api_latency_ms", "API 延迟（毫秒）"),
            orders_submitted: Counter::new("infra_orders_submitted", "提交订单数"),
            orders_filled: Counter::new("infra_orders_filled", "成交订单数"),
            orders_cancelled: Counter::new("infra_orders_cancelled", "取消订单数"),
            orders_rejected: Counter::new("infra_orders_rejected", "拒绝订单数"),
            cache_hits: Counter::new("infra_cache_hits", "缓存命中次数"),
            cache_misses: Counter::new("infra_cache_misses", "缓存未命中次数"),
            events_published: Counter::new("infra_events_published", "事件发布数"),
            scheduler_executions: Counter::new("infra_scheduler_executions", "调度器执行次数"),
        }
    }
}

impl MetricsCollector for InfrastructureMetrics {
    fn name(&self) -> &str {
        &self.name
    }

    fn record_api_call(&self, latency_ms: u64, success: bool) {
        self.api_calls.inc();
        self.api_latency.observe(latency_ms as f64);
        if success {
            self.api_successes.inc();
        } else {
            self.api_failures.inc();
        }
    }

    fn record_order_event(&self, event_type: &str) {
        match event_type {
            "submitted" | "OrderCreated" => self.orders_submitted.inc(),
            "filled" | "OrderFilled" => self.orders_filled.inc(),
            "cancelled" | "OrderCancelled" => self.orders_cancelled.inc(),
            "rejected" | "OrderRejected" => self.orders_rejected.inc(),
            _ => {}
        }
    }

    fn record_cache_hit(&self) {
        self.cache_hits.inc();
    }

    fn record_cache_miss(&self) {
        self.cache_misses.inc();
    }

    fn record_scheduler_execution(&self) {
        self.scheduler_executions.inc();
    }

    fn record_event_published(&self) {
        self.events_published.inc();
    }

    fn snapshot(&self) -> MetricsSnapshot {
        let api_calls = self.api_calls.get();
        let api_successes = self.api_successes.get();
        let cache_hits = self.cache_hits.get();
        let cache_misses = self.cache_misses.get();
        let total_cache = cache_hits + cache_misses;

        MetricsSnapshot {
            timestamp: Local::now(),
            total_api_calls: api_calls,
            avg_api_latency_ms: self.api_latency.mean(),
            http_success_rate: if api_calls > 0 {
                (api_successes as f64 / api_calls as f64) * 100.0
            } else {
                100.0
            },
            orders_submitted: self.orders_submitted.get(),
            orders_filled: self.orders_filled.get(),
            orders_cancelled: self.orders_cancelled.get(),
            orders_rejected: self.orders_rejected.get(),
            events_published: self.events_published.get(),
            cache_hit_rate: if total_cache > 0 {
                (cache_hits as f64 / total_cache as f64) * 100.0
            } else {
                0.0
            },
            scheduler_executions: self.scheduler_executions.get(),
        }
    }

    fn report_zh(&self) -> String {
        let snap = self.snapshot();
        format!(
            "══════ 指标报告 ══════\n\
             时间: {}\n\
             API 调用: {} (成功率 {:.1}%, 平均延迟 {:.1}ms)\n\
             订单: 提交 {} / 成交 {} / 取消 {} / 拒绝 {}\n\
             缓存命中率: {:.1}% (命中 {} / 未命中 {})\n\
             事件发布: {}\n\
             调度器执行: {}\n\
             ══════════════════════",
            snap.timestamp.format("%Y-%m-%d %H:%M:%S"),
            snap.total_api_calls,
            snap.http_success_rate,
            snap.avg_api_latency_ms,
            snap.orders_submitted,
            snap.orders_filled,
            snap.orders_cancelled,
            snap.orders_rejected,
            snap.cache_hit_rate,
            self.cache_hits.get(),
            self.cache_misses.get(),
            snap.events_published,
            snap.scheduler_executions,
        )
    }

    fn summary_zh(&self) -> String {
        let snap = self.snapshot();
        format!(
            "API:{}/{} ({:.1}%) | 订单:S{}/F{}/C{}/R{} | 缓存:{:.1}% | 事件:{} | 调度:{}",
            snap.total_api_calls,
            self.api_successes.get(),
            snap.http_success_rate,
            snap.orders_submitted,
            snap.orders_filled,
            snap.orders_cancelled,
            snap.orders_rejected,
            snap.cache_hit_rate,
            snap.events_published,
            snap.scheduler_executions,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_inc_and_reset() {
        let c = Counter::new("test_counter", "test help");
        c.inc();
        c.inc();
        assert_eq!(c.get(), 2);
        c.inc_by(3);
        assert_eq!(c.get(), 5);
        c.reset();
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn gauge_set_and_inc_dec() {
        let g = Gauge::new("test_gauge", "test help");
        g.set(42);
        assert_eq!(g.get(), 42);
        g.inc();
        assert_eq!(g.get(), 43);
        g.dec();
        assert_eq!(g.get(), 42);
    }

    #[test]
    fn histogram_basic() {
        let h = Histogram::new("test_latency", "test help");
        h.observe(50.0);
        h.observe(150.0);
        assert_eq!(h.count(), 2);
        assert!(h.mean() > 0.0);
        let buckets = h.bucket_counts();
        assert!(!buckets.is_empty());
    }

    #[test]
    fn histogram_reset() {
        let h = Histogram::new("test_latency", "test help");
        h.observe(100.0);
        h.reset();
        assert_eq!(h.count(), 0);
    }

    #[test]
    fn infrastructure_metrics_records() {
        let m = InfrastructureMetrics::new("test");
        m.record_api_call(50, true);
        m.record_api_call(200, false);
        m.record_order_event("filled");
        m.record_cache_hit();
        m.record_cache_miss();
        m.record_event_published();
        m.record_scheduler_execution();

        let snap = m.snapshot();
        assert_eq!(snap.total_api_calls, 2);
        assert_eq!(snap.orders_filled, 1);
        assert_eq!(snap.events_published, 1);
        assert_eq!(snap.scheduler_executions, 1);
    }

    #[test]
    fn infrastructure_metrics_zh_output() {
        let m = InfrastructureMetrics::new("test");
        m.record_api_call(10, true);
        let report = m.report_zh();
        assert!(report.contains("指标报告"));
        let summary = m.summary_zh();
        assert!(summary.contains("API:"));
    }

    #[test]
    fn metrics_snapshot_default() {
        let snap = MetricsSnapshot::default();
        assert_eq!(snap.http_success_rate, 100.0);
    }
}
