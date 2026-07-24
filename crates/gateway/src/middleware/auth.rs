//! 认证中间件（P2-03）。
//!
//! 在请求前注入认证头。

use async_trait::async_trait;
use std::sync::Arc;

use super::{Middleware, MiddlewareContext};
use crate::auth::AuthProvider;
use crate::error::GatewayError;

// ============================================================================
// AuthMiddleware
// ============================================================================

/// 认证中间件。
///
/// 在请求发送前注入认证头（Bearer Token 等）。
/// 从 `AuthProvider` 读取认证信息。
pub struct AuthMiddleware {
    /// 认证提供者。
    provider: Arc<dyn AuthProvider>,
}

impl AuthMiddleware {
    /// 创建新的认证中间件。
    pub fn new(provider: Arc<dyn AuthProvider>) -> Self {
        tracing::info!(
            provider = %provider.name(),
            authenticated = %provider.is_authenticated(),
            "认证中间件已创建"
        );
        Self { provider }
    }

    /// 获取认证提供者。
    pub fn provider(&self) -> &dyn AuthProvider {
        self.provider.as_ref()
    }

    /// 获取认证头（供 Transport 层使用）。
    pub fn auth_headers(&self) -> Vec<(String, String)> {
        self.provider.headers().into_iter().collect()
    }
}

#[async_trait]
impl Middleware for AuthMiddleware {
    fn name(&self) -> &str {
        "AuthMiddleware"
    }

    async fn on_request(&self, ctx: &MiddlewareContext) {
        if self.provider.is_authenticated() {
            tracing::debug!(
                request_id = %ctx.request_id,
                provider = %self.provider.name(),
                "认证头已注入"
            );
        } else {
            tracing::warn!(
                request_id = %ctx.request_id,
                provider = %self.provider.name(),
                "未配置认证信息，请求可能失败"
            );
        }
    }

    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        if matches!(error, GatewayError::AuthenticationError { .. }) {
            tracing::error!(
                request_id = %ctx.request_id,
                provider = %self.provider.name(),
                "认证失败 — 请检查 API 密钥配置"
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
    use crate::auth::NoopAuth;

    #[test]
    fn auth_middleware_name() {
        let mw = AuthMiddleware::new(Arc::new(NoopAuth));
        assert_eq!(mw.name(), "AuthMiddleware");
    }

    #[test]
    fn auth_headers_from_noop_is_empty() {
        let mw = AuthMiddleware::new(Arc::new(NoopAuth));
        assert!(mw.auth_headers().is_empty());
    }

    #[tokio::test]
    async fn auth_middleware_hooks_dont_panic() {
        let mw = AuthMiddleware::new(Arc::new(NoopAuth));
        let ctx = MiddlewareContext::new("req-1", "POST", "/order", "test");

        mw.on_request(&ctx).await;

        let err = GatewayError::authentication("密钥无效");
        mw.on_error(&err, &ctx).await;
    }
}
