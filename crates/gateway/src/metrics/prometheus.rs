//! Prometheus 风格 Metrics（P2-03）。
//!
//! 定义标准 Prometheus 数据模型：Counter / Gauge / Histogram。
//! 当前仅作为数据模型接口，不暴露 HTTP。
//! 以后可对接 Prometheus exporter 或 OpenTelemetry。
//!
//! # 数据模型
//!
//! - Counter：单调递增计数器（如 API 请求次数）。
//! - Gauge：可增可减的数值（如当前连接数）。
//! - Histogram：分桶统计（如 API 延迟分布）。
//!
//! # 输出格式
//!
//! Prometheus 文本格式：
//! ```text
//! # HELP gateway_api_requests_total API 请求总数
//! # TYPE gateway_api_requests_total counter
//! gateway_api_requests_total 1234
//! ```

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

// ============================================================================
// Counter
// ============================================================================

/// Prometheus Counter（单调递增计数器）。
pub struct Counter {
    /// 指标名称。
    name: &'static str,
    /// 帮助文本（中文）。
    help: &'static str,
    /// 当前值。
    value: AtomicU64,
}

impl Counter {
    /// 创建新计数器。
    pub const fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            value: AtomicU64::new(0),
        }
    }

    /// 增加 1。
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 增加指定值。
    pub fn inc_by(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    /// 当前值。
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// 重置。
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

// ============================================================================
// Gauge
// ============================================================================

/// Prometheus Gauge（可增可减数值）。
pub struct Gauge {
    /// 指标名称。
    name: &'static str,
    /// 帮助文本（中文）。
    help: &'static str,
    /// 当前值（使用 AtomicI64 支持负值）。
    value: AtomicI64,
}

impl Gauge {
    /// 创建新 Gauge。
    pub const fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            value: AtomicI64::new(0),
        }
    }

    /// 设置值。
    pub fn set(&self, v: i64) {
        self.value.store(v, Ordering::Relaxed);
    }

    /// 增加 1。
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// 减少 1。
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// 当前值。
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

// ============================================================================
// Histogram
// ============================================================================

/// Prometheus Histogram（分桶统计）。
///
/// 默认桶：[1, 5, 10, 50, 100, 500, 1000, 5000, 10000, 30000, 60000]（毫秒）。
pub struct Histogram {
    /// 指标名称。
    name: &'static str,
    /// 帮助文本（中文）。
    help: &'static str,
    /// 桶边界（递增）。
    buckets: &'static [f64],
    /// 各桶计数。
    counts: Vec<AtomicU64>,
    /// 总和。
    sum: Mutex<f64>,
    /// 观测次数。
    count: AtomicU64,
}

impl Histogram {
    /// 创建新 Histogram（默认桶，单位毫秒）。
    pub fn new(name: &'static str, help: &'static str) -> Self {
        let buckets: &'static [f64] = &[
            1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0, 30000.0, 60000.0,
        ];
        let counts: Vec<AtomicU64> = (0..=buckets.len()).map(|_| AtomicU64::new(0)).collect();
        Self {
            name,
            help,
            buckets,
            counts,
            sum: Mutex::new(0.0),
            count: AtomicU64::new(0),
        }
    }

    /// 观测一个值。
    pub fn observe(&self, value: f64) {
        if let Ok(mut s) = self.sum.lock() {
            *s += value;
        }
        self.count.fetch_add(1, Ordering::Relaxed);

        for (i, bound) in self.buckets.iter().enumerate() {
            if value <= *bound {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        // 永远落入 +Inf 桶（最后一桶）
        if let Some(last) = self.counts.last() {
            last.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 总观测次数。
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// 平均值。
    pub fn mean(&self) -> f64 {
        let c = self.count();
        if c == 0 {
            0.0
        } else {
            self.sum.lock().map(|s| *s / c as f64).unwrap_or(0.0)
        }
    }

    /// 各桶计数（含 +Inf）。
    pub fn bucket_counts(&self) -> Vec<u64> {
        self.counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect()
    }
}

// ============================================================================
// GatewayPrometheusMetrics
// ============================================================================

/// Gateway Prometheus Metrics 集合。
pub struct GatewayPrometheusMetrics {
    /// API 请求总数。
    pub api_requests_total: Counter,
    /// API 成功次数。
    pub api_requests_success: Counter,
    /// API 失败次数。
    pub api_requests_failure: Counter,
    /// API 延迟分布。
    pub api_latency_ms: Histogram,
    /// WebSocket 重连次数。
    pub ws_reconnects_total: Counter,
    /// WebSocket 断连次数。
    pub ws_disconnects_total: Counter,
    /// 速率限制触发次数。
    pub rate_limit_hits_total: Counter,
    /// 断路器跳闸次数。
    pub circuit_trips_total: Counter,
    /// 活跃订单数（Gauge）。
    pub active_orders: Gauge,
    /// WebSocket 连接状态（0/1）。
    pub ws_connected: Gauge,
}

impl GatewayPrometheusMetrics {
    /// 创建新的 Prometheus Metrics 集合。
    pub fn new() -> Self {
        Self {
            api_requests_total: Counter::new(
                "gateway_api_requests_total",
                "API 请求总数（所有调用）",
            ),
            api_requests_success: Counter::new(
                "gateway_api_requests_success",
                "API 成功次数（2xx）",
            ),
            api_requests_failure: Counter::new(
                "gateway_api_requests_failure",
                "API 失败次数（非 2xx）",
            ),
            api_latency_ms: Histogram::new("gateway_api_latency_ms", "API 请求延迟（毫秒）"),
            ws_reconnects_total: Counter::new(
                "gateway_ws_reconnects_total",
                "WebSocket 重连总次数",
            ),
            ws_disconnects_total: Counter::new(
                "gateway_ws_disconnects_total",
                "WebSocket 断连总次数",
            ),
            rate_limit_hits_total: Counter::new(
                "gateway_rate_limit_hits_total",
                "触发速率限制总次数",
            ),
            circuit_trips_total: Counter::new("gateway_circuit_trips_total", "断路器跳闸总次数"),
            active_orders: Gauge::new("gateway_active_orders", "当前活跃订单数"),
            ws_connected: Gauge::new(
                "gateway_ws_connected",
                "WebSocket 连接状态（0=未连接, 1=已连接）",
            ),
        }
    }

    /// 记录一次 API 调用。
    pub fn record_api_call(&self, latency_ms: u64, success: bool) {
        self.api_requests_total.inc();
        if success {
            self.api_requests_success.inc();
        } else {
            self.api_requests_failure.inc();
        }
        self.api_latency_ms.observe(latency_ms as f64);
    }

    /// 记录 WebSocket 重连。
    pub fn record_ws_reconnect(&self) {
        self.ws_reconnects_total.inc();
    }

    /// 记录 WebSocket 断连。
    pub fn record_ws_disconnect(&self) {
        self.ws_disconnects_total.inc();
    }

    /// 记录速率限制触发。
    pub fn record_rate_limit_hit(&self) {
        self.rate_limit_hits_total.inc();
    }

    /// 记录断路器跳闸。
    pub fn record_circuit_trip(&self) {
        self.circuit_trips_total.inc();
    }

    /// 设置 WebSocket 连接状态。
    pub fn set_ws_connected(&self, connected: bool) {
        self.ws_connected.set(if connected { 1 } else { 0 });
    }

    /// 设置活跃订单数。
    pub fn set_active_orders(&self, count: i64) {
        self.active_orders.set(count);
    }

    /// 生成 Prometheus 文本格式输出。
    pub fn to_prometheus_text(&self) -> String {
        let mut output = String::new();

        // HELP 和 TYPE 行 + counter 值
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.api_requests_total.name,
            self.api_requests_total.help,
            self.api_requests_total.name,
            self.api_requests_total.name,
            self.api_requests_total.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.api_requests_success.name,
            self.api_requests_success.help,
            self.api_requests_success.name,
            self.api_requests_success.name,
            self.api_requests_success.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.api_requests_failure.name,
            self.api_requests_failure.help,
            self.api_requests_failure.name,
            self.api_requests_failure.name,
            self.api_requests_failure.get()
        ));

        // Histogram
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} histogram\n",
            self.api_latency_ms.name, self.api_latency_ms.help, self.api_latency_ms.name
        ));
        let buckets = self.api_latency_ms.bucket_counts();
        // buckets 数组长度 = buckets.len() + 1（含 +Inf）
        // 实际桶索引：0..buckets.len()
        for (i, bound) in self.api_latency_ms.buckets.iter().enumerate() {
            output.push_str(&format!(
                "{}_bucket{{le=\"{}\"}} {}\n",
                self.api_latency_ms.name,
                bound,
                buckets.get(i).copied().unwrap_or(0)
            ));
        }
        output.push_str(&format!(
            "{}_bucket{{le=\"+Inf\"}} {}\n",
            self.api_latency_ms.name,
            buckets.last().copied().unwrap_or(0)
        ));
        output.push_str(&format!(
            "{}_sum {}\n",
            self.api_latency_ms.name,
            self.api_latency_ms.sum.lock().map(|s| *s).unwrap_or(0.0)
        ));
        output.push_str(&format!(
            "{}_count {}\n",
            self.api_latency_ms.name,
            self.api_latency_ms.count()
        ));

        // 其余 counters
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.ws_reconnects_total.name,
            self.ws_reconnects_total.help,
            self.ws_reconnects_total.name,
            self.ws_reconnects_total.name,
            self.ws_reconnects_total.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.ws_disconnects_total.name,
            self.ws_disconnects_total.help,
            self.ws_disconnects_total.name,
            self.ws_disconnects_total.name,
            self.ws_disconnects_total.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.rate_limit_hits_total.name,
            self.rate_limit_hits_total.help,
            self.rate_limit_hits_total.name,
            self.rate_limit_hits_total.name,
            self.rate_limit_hits_total.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} counter\n{} {}\n",
            self.circuit_trips_total.name,
            self.circuit_trips_total.help,
            self.circuit_trips_total.name,
            self.circuit_trips_total.name,
            self.circuit_trips_total.get()
        ));

        // Gauges
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} gauge\n{} {}\n",
            self.active_orders.name,
            self.active_orders.help,
            self.active_orders.name,
            self.active_orders.name,
            self.active_orders.get()
        ));
        output.push_str(&format!(
            "# HELP {} {}\n# TYPE {} gauge\n{} {}\n",
            self.ws_connected.name,
            self.ws_connected.help,
            self.ws_connected.name,
            self.ws_connected.name,
            self.ws_connected.get()
        ));

        output
    }
}

impl Default for GatewayPrometheusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_basic() {
        let c = Counter::new("test", "测试");
        assert_eq!(c.get(), 0);
        c.inc();
        c.inc();
        c.inc_by(5);
        assert_eq!(c.get(), 7);
        c.reset();
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn gauge_basic() {
        let g = Gauge::new("test", "测试");
        assert_eq!(g.get(), 0);
        g.inc();
        g.inc();
        assert_eq!(g.get(), 2);
        g.dec();
        assert_eq!(g.get(), 1);
        g.set(100);
        assert_eq!(g.get(), 100);
        g.set(-50);
        assert_eq!(g.get(), -50);
    }

    #[test]
    fn histogram_observe() {
        let h = Histogram::new("test", "测试");
        h.observe(50.0);
        h.observe(150.0);
        h.observe(5000.0);

        assert_eq!(h.count(), 3);
        assert!((h.mean() - 1733.33).abs() < 1.0);
    }

    #[test]
    fn gateway_metrics_basic() {
        let m = GatewayPrometheusMetrics::new();
        m.record_api_call(50, true);
        m.record_api_call(100, false);
        m.record_ws_reconnect();
        m.record_rate_limit_hit();
        m.record_circuit_trip();
        m.set_ws_connected(true);
        m.set_active_orders(5);

        assert_eq!(m.api_requests_total.get(), 2);
        assert_eq!(m.api_requests_success.get(), 1);
        assert_eq!(m.api_requests_failure.get(), 1);
        assert_eq!(m.ws_reconnects_total.get(), 1);
        assert_eq!(m.rate_limit_hits_total.get(), 1);
        assert_eq!(m.circuit_trips_total.get(), 1);
        assert_eq!(m.ws_connected.get(), 1);
        assert_eq!(m.active_orders.get(), 5);
    }

    #[test]
    fn prometheus_text_output() {
        let m = GatewayPrometheusMetrics::new();
        m.record_api_call(50, true);
        let text = m.to_prometheus_text();
        assert!(text.contains("# HELP gateway_api_requests_total"));
        assert!(text.contains("# TYPE gateway_api_requests_total counter"));
        assert!(text.contains("gateway_api_requests_total 1"));
        assert!(text.contains("# TYPE gateway_api_latency_ms histogram"));
        assert!(text.contains("gateway_api_latency_ms_bucket"));
        assert!(text.contains("gateway_api_latency_ms_sum"));
        assert!(text.contains("gateway_api_latency_ms_count"));
    }

    #[test]
    fn prometheus_text_has_all_metrics() {
        let m = GatewayPrometheusMetrics::new();
        let text = m.to_prometheus_text();
        // 应包含所有指标
        assert!(text.contains("gateway_api_requests_total"));
        assert!(text.contains("gateway_api_requests_success"));
        assert!(text.contains("gateway_api_requests_failure"));
        assert!(text.contains("gateway_api_latency_ms"));
        assert!(text.contains("gateway_ws_reconnects_total"));
        assert!(text.contains("gateway_ws_disconnects_total"));
        assert!(text.contains("gateway_rate_limit_hits_total"));
        assert!(text.contains("gateway_circuit_trips_total"));
        assert!(text.contains("gateway_active_orders"));
        assert!(text.contains("gateway_ws_connected"));
    }
}
