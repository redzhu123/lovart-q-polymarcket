//! Gateway Metrics（V1.08 第八节 / P2-03 扩展）。
//!
//! 统计：API 延迟 / HTTP 成功率 / WS 重连次数 / 订单成功率 / 同步耗时。
//! 全部中文。
//!
//! # 模块
//!
//! - `mod`：基础指标收集器。
//! - [`prometheus`]：Prometheus 风格指标（Counter / Gauge / Histogram）。

pub mod prometheus;

use chrono::{DateTime, Local};
use serde::Serialize;
use std::time::Duration;

// ============================================================================
// GatewayMetrics
// ============================================================================

/// Gateway 指标收集器。
#[derive(Debug, Clone)]
pub struct GatewayMetrics {
    // ---- API 延迟 ----
    /// 累计 API 调用次数。
    pub total_api_calls: u64,
    /// 累计 API 延迟（毫秒）。
    pub total_api_latency_ms: u64,
    /// 最小 API 延迟（毫秒）。
    pub min_api_latency_ms: u64,
    /// 最大 API 延迟（毫秒）。
    pub max_api_latency_ms: u64,
    /// 最近一次 API 延迟（毫秒）。
    pub last_api_latency_ms: u64,

    // ---- HTTP 成功率 ----
    /// HTTP 成功次数（2xx）。
    pub http_successes: u64,
    /// HTTP 失败次数（非 2xx / 网络错误）。
    pub http_failures: u64,
    /// HTTP 总请求数。
    pub total_http_requests: u64,

    // ---- WebSocket ----
    /// WS 总重连次数。
    pub ws_reconnects: u64,
    /// WS 总断连次数。
    pub ws_disconnects: u64,
    /// WS 最后连接时间。
    pub ws_last_connect: Option<DateTime<Local>>,
    /// WS 最后断连时间。
    pub ws_last_disconnect: Option<DateTime<Local>>,

    // ---- 订单 ----
    /// 总提交订单数。
    pub total_orders_submitted: u64,
    /// 总成交订单数。
    pub total_orders_filled: u64,
    /// 总取消订单数。
    pub total_orders_cancelled: u64,
    /// 总拒绝订单数。
    pub total_orders_rejected: u64,
    /// 总过期订单数。
    pub total_orders_expired: u64,
    /// 总失败订单数。
    pub total_orders_failed: u64,

    // ---- 同步 ----
    /// 订单同步次数。
    pub sync_count: u64,
    /// 订单同步累计耗时（毫秒）。
    pub total_sync_latency_ms: u64,
    /// 余额同步次数。
    pub balance_sync_count: u64,
    /// 余额同步累计耗时（毫秒）。
    pub total_balance_sync_latency_ms: u64,
    /// 持仓同步次数。
    pub position_sync_count: u64,
    /// 持仓同步累计耗时（毫秒）。
    pub total_position_sync_latency_ms: u64,

    // ---- 重试 ----
    /// 总重试次数。
    pub total_retries: u64,
    /// 断路器跳闸次数。
    pub circuit_trips: u64,

    // ---- 时间 ----
    /// 指标开始时间。
    pub started_at: DateTime<Local>,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,
}

impl GatewayMetrics {
    /// 创建新的指标收集器。
    pub fn new() -> Self {
        let now = Local::now();
        Self {
            total_api_calls: 0,
            total_api_latency_ms: 0,
            min_api_latency_ms: u64::MAX,
            max_api_latency_ms: 0,
            last_api_latency_ms: 0,
            http_successes: 0,
            http_failures: 0,
            total_http_requests: 0,
            ws_reconnects: 0,
            ws_disconnects: 0,
            ws_last_connect: None,
            ws_last_disconnect: None,
            total_orders_submitted: 0,
            total_orders_filled: 0,
            total_orders_cancelled: 0,
            total_orders_rejected: 0,
            total_orders_expired: 0,
            total_orders_failed: 0,
            sync_count: 0,
            total_sync_latency_ms: 0,
            balance_sync_count: 0,
            total_balance_sync_latency_ms: 0,
            position_sync_count: 0,
            total_position_sync_latency_ms: 0,
            total_retries: 0,
            circuit_trips: 0,
            started_at: now,
            updated_at: now,
        }
    }

    // ---- 记录方法 ----

    /// 记录一次 API 调用。
    pub fn record_api_call(&mut self, latency_ms: u64, success: bool) {
        self.total_api_calls += 1;
        self.total_api_latency_ms += latency_ms;
        self.last_api_latency_ms = latency_ms;
        if latency_ms < self.min_api_latency_ms {
            self.min_api_latency_ms = latency_ms;
        }
        if latency_ms > self.max_api_latency_ms {
            self.max_api_latency_ms = latency_ms;
        }
        self.total_http_requests += 1;
        if success {
            self.http_successes += 1;
        } else {
            self.http_failures += 1;
        }
        self.updated_at = Local::now();
    }

    /// 记录订单提交。
    pub fn record_order_submitted(&mut self) {
        self.total_orders_submitted += 1;
        self.updated_at = Local::now();
    }

    /// 记录订单成交。
    pub fn record_order_filled(&mut self) {
        self.total_orders_filled += 1;
        self.updated_at = Local::now();
    }

    /// 记录订单取消。
    pub fn record_order_cancelled(&mut self) {
        self.total_orders_cancelled += 1;
        self.updated_at = Local::now();
    }

    /// 记录订单拒绝。
    pub fn record_order_rejected(&mut self) {
        self.total_orders_rejected += 1;
        self.updated_at = Local::now();
    }

    /// 记录订单过期。
    pub fn record_order_expired(&mut self) {
        self.total_orders_expired += 1;
        self.updated_at = Local::now();
    }

    /// 记录订单失败。
    pub fn record_order_failed(&mut self) {
        self.total_orders_failed += 1;
        self.updated_at = Local::now();
    }

    /// 记录 WebSocket 重连。
    pub fn record_ws_reconnect(&mut self) {
        self.ws_reconnects += 1;
        self.ws_last_connect = Some(Local::now());
        self.updated_at = Local::now();
    }

    /// 记录 WebSocket 断连。
    pub fn record_ws_disconnect(&mut self) {
        self.ws_disconnects += 1;
        self.ws_last_disconnect = Some(Local::now());
        self.updated_at = Local::now();
    }

    /// 记录同步。
    pub fn record_sync(&mut self, latency_ms: u64) {
        self.sync_count += 1;
        self.total_sync_latency_ms += latency_ms;
        self.updated_at = Local::now();
    }

    /// 记录余额同步。
    pub fn record_balance_sync(&mut self, latency_ms: u64) {
        self.balance_sync_count += 1;
        self.total_balance_sync_latency_ms += latency_ms;
        self.updated_at = Local::now();
    }

    /// 记录持仓同步。
    pub fn record_position_sync(&mut self, latency_ms: u64) {
        self.position_sync_count += 1;
        self.total_position_sync_latency_ms += latency_ms;
        self.updated_at = Local::now();
    }

    /// 记录重试。
    pub fn record_retry(&mut self) {
        self.total_retries += 1;
        self.updated_at = Local::now();
    }

    /// 记录断路器跳闸。
    pub fn record_circuit_trip(&mut self) {
        self.circuit_trips += 1;
        self.updated_at = Local::now();
    }

    // ---- 计算指标 ----

    /// 平均 API 延迟（毫秒）。
    pub fn avg_api_latency_ms(&self) -> f64 {
        if self.total_api_calls == 0 {
            0.0
        } else {
            self.total_api_latency_ms as f64 / self.total_api_calls as f64
        }
    }

    /// HTTP 成功率（0.0 ~ 1.0）。
    pub fn http_success_rate(&self) -> f64 {
        if self.total_http_requests == 0 {
            1.0
        } else {
            self.http_successes as f64 / self.total_http_requests as f64
        }
    }

    /// 订单成功率（成交 / 提交，含部分成交）。
    pub fn order_success_rate(&self) -> f64 {
        if self.total_orders_submitted == 0 {
            1.0
        } else {
            self.total_orders_filled as f64 / self.total_orders_submitted as f64
        }
    }

    /// 平均同步耗时（毫秒）。
    pub fn avg_sync_latency_ms(&self) -> f64 {
        if self.sync_count == 0 {
            0.0
        } else {
            self.total_sync_latency_ms as f64 / self.sync_count as f64
        }
    }

    /// 运行时长。
    pub fn uptime(&self) -> Duration {
        let elapsed = Local::now() - self.started_at;
        elapsed.to_std().unwrap_or(Duration::from_secs(0))
    }

    // ---- 中文报告 ----

    /// 完整中文指标报告。
    pub fn report_zh(&self) -> String {
        format!(
            "══════════════════════════════════════════════════\n\
             【Gateway 指标报告】\n\
             ══════════════════════════════════════════════════\n\
             \n\
             ── API 延迟 ──\n\
               API 调用次数   : {}\n\
               平均延迟       : {:.1} ms\n\
               最小延迟       : {} ms\n\
               最大延迟       : {} ms\n\
               最近延迟       : {} ms\n\
             \n\
             ── HTTP ──\n\
               总请求数       : {}\n\
               成功           : {}\n\
               失败           : {}\n\
               成功率         : {:.1}%\n\
             \n\
             ── WebSocket ──\n\
               重连次数       : {}\n\
               断连次数       : {}\n\
             \n\
             ── 订单 ──\n\
               总提交         : {}\n\
               成交           : {}\n\
               取消           : {}\n\
               拒绝           : {}\n\
               过期           : {}\n\
               失败           : {}\n\
               订单成功率     : {:.1}%\n\
             \n\
             ── 同步 ──\n\
               订单同步次数   : {}  平均耗时: {:.1} ms\n\
               余额同步次数   : {}  平均耗时: {:.1} ms\n\
               持仓同步次数   : {}  平均耗时: {:.1} ms\n\
             \n\
             ── 重试 ──\n\
               总重试次数     : {}\n\
               断路器跳闸     : {}\n\
             \n\
             ── 运行时长 ──\n\
               {} \n\
             ══════════════════════════════════════════════════",
            self.total_api_calls,
            self.avg_api_latency_ms(),
            if self.min_api_latency_ms == u64::MAX {
                0
            } else {
                self.min_api_latency_ms
            },
            self.max_api_latency_ms,
            self.last_api_latency_ms,
            self.total_http_requests,
            self.http_successes,
            self.http_failures,
            self.http_success_rate() * 100.0,
            self.ws_reconnects,
            self.ws_disconnects,
            self.total_orders_submitted,
            self.total_orders_filled,
            self.total_orders_cancelled,
            self.total_orders_rejected,
            self.total_orders_expired,
            self.total_orders_failed,
            self.order_success_rate() * 100.0,
            self.sync_count,
            self.avg_sync_latency_ms(),
            self.balance_sync_count,
            if self.balance_sync_count > 0 {
                self.total_balance_sync_latency_ms as f64 / self.balance_sync_count as f64
            } else {
                0.0
            },
            self.position_sync_count,
            if self.position_sync_count > 0 {
                self.total_position_sync_latency_ms as f64 / self.position_sync_count as f64
            } else {
                0.0
            },
            self.total_retries,
            self.circuit_trips,
            self.format_uptime(),
        )
    }

    /// 简短中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "API 延迟: {:.1}ms | HTTP 成功率: {:.1}% | 订单成功率: {:.1}% | WS 重连: {} | 重试: {} | 熔断: {}",
            self.avg_api_latency_ms(),
            self.http_success_rate() * 100.0,
            self.order_success_rate() * 100.0,
            self.ws_reconnects,
            self.total_retries,
            self.circuit_trips,
        )
    }

    /// 格式化运行时长。
    fn format_uptime(&self) -> String {
        let uptime = self.uptime();
        let secs = uptime.as_secs();
        if secs < 60 {
            format!("{} 秒", secs)
        } else if secs < 3600 {
            format!("{} 分 {} 秒", secs / 60, secs % 60)
        } else {
            format!("{} 时 {} 分", secs / 3600, (secs % 3600) / 60)
        }
    }
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Gateway Metrics CSV 记录。
#[derive(Debug, Clone, Serialize)]
pub struct GatewayMetricsRecord {
    pub timestamp: String,
    pub total_api_calls: u64,
    pub avg_api_latency_ms: f64,
    pub http_success_rate: f64,
    pub ws_reconnects: u64,
    pub total_orders_submitted: u64,
    pub total_orders_filled: u64,
    pub order_success_rate: f64,
    pub sync_count: u64,
    pub avg_sync_latency_ms: f64,
    pub total_retries: u64,
    pub circuit_trips: u64,
}

impl GatewayMetricsRecord {
    pub fn header() -> &'static str {
        "timestamp,total_api_calls,avg_api_latency_ms,http_success_rate,ws_reconnects,total_orders_submitted,total_orders_filled,order_success_rate,sync_count,avg_sync_latency_ms,total_retries,circuit_trips"
    }

    pub fn from_metrics(m: &GatewayMetrics) -> Self {
        Self {
            timestamp: m.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            total_api_calls: m.total_api_calls,
            avg_api_latency_ms: m.avg_api_latency_ms(),
            http_success_rate: m.http_success_rate(),
            ws_reconnects: m.ws_reconnects,
            total_orders_submitted: m.total_orders_submitted,
            total_orders_filled: m.total_orders_filled,
            order_success_rate: m.order_success_rate(),
            sync_count: m.sync_count,
            avg_sync_latency_ms: m.avg_sync_latency_ms(),
            total_retries: m.total_retries,
            circuit_trips: m.circuit_trips,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_metrics_has_zero_values() {
        let m = GatewayMetrics::new();
        assert_eq!(m.total_api_calls, 0);
        assert_eq!(m.total_orders_submitted, 0);
        assert_eq!(m.min_api_latency_ms, u64::MAX);
    }

    #[test]
    fn record_api_call_updates_metrics() {
        let mut m = GatewayMetrics::new();
        m.record_api_call(50, true);
        m.record_api_call(100, true);
        m.record_api_call(30, false);

        assert_eq!(m.total_api_calls, 3);
        assert_eq!(m.http_successes, 2);
        assert_eq!(m.http_failures, 1);
        assert_eq!(m.min_api_latency_ms, 30);
        assert_eq!(m.max_api_latency_ms, 100);
        assert!((m.avg_api_latency_ms() - 60.0).abs() < 1.0);
        assert!((m.http_success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn record_orders() {
        let mut m = GatewayMetrics::new();
        m.record_order_submitted();
        m.record_order_submitted();
        m.record_order_filled();
        m.record_order_rejected();

        assert_eq!(m.total_orders_submitted, 2);
        assert_eq!(m.total_orders_filled, 1);
        assert_eq!(m.total_orders_rejected, 1);
        assert_eq!(m.order_success_rate(), 0.5);
    }

    #[test]
    fn record_ws_events() {
        let mut m = GatewayMetrics::new();
        m.record_ws_reconnect();
        m.record_ws_disconnect();
        m.record_ws_reconnect();

        assert_eq!(m.ws_reconnects, 2);
        assert_eq!(m.ws_disconnects, 1);
        assert!(m.ws_last_connect.is_some());
        assert!(m.ws_last_disconnect.is_some());
    }

    #[test]
    fn report_zh_contains_key_info() {
        let mut m = GatewayMetrics::new();
        m.record_api_call(42, true);
        m.record_order_submitted();
        m.record_order_filled();

        let report = m.report_zh();
        assert!(report.contains("Gateway 指标报告"));
        assert!(report.contains("API 延迟"));
        assert!(report.contains("成功率"));
        assert!(report.contains("订单成功率"));
    }

    #[test]
    fn summary_zh_is_concise() {
        let mut m = GatewayMetrics::new();
        m.record_api_call(42, true);
        let summary = m.summary_zh();
        assert!(summary.contains("API 延迟"));
        assert!(summary.contains("HTTP 成功率"));
    }

    #[test]
    fn uptime_is_positive() {
        let m = GatewayMetrics::new();
        assert!(m.uptime().as_secs() < 5); // just created
    }
}
