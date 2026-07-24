//! 重试中间件（P2-03）。
//!
//! 使用 RetryExecutor 进行自动重试。
//! 仅对可重试错误（NetworkError / TimeoutError / RateLimitError）重试。

use async_trait::async_trait;
use std::sync::Mutex;

use super::{Middleware, MiddlewareContext};
use crate::error::GatewayError;
use crate::retry::RetryExecutor;

// ============================================================================
// RetryMiddleware
// ============================================================================

/// 重试中间件。
///
/// 包装 RetryExecutor，在发生可重试错误时自动重试。
/// 记录重试次数和断路器状态。
pub struct RetryMiddleware {
    /// 重试执行器。
    executor: Mutex<RetryExecutor>,
}

impl RetryMiddleware {
    /// 创建新的重试中间件。
    pub fn new(executor: RetryExecutor) -> Self {
        tracing::info!(
            max_retries = %executor.breaker().stats_zh(),
            "重试中间件已创建"
        );
        Self {
            executor: Mutex::new(executor),
        }
    }

    /// 获取断路器引用。
    pub fn breaker_stats_zh(&self) -> String {
        let executor = self.executor.lock().unwrap();
        executor.breaker().stats_zh()
    }

    /// 是否允许请求（断路器状态）。
    pub fn allow_request(&self) -> bool {
        let mut executor = self.executor.lock().unwrap();
        executor.breaker_mut().allow_request()
    }

    /// 记录成功。
    pub fn record_success(&self) {
        let mut executor = self.executor.lock().unwrap();
        executor.breaker_mut().record_success();
    }

    /// 记录失败。
    pub fn record_failure(&self) {
        let mut executor = self.executor.lock().unwrap();
        executor.breaker_mut().record_failure();
    }

    /// 重试次数信息。
    pub fn retry_info_zh(&self) -> String {
        let executor = self.executor.lock().unwrap();
        executor.breaker().summary_zh()
    }
}

#[async_trait]
impl Middleware for RetryMiddleware {
    fn name(&self) -> &str {
        "RetryMiddleware"
    }

    async fn on_request(&self, ctx: &MiddlewareContext) {
        let info = self.retry_info_zh();
        tracing::debug!(
            request_id = %ctx.request_id,
            "断路器状态: {}",
            info,
        );
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        if error.is_retryable() {
            tracing::warn!(
                request_id = %ctx.request_id,
                error = %error.message_zh(),
                "可重试错误 — 将自动重试"
            );
        } else {
            tracing::debug!(
                request_id = %ctx.request_id,
                error = %error.message_zh(),
                "不可重试错误 — 不重试"
            );
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;

    #[test]
    fn retry_middleware_name() {
        let executor = RetryExecutor::from_config(&GatewayConfig::default());
        let mw = RetryMiddleware::new(executor);
        assert_eq!(mw.name(), "RetryMiddleware");
    }

    #[test]
    fn retry_middleware_allow_request() {
        let executor = RetryExecutor::from_config(&GatewayConfig::default());
        let mw = RetryMiddleware::new(executor);
        assert!(mw.allow_request());
    }

    #[test]
    fn retry_middleware_breaker_stats() {
        let executor = RetryExecutor::from_config(&GatewayConfig::default());
        let mw = RetryMiddleware::new(executor);
        let stats = mw.breaker_stats_zh();
        assert!(stats.contains("关闭"));
    }

    #[tokio::test]
    async fn retry_hooks_dont_panic() {
        let executor = RetryExecutor::from_config(&GatewayConfig::default());
        let mw = RetryMiddleware::new(executor);
        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test");

        mw.on_request(&ctx).await;

        let err = GatewayError::network("连接失败");
        mw.on_error(&err, &ctx).await;
    }
}