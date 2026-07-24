//! Auth 中间件（P2-06 第六节）。
//!
//! 为 OMS / Execution 提供统一的认证中间件层。
//! Execution、OMS、PMS 不允许直接处理认证，必须通过本中间件。

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth_provider::AuthenticationProvider;

// ============================================================================
// AuthMiddleware
// ============================================================================

/// 认证中间件（P2-06 第六节）。
///
/// 包装 AuthenticationProvider，为上层业务（OMS/Execution）提供：
/// - 统一的认证头注入
/// - 请求认证状态检查
/// - 认证失败时的统一处理
///
/// Execution / OMS / PMS 禁止直接调用 AuthenticationProvider，
/// 必须通过 AuthMiddleware 访问认证功能。
pub struct AuthMiddleware {
    /// 底层认证提供者。
    provider: Arc<RwLock<Box<dyn AuthenticationProvider>>>,
    /// 中间件名称。
    name: String,
}

impl AuthMiddleware {
    /// 创建认证中间件。
    pub fn new(provider: Box<dyn AuthenticationProvider>) -> Self {
        let name = format!("AuthMiddleware({})", provider.name());
        Self {
            provider: Arc::new(RwLock::new(provider)),
            name,
        }
    }

    /// 中间件名称。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 注入认证头（用于 HTTP 请求）。
    ///
    /// 当前为 Simulation Only —— 返回空的认证头。
    /// 生产环境需根据 Provider 类型生成对应的 Authorization 头。
    pub async fn inject_auth_headers(&self) -> Result<HashMap<String, String>> {
        let provider = self.provider.read().await;
        if !provider.live_enabled() {
            tracing::debug!("AuthMiddleware: 模拟模式，不注入认证头");
            return Ok(HashMap::new());
        }

        // 检查认证状态
        if !provider.validate().await? {
            tracing::warn!("AuthMiddleware: 认证无效，无法注入认证头");
            anyhow::bail!("认证无效");
        }

        // 生成认证头
        let mut headers = HashMap::new();
        headers.insert(
            "X-Auth-Provider".to_string(),
            provider.provider_type().to_string(),
        );
        headers.insert("X-Auth-Mode".to_string(), "bearer".to_string());
        tracing::debug!("AuthMiddleware: 认证头注入完成（脱敏）");
        Ok(headers)
    }

    /// 验证请求是否已认证。
    pub async fn validate_request(&self) -> Result<bool> {
        let provider = self.provider.read().await;
        provider.validate().await
    }

    /// 检查认证是否健康。
    pub async fn check_health(&self) -> Result<crate::auth_provider::AuthHealth> {
        let provider = self.provider.read().await;
        provider.health().await
    }

    /// 获取 Provider 名称。
    pub async fn provider_name(&self) -> String {
        let provider = self.provider.read().await;
        provider.name().to_string()
    }

    /// 是否启用真实认证。
    pub async fn is_live(&self) -> bool {
        let provider = self.provider.read().await;
        provider.live_enabled()
    }

    /// 安全摘要（中文，脱敏）。
    pub async fn safe_summary(&self) -> String {
        let provider = self.provider.read().await;
        let health = provider
            .health()
            .await
            .unwrap_or_else(|_| crate::auth_provider::AuthHealth::new(provider.name()));
        format!(
            "AuthMiddleware: {} | 模式: {} | 健康: {}",
            provider.name(),
            if provider.live_enabled() {
                "⚠️ 真实"
            } else {
                "🔒 模拟"
            },
            health.summary_zh(),
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_provider::MockAuthProvider;

    #[tokio::test]
    async fn middleware_creation() {
        let provider = Box::new(MockAuthProvider::new());
        let mw = AuthMiddleware::new(provider);
        assert!(mw.name().contains("AuthMiddleware"));
        assert!(mw.name().contains("MockAuth"));
    }

    #[tokio::test]
    async fn middleware_inject_headers_simulation() {
        let provider = Box::new(MockAuthProvider::new());
        let mw = AuthMiddleware::new(provider);
        let headers = mw.inject_auth_headers().await.unwrap();
        // Simulation mode: no headers
        assert!(headers.is_empty());
    }

    #[tokio::test]
    async fn middleware_validate_request() {
        let mut provider = Box::new(MockAuthProvider::new());
        provider.login().await.unwrap();
        let mw = AuthMiddleware::new(provider);
        assert!(mw.validate_request().await.unwrap());
    }

    #[tokio::test]
    async fn middleware_check_health() {
        let provider = Box::new(MockAuthProvider::new());
        let mw = AuthMiddleware::new(provider);
        let health = mw.check_health().await.unwrap();
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn middleware_is_not_live() {
        let provider = Box::new(MockAuthProvider::new());
        let mw = AuthMiddleware::new(provider);
        assert!(!mw.is_live().await);
    }

    #[tokio::test]
    async fn middleware_safe_summary_chinese() {
        let provider = Box::new(MockAuthProvider::new());
        let mw = AuthMiddleware::new(provider);
        let summary = mw.safe_summary().await;
        assert!(summary.contains("AuthMiddleware"));
        assert!(summary.contains("模拟"));
    }
}
