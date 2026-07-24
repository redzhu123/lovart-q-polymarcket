//! 速率限制中间件（P2-03）。
//!
//! 在请求前检查速率限制，自动等待或拒绝。

use async_trait::async_trait;
use std::sync::Arc;

use super::{Middleware, MiddlewareContext};
use crate::error::GatewayError;
use crate::ratelimit::RateLimiter;

// ============================================================================
// RateLimitMiddleware
// ============================================================================

/// 速率限制中间件。
///
/// 在请求前获取 Token，若不足则等待或拒绝。
pub struct RateLimitMiddleware {
    /// 速率限制器。
    limiter: Arc<RateLimiter>,
}

impl RateLimitMiddleware {
    /// 创建新的速率限制中间件。
    pub fn new(limiter: Arc<RateLimiter>) -> Self {
        Self { limiter }
    }

    /// 获取速率限制器引用。
    pub fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    /// 获取一个 token，返回等待时间。
    pub fn acquire(&self) -> u64 {
        self.limiter.acquire()
    }

    /// 剩余比例。
    pub fn remaining(&self) -> f64 {
        self.limiter.remaining()
    }

    /// 速率限制统计（中文）。
    pub fn stats_zh(&self) -> String {
        self.limiter.stats().summary_zh()
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    fn name(&self) -> &str {
        "RateLimitMiddleware"
    }

    async fn on_request(&self, ctx: &MiddlewareContext) {
        let remaining = self.limiter.remaining();
        if remaining < 0.1 {
            tracing::warn!(
                request_id = %ctx.request_id,
                remaining_pct = %(remaining * 100.0),
                "速率限制即将耗尽 — 剩余 {:.0}%",
                remaining * 100.0,
            );
        }
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        if matches!(error, GatewayError::RateLimitError { .. }) {
            tracing::error!(
                request_id = %ctx.request_id,
                stats = %self.stats_zh(),
                "触发速率限制错误"
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

    #[test]
    fn ratelimit_middleware_name() {
        let mw = RateLimitMiddleware::new(Arc::new(RateLimiter::new(10, 300)));
        assert_eq!(mw.name(), "RateLimitMiddleware");
    }

    #[test]
    fn ratelimit_acquire() {
        let mw = RateLimitMiddleware::new(Arc::new(RateLimiter::new(100, 1000)));
        assert_eq!(mw.acquire(), 0);
    }

    #[test]
    fn ratelimit_remaining() {
        let mw = RateLimitMiddleware::new(Arc::new(RateLimiter::new(100, 1000)));
        assert!((mw.remaining() - 1.0).abs() < 0.01);
    }

    #[test]
    fn ratelimit_stats() {
        let mw = RateLimitMiddleware::new(Arc::new(RateLimiter::new(10, 300)));
        let stats = mw.stats_zh();
        assert!(stats.contains("每秒限制"));
        assert!(stats.contains("每分钟限制"));
    }

    #[tokio::test]
    async fn ratelimit_hooks_dont_panic() {
        let mw = RateLimitMiddleware::new(Arc::new(RateLimiter::new(10, 300)));
        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test");

        mw.on_request(&ctx).await;

        let err = GatewayError::rate_limit("触发限流", 5000);
        mw.on_error(&err, &ctx).await;
    }
}