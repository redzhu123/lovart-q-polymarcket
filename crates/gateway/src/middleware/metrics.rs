//! Metrics 中间件（P2-03）。
//!
//! 在响应后/错误时自动更新 GatewayMetrics。

use async_trait::async_trait;
use std::sync::Mutex;

use super::{Middleware, MiddlewareContext};
use crate::error::GatewayError;
use crate::metrics::GatewayMetrics;

// ============================================================================
// MetricsMiddleware
// ============================================================================

/// Metrics 中间件。
///
/// 自动记录：API 请求次数、延迟、成功率、失败率。
pub struct MetricsMiddleware {
    /// 指标收集器。
    metrics: Mutex<GatewayMetrics>,
}

impl MetricsMiddleware {
    /// 创建新的 Metrics 中间件。
    pub fn new(metrics: GatewayMetrics) -> Self {
        Self {
            metrics: Mutex::new(metrics),
        }
    }

    /// 获取指标快照。
    pub fn snapshot(&self) -> GatewayMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// 中文指标报告。
    pub fn report_zh(&self) -> String {
        self.metrics.lock().unwrap().report_zh()
    }

    /// 中文指标摘要。
    pub fn summary_zh(&self) -> String {
        self.metrics.lock().unwrap().summary_zh()
    }

    /// 记录 API 调用。
    pub fn record_api_call(&self, latency_ms: u64, success: bool) {
        self.metrics
            .lock()
            .unwrap()
            .record_api_call(latency_ms, success);
    }

    /// 记录订单提交。
    pub fn record_order_submitted(&self) {
        self.metrics.lock().unwrap().record_order_submitted();
    }

    /// 记录订单成交。
    pub fn record_order_filled(&self) {
        self.metrics.lock().unwrap().record_order_filled();
    }

    /// 记录订单拒绝。
    pub fn record_order_rejected(&self) {
        self.metrics.lock().unwrap().record_order_rejected();
    }

    /// 记录 WebSocket 重连。
    pub fn record_ws_reconnect(&self) {
        self.metrics.lock().unwrap().record_ws_reconnect();
    }

    /// 记录 WebSocket 断连。
    pub fn record_ws_disconnect(&self) {
        self.metrics.lock().unwrap().record_ws_disconnect();
    }

    /// 记录断路器跳闸。
    pub fn record_circuit_trip(&self) {
        self.metrics.lock().unwrap().record_circuit_trip();
    }
}

#[async_trait]
impl Middleware for MetricsMiddleware {
    fn name(&self) -> &str {
        "MetricsMiddleware"
    }

    async fn on_response(&self, ctx: &MiddlewareContext) {
        let status = ctx.status.unwrap_or(0);
        let success = (200..300).contains(&status);
        self.metrics
            .lock()
            .unwrap()
            .record_api_call(ctx.latency_ms, success);
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        self.metrics
            .lock()
            .unwrap()
            .record_api_call(ctx.latency_ms, false);

        if error.is_retryable() {
            self.metrics.lock().unwrap().record_retry();
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
    fn metrics_middleware_name() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        assert_eq!(mw.name(), "MetricsMiddleware");
    }

    #[test]
    fn metrics_record_api_call() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        mw.record_api_call(42, true);
        mw.record_api_call(100, false);

        let snap = mw.snapshot();
        assert_eq!(snap.total_api_calls, 2);
        assert_eq!(snap.http_successes, 1);
        assert_eq!(snap.http_failures, 1);
    }

    #[test]
    fn metrics_record_orders() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        mw.record_order_submitted();
        mw.record_order_filled();

        let snap = mw.snapshot();
        assert_eq!(snap.total_orders_submitted, 1);
        assert_eq!(snap.total_orders_filled, 1);
    }

    #[test]
    fn metrics_report_zh() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        mw.record_api_call(42, true);
        let report = mw.report_zh();
        assert!(report.contains("Gateway 指标报告"));
    }

    #[tokio::test]
    async fn metrics_hooks_update_counts() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test").with_response(200, 42);

        mw.on_response(&ctx).await;

        let snap = mw.snapshot();
        assert_eq!(snap.total_api_calls, 1);
        assert_eq!(snap.http_successes, 1);
    }

    #[tokio::test]
    async fn metrics_error_hook() {
        let mw = MetricsMiddleware::new(GatewayMetrics::new());
        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test");

        let err = GatewayError::network("连接失败");
        mw.on_error(&err, &ctx).await;

        let snap = mw.snapshot();
        assert_eq!(snap.total_api_calls, 1);
        assert_eq!(snap.http_failures, 1);
    }
}
