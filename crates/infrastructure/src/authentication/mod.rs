//! 认证框架：统一的认证接口和中间件。
//!
//! 从 `pm-auth::auth_provider`、`pm-gateway::auth`、`pm-gateway::middleware::auth` 提取并统一。
//!
//! # 支持的认证方式
//!
//! - Session（已实现）
//! - API Key（已实现）
//! - JWT（接口预留）
//! - Wallet Signature（接口预留）
//! - OAuth（接口预留）

pub mod session;
pub mod signer;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 认证健康状态
#[derive(Debug, Clone)]
pub struct AuthHealth {
    /// 提供者名称
    pub provider_name: String,
    /// 是否健康
    pub healthy: bool,
    /// 是否已认证
    pub authenticated: bool,
    /// 会话是否有效
    pub session_valid: bool,
    /// 令牌是否有效
    pub token_valid: bool,
    /// 凭证是否已加载
    pub credential_loaded: bool,
    /// 详情
    pub detail: String,
    /// 是否可用于真实交易
    pub is_live: bool,
}

impl AuthHealth {
    /// 创建健康的认证状态
    pub fn healthy(provider: &str, is_live: bool) -> Self {
        Self {
            provider_name: provider.to_string(),
            healthy: true,
            authenticated: true,
            session_valid: true,
            token_valid: true,
            credential_loaded: true,
            detail: "认证状态正常".to_string(),
            is_live,
        }
    }

    /// 中文摘要
    pub fn summary_zh(&self) -> String {
        format!(
            "认证状态: {} | 提供者: {} | 已认证: {} | 会话有效: {} | 真实交易: {}",
            if self.healthy { "健康" } else { "异常" },
            self.provider_name,
            if self.authenticated { "是" } else { "否" },
            if self.session_valid { "是" } else { "否" },
            if self.is_live { "是" } else { "否(DryRun)" },
        )
    }
}

/// 统一的认证提供者 trait
///
/// 所有交易所的认证实现均应实现此接口。
#[async_trait]
pub trait AuthenticationProvider: Send + Sync {
    /// 提供者名称
    fn name(&self) -> &str;

    /// 提供者类型（"polymarket", "kalshi", "binance", "okx" 等）
    fn provider_type(&self) -> &str;

    /// 是否启用真实交易
    fn live_enabled(&self) -> bool;

    /// 登录
    async fn login(&mut self) -> anyhow::Result<()>;

    /// 登出
    async fn logout(&mut self) -> anyhow::Result<()>;

    /// 验证当前认证状态
    async fn validate(&self) -> anyhow::Result<bool>;

    /// 刷新认证令牌
    async fn refresh(&mut self) -> anyhow::Result<()>;

    /// 健康检查
    async fn health(&self) -> anyhow::Result<AuthHealth>;
}

/// 认证中间件
///
/// 包装 AuthenticationProvider，提供自动注入认证头的能力。
pub struct AuthMiddleware<A: AuthenticationProvider> {
    provider: Arc<A>,
}

impl<A: AuthenticationProvider> AuthMiddleware<A> {
    /// 创建认证中间件
    pub fn new(provider: A) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }

    /// 注入认证头（用于 HTTP 请求）
    pub async fn inject_auth_headers(&self) -> anyhow::Result<HashMap<String, String>> {
        if !self.provider.live_enabled() {
            tracing::debug!("DryRun 模式，跳过认证头注入");
            return Ok(HashMap::new());
        }
        // 预留：从 provider 获取实际认证头
        Ok(HashMap::new())
    }

    /// 是否已认证
    pub async fn is_authenticated(&self) -> bool {
        self.provider.validate().await.unwrap_or(false)
    }

    /// 健康检查
    pub async fn health(&self) -> anyhow::Result<AuthHealth> {
        self.provider.health().await
    }
}

/// 模拟认证提供者（测试和 DryRun 用）
pub struct MockAuthProvider {
    name: String,
    provider_type: String,
    live: bool,
    logged_in: bool,
}

impl MockAuthProvider {
    /// 创建新的模拟认证提供者
    pub fn new(name: impl Into<String>, provider_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider_type: provider_type.into(),
            live: false,
            logged_in: false,
        }
    }
}

#[async_trait]
impl AuthenticationProvider for MockAuthProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn live_enabled(&self) -> bool {
        self.live
    }

    async fn login(&mut self) -> anyhow::Result<()> {
        tracing::info!("模拟登录: {}", self.name);
        self.logged_in = true;
        Ok(())
    }

    async fn logout(&mut self) -> anyhow::Result<()> {
        tracing::info!("模拟登出: {}", self.name);
        self.logged_in = false;
        Ok(())
    }

    async fn validate(&self) -> anyhow::Result<bool> {
        Ok(self.logged_in)
    }

    async fn refresh(&mut self) -> anyhow::Result<()> {
        tracing::debug!("模拟刷新令牌: {}", self.name);
        Ok(())
    }

    async fn health(&self) -> anyhow::Result<AuthHealth> {
        Ok(AuthHealth {
            provider_name: self.name.clone(),
            healthy: true,
            authenticated: self.logged_in,
            session_valid: self.logged_in,
            token_valid: true,
            credential_loaded: true,
            detail: "模拟认证提供者".to_string(),
            is_live: self.live,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_auth_lifecycle() {
        let mut auth = MockAuthProvider::new("test", "mock");
        assert!(!auth.live_enabled());

        auth.login().await.unwrap();
        assert!(auth.validate().await.unwrap());

        auth.logout().await.unwrap();
        assert!(!auth.validate().await.unwrap());

        let health = auth.health().await.unwrap();
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn mock_auth_refresh() {
        let mut auth = MockAuthProvider::new("test", "mock");
        auth.login().await.unwrap();
        auth.refresh().await.unwrap();
        assert!(auth.validate().await.unwrap());
    }

    #[tokio::test]
    async fn auth_middleware_health() {
        let auth = MockAuthProvider::new("test", "mock");
        let middleware = AuthMiddleware::new(auth);
        let health = middleware.health().await.unwrap();
        assert!(health.healthy);
        assert!(health.summary_zh().contains("DryRun"));
    }

    #[test]
    fn auth_health_summary() {
        let h = AuthHealth::healthy("polymarket", false);
        let summary = h.summary_zh();
        assert!(summary.contains("polymarket"));
        assert!(summary.contains("DryRun"));
    }
}
