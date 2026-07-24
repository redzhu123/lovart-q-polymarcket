//! Tracing 中间件（P2-03）。
//!
//! 为每个请求创建 tracing span，传播 request_id。
//! 捕获所有上下文信息便于调试。

use async_trait::async_trait;
use tracing;

use super::{Middleware, MiddlewareContext};
use crate::error::GatewayError;

// ============================================================================
// TracingMiddleware
// ============================================================================

/// Tracing 中间件。
///
/// 为每个请求创建 tracing span，包含：
/// - 模块名称
/// - 请求 ID
/// - HTTP 方法
/// - 请求路径
/// - 请求体大小
/// - 响应状态
/// - 耗时
pub struct TracingMiddleware;

impl TracingMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TracingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for TracingMiddleware {
    fn name(&self) -> &str {
        "TracingMiddleware"
    }

    async fn on_request(&self, ctx: &MiddlewareContext) {
        let span = tracing::info_span!(
            "gateway.request",
            gateway.module = %ctx.module,
            gateway.request_id = %ctx.request_id,
            gateway.method = %ctx.method,
            gateway.path = %ctx.path,
        );
        span.in_scope(|| {
            tracing::debug!(
                "Tracing span 已创建 — 模块={} 请求ID={}",
                ctx.module,
                ctx.request_id,
            );
        });
    }

    async fn on_response(&self, ctx: &MiddlewareContext) {
        let status = ctx.status.unwrap_or(0);
        tracing::debug!(
            gateway.module = %ctx.module,
            gateway.request_id = %ctx.request_id,
            gateway.status = %status,
            gateway.latency_ms = %ctx.latency_ms,
            "Tracing span 完成 — 状态={} 耗时={}ms",
            status,
            ctx.latency_ms,
        );
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        tracing::error!(
            gateway.module = %ctx.module,
            gateway.request_id = %ctx.request_id,
            gateway.error_code = %error.code(),
            gateway.error_kind = %error.kind_zh(),
            "Tracing span 错误 — {}: {}",
            error.kind_zh(),
            error.message_zh(),
        );
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_middleware_name() {
        let mw = TracingMiddleware::new();
        assert_eq!(mw.name(), "TracingMiddleware");
    }

    #[tokio::test]
    async fn tracing_hooks_dont_panic() {
        let mw = TracingMiddleware::new();
        let ctx = MiddlewareContext::new("req-001", "POST", "/order", "PolymarketGateway")
            .with_response(200, 42);

        mw.on_request(&ctx).await;
        mw.on_response(&ctx).await;

        let err = GatewayError::network("连接失败");
        mw.on_error(&err, &ctx).await;
    }
}