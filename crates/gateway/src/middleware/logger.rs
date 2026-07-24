//! 请求/响应日志中间件（P2-03）。
//!
//! 使用 tracing 记录所有请求和响应。
//! 全部中文日志。

use async_trait::async_trait;
use tracing;

use super::{Middleware, MiddlewareContext};
use crate::error::GatewayError;

// ============================================================================
// RequestLogger
// ============================================================================

/// 请求/响应日志中间件。
///
/// 在每个请求前和响应后通过 tracing 输出中文日志。
/// 包含：模块、请求 ID、方法、路径、状态、耗时、错误原因。
pub struct RequestLogger;

impl RequestLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RequestLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for RequestLogger {
    fn name(&self) -> &str {
        "RequestLogger"
    }

    async fn on_request(&self, ctx: &MiddlewareContext) {
        tracing::info!(
            module = %ctx.module,
            request_id = %ctx.request_id,
            method = %ctx.method,
            path = %ctx.path,
            body_size = %ctx.body_size,
            "[请求] {} {} | id={} | 模块={} | 请求体={}字节",
            ctx.method,
            ctx.path,
            ctx.request_id,
            ctx.module,
            ctx.body_size,
        );
    }

    async fn on_response(&self, ctx: &MiddlewareContext) {
        let status = ctx.status.unwrap_or(0);
        let icon = if (200..300).contains(&status) { "✅" } else { "❌" };

        tracing::info!(
            module = %ctx.module,
            request_id = %ctx.request_id,
            status = %status,
            latency_ms = %ctx.latency_ms,
            "[响应] {} {} {} | id={} | 耗时={}ms | 状态={}",
            icon,
            ctx.method,
            ctx.path,
            ctx.request_id,
            ctx.latency_ms,
            status,
        );
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        tracing::error!(
            module = %ctx.module,
            request_id = %ctx.request_id,
            method = %ctx.method,
            path = %ctx.path,
            error_code = %error.code(),
            error_kind = %error.kind_zh(),
            error_msg = %error.message_zh(),
            "[错误] {} {} | id={} | {}: {}",
            ctx.method,
            ctx.path,
            ctx.request_id,
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
    fn logger_name() {
        let logger = RequestLogger::new();
        assert_eq!(logger.name(), "RequestLogger");
    }

    #[tokio::test]
    async fn logger_hooks_dont_panic() {
        let logger = RequestLogger::new();
        let ctx = MiddlewareContext::new("req-001", "GET", "/time", "test-module");

        logger.on_request(&ctx).await;
        let resp_ctx = ctx.clone().with_response(200, 42);
        logger.on_response(&resp_ctx).await;

        let err = GatewayError::network("连接失败");
        logger.on_error(&err, &ctx).await;
    }
}