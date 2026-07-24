//! AuthenticationProvider Trait（P2-06 第二节）。
//!
//! 统一认证接口，所有交易所认证必须实现此 trait。
//! 实现：PolymarketAuth / KalshiAuth（预留）/ DexWalletAuth（预留）。
//!
//! Execution、OMS、PMS 不允许直接处理认证。

use anyhow::Result;
use async_trait::async_trait;

use crate::credential::CredentialManager;
use crate::session::AuthSessionManager;

// ============================================================================
// AuthHealth — 认证健康状态
// ============================================================================

/// 认证健康状态（P2-06 第七节）。
#[derive(Debug, Clone)]
pub struct AuthHealth {
    /// Provider 名称。
    pub provider_name: String,
    /// 是否健康。
    pub healthy: bool,
    /// 是否已认证。
    pub authenticated: bool,
    /// Session 是否有效。
    pub session_valid: bool,
    /// Token 是否有效。
    pub token_valid: bool,
    /// 凭据是否已加载。
    pub credential_loaded: bool,
    /// 是否真实认证。
    pub is_live: bool,
    /// 详情描述。
    pub detail: String,
}

impl AuthHealth {
    /// 创建健康状态。
    pub fn new(provider_name: &str) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            healthy: false,
            authenticated: false,
            session_valid: false,
            token_valid: false,
            credential_loaded: false,
            is_live: false,
            detail: String::new(),
        }
    }

    /// 全部健康的 Mock 状态。
    pub fn mock_healthy() -> Self {
        Self {
            provider_name: "Mock".to_string(),
            healthy: true,
            authenticated: true,
            session_valid: true,
            token_valid: true,
            credential_loaded: true,
            is_live: false,
            detail: "模拟模式，全部健康".to_string(),
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "Provider: {} | 健康: {} | 已认证: {} | Session: {} | Token: {} | 凭据: {} | 真实: {}",
            self.provider_name,
            if self.healthy { "✅" } else { "❌" },
            if self.authenticated { "✅" } else { "❌" },
            if self.session_valid { "✅" } else { "❌" },
            if self.token_valid { "✅" } else { "❌" },
            if self.credential_loaded { "✅" } else { "❌" },
            if self.is_live {
                "⚠️ 是"
            } else {
                "🔒 否（模拟）"
            },
        )
    }
}

// ============================================================================
// AuthenticationProvider Trait
// ============================================================================

/// 统一认证接口（P2-06 第二节）。
///
/// 所有交易所认证必须实现此 trait。
/// Execution / OMS / PMS 禁止直接处理认证。
#[async_trait]
pub trait AuthenticationProvider: Send + Sync {
    /// Provider 名称。
    fn name(&self) -> &str;

    /// Provider 类型（如 "polymarket", "kalshi", "dex_wallet"）。
    fn provider_type(&self) -> &str;

    /// 是否启用真实认证。
    fn live_enabled(&self) -> bool;

    /// 登录认证。
    async fn login(&mut self) -> Result<()>;

    /// 登出。
    async fn logout(&mut self) -> Result<()>;

    /// 刷新认证（续期 Token）。
    async fn refresh(&mut self) -> Result<()>;

    /// 验证当前认证是否有效。
    async fn validate(&self) -> Result<bool>;

    /// 健康检查。
    async fn health(&self) -> Result<AuthHealth>;

    /// 加载凭据。
    fn load_credentials(&mut self) -> Result<()>;

    /// 保存凭据（当前仅日志，未来 KMS）。
    fn save_credentials(&self) -> Result<()>;
}

// ============================================================================
// MockAuthProvider — Mock 认证提供者
// ============================================================================

/// Mock 认证提供者（测试 / 演示用）。
pub struct MockAuthProvider {
    name: String,
    provider_type: String,
    live_enabled: bool,
    authenticated: bool,
    credential_manager: CredentialManager,
    session_manager: AuthSessionManager,
}

impl MockAuthProvider {
    /// 创建 Mock 认证提供者。
    pub fn new() -> Self {
        Self {
            name: "MockAuth".to_string(),
            provider_type: "mock".to_string(),
            live_enabled: false,
            authenticated: false,
            credential_manager: CredentialManager::new(),
            session_manager: AuthSessionManager::with_defaults(),
        }
    }

    /// 获取凭据管理器引用。
    pub fn credential_manager(&self) -> &CredentialManager {
        &self.credential_manager
    }

    /// 获取会话管理器引用。
    pub fn session_manager(&self) -> &AuthSessionManager {
        &self.session_manager
    }

    /// 获取会话管理器可变引用。
    pub fn session_manager_mut(&mut self) -> &mut AuthSessionManager {
        &mut self.session_manager
    }
}

impl Default for MockAuthProvider {
    fn default() -> Self {
        Self::new()
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
        self.live_enabled
    }

    async fn login(&mut self) -> Result<()> {
        tracing::info!("MockAuthProvider: 模拟登录");
        self.session_manager.create_session(
            "mock",
            Some("mock-access-token".to_string()),
            Some("mock-refresh-token".to_string()),
        );
        self.authenticated = true;
        Ok(())
    }

    async fn logout(&mut self) -> Result<()> {
        tracing::info!("MockAuthProvider: 模拟登出");
        self.session_manager.invalidate("mock");
        self.authenticated = false;
        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        tracing::info!("MockAuthProvider: 模拟刷新");
        self.session_manager.renew(
            "mock",
            "mock-refreshed-token".to_string(),
            Some("mock-refreshed-rt".to_string()),
        );
        Ok(())
    }

    async fn validate(&self) -> Result<bool> {
        Ok(self.authenticated)
    }

    async fn health(&self) -> Result<AuthHealth> {
        Ok(AuthHealth::mock_healthy())
    }

    fn load_credentials(&mut self) -> Result<()> {
        tracing::info!("MockAuthProvider: 模拟加载凭据");
        Ok(())
    }

    fn save_credentials(&self) -> Result<()> {
        tracing::info!("MockAuthProvider: 模拟保存凭据");
        Ok(())
    }
}

// ============================================================================
// PolymarketAuth — Polymarket 认证实现
// ============================================================================

/// Polymarket 认证提供者（P2-06 第二节）。
///
/// 实现 Polymarket API 认证流程。
/// Simulation Only —— 不连接真实认证服务。
pub struct PolymarketAuth {
    name: String,
    provider_type: String,
    live_enabled: bool,
    authenticated: bool,
    credential_manager: CredentialManager,
    session_manager: AuthSessionManager,
}

impl PolymarketAuth {
    /// 创建 Polymarket 认证提供者。
    pub fn new() -> Self {
        Self {
            name: "PolymarketAuth".to_string(),
            provider_type: "polymarket".to_string(),
            live_enabled: false,
            authenticated: false,
            credential_manager: CredentialManager::new(),
            session_manager: AuthSessionManager::with_defaults(),
        }
    }

    /// 从环境变量创建（自动加载 POLYMARKET_* 环境变量）。
    pub fn from_env() -> Result<Self> {
        let mut auth = Self::new();
        auth.load_credentials()?;
        Ok(auth)
    }

    /// 获取凭据管理器引用。
    pub fn credential_manager(&self) -> &CredentialManager {
        &self.credential_manager
    }

    /// 获取凭据管理器可变引用。
    pub fn credential_manager_mut(&mut self) -> &mut CredentialManager {
        &mut self.credential_manager
    }

    /// 获取会话管理器引用。
    pub fn session_manager(&self) -> &AuthSessionManager {
        &self.session_manager
    }

    /// 获取会话管理器可变引用。
    pub fn session_manager_mut(&mut self) -> &mut AuthSessionManager {
        &mut self.session_manager
    }

    /// 安全摘要（中文，脱敏）。
    pub fn safe_summary(&self) -> String {
        format!(
            "PolymarketAuth | 模式: {} | 已认证: {} | 凭据: {} | 会话: {}",
            if self.live_enabled {
                "⚠️ 真实"
            } else {
                "🔒 模拟"
            },
            if self.authenticated {
                "✅ 是"
            } else {
                "❌ 否"
            },
            if self.credential_manager.has_real_credentials() {
                "已加载"
            } else {
                "无"
            },
            if self.session_manager.is_empty() {
                "无"
            } else {
                "活跃"
            },
        )
    }
}

impl Default for PolymarketAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthenticationProvider for PolymarketAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn live_enabled(&self) -> bool {
        self.live_enabled
    }

    async fn login(&mut self) -> Result<()> {
        if self.live_enabled {
            anyhow::bail!("真实登录未实现（Simulation Only）");
        }

        tracing::info!("PolymarketAuth: 模拟登录");
        self.session_manager.create_session(
            "polymarket",
            Some("mock-polymarket-access-token".to_string()),
            Some("mock-polymarket-refresh-token".to_string()),
        );
        self.authenticated = true;
        tracing::info!("PolymarketAuth: 登录成功（模拟）");
        Ok(())
    }

    async fn logout(&mut self) -> Result<()> {
        tracing::info!("PolymarketAuth: 登出");
        self.session_manager.invalidate("polymarket");
        self.authenticated = false;
        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        if self.live_enabled {
            anyhow::bail!("真实刷新未实现（Simulation Only）");
        }

        tracing::info!("PolymarketAuth: 刷新认证");
        self.session_manager.renew(
            "polymarket",
            "mock-refreshed-polymarket-token".to_string(),
            Some("mock-refreshed-polymarket-rt".to_string()),
        );
        Ok(())
    }

    async fn validate(&self) -> Result<bool> {
        if !self.authenticated {
            return Ok(false);
        }
        if let Some(session) = self.session_manager.get("polymarket") {
            Ok(!session.is_expired())
        } else {
            Ok(false)
        }
    }

    async fn health(&self) -> Result<AuthHealth> {
        let session_valid = self
            .session_manager
            .get("polymarket")
            .map(|s| !s.is_expired())
            .unwrap_or(false);

        Ok(AuthHealth {
            provider_name: self.name.clone(),
            healthy: true,
            authenticated: self.authenticated,
            session_valid,
            token_valid: session_valid,
            credential_loaded: self.credential_manager.is_initialized(),
            is_live: self.live_enabled,
            detail: if self.live_enabled {
                "真实模式（未实现）".to_string()
            } else {
                "模拟模式，健康".to_string()
            },
        })
    }

    fn load_credentials(&mut self) -> Result<()> {
        self.credential_manager.load_from_env()?;
        tracing::info!(
            has_credentials = self.credential_manager.has_real_credentials(),
            provider_count = self.credential_manager.len(),
            "凭据加载完成"
        );
        Ok(())
    }

    fn save_credentials(&self) -> Result<()> {
        self.credential_manager
            .save_credentials("auth_credentials.json")?;
        Ok(())
    }
}

// ============================================================================
// KalshiAuth（预留）
// ============================================================================

/// Kalshi 认证提供者（接口预留，P2-06 第二节）。
pub struct KalshiAuth {
    name: String,
    provider_type: String,
}

impl KalshiAuth {
    pub fn new() -> Self {
        Self {
            name: "KalshiAuth".to_string(),
            provider_type: "kalshi".to_string(),
        }
    }
}

impl Default for KalshiAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthenticationProvider for KalshiAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn live_enabled(&self) -> bool {
        false
    }

    async fn login(&mut self) -> Result<()> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }

    async fn logout(&mut self) -> Result<()> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }

    async fn refresh(&mut self) -> Result<()> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }

    async fn validate(&self) -> Result<bool> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }

    async fn health(&self) -> Result<AuthHealth> {
        Ok(AuthHealth {
            provider_name: self.name.clone(),
            healthy: false,
            authenticated: false,
            session_valid: false,
            token_valid: false,
            credential_loaded: false,
            is_live: false,
            detail: "Kalshi 认证接口预留，未实现".to_string(),
        })
    }

    fn load_credentials(&mut self) -> Result<()> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }

    fn save_credentials(&self) -> Result<()> {
        anyhow::bail!("Kalshi 认证未实现（接口预留）")
    }
}

// ============================================================================
// DexWalletAuth（预留）
// ============================================================================

/// DEX 钱包认证提供者（接口预留，P2-06 第二节）。
pub struct DexWalletAuth {
    name: String,
    provider_type: String,
}

impl DexWalletAuth {
    pub fn new() -> Self {
        Self {
            name: "DexWalletAuth".to_string(),
            provider_type: "dex_wallet".to_string(),
        }
    }
}

impl Default for DexWalletAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthenticationProvider for DexWalletAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn live_enabled(&self) -> bool {
        false
    }

    async fn login(&mut self) -> Result<()> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }

    async fn logout(&mut self) -> Result<()> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }

    async fn refresh(&mut self) -> Result<()> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }

    async fn validate(&self) -> Result<bool> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }

    async fn health(&self) -> Result<AuthHealth> {
        Ok(AuthHealth {
            provider_name: self.name.clone(),
            healthy: false,
            authenticated: false,
            session_valid: false,
            token_valid: false,
            credential_loaded: false,
            is_live: false,
            detail: "DEX Wallet 认证接口预留，未实现".to_string(),
        })
    }

    fn load_credentials(&mut self) -> Result<()> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }

    fn save_credentials(&self) -> Result<()> {
        anyhow::bail!("DEX Wallet 认证未实现（接口预留）")
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_health_mock() {
        let h = AuthHealth::mock_healthy();
        assert!(h.healthy);
        assert!(h.authenticated);
        assert!(h.summary_zh().contains("✅"));
    }

    #[test]
    fn auth_health_new_is_not_healthy() {
        let h = AuthHealth::new("test");
        assert!(!h.healthy);
    }

    #[tokio::test]
    async fn mock_provider_login_logout() {
        let mut auth = MockAuthProvider::new();
        assert!(!auth.authenticated);

        auth.login().await.unwrap();
        assert!(auth.authenticated);
        assert!(auth.validate().await.unwrap());

        auth.logout().await.unwrap();
        assert!(!auth.authenticated);
    }

    #[tokio::test]
    async fn mock_provider_health() {
        let auth = MockAuthProvider::new();
        let health = auth.health().await.unwrap();
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn polymarket_auth_login_simulation() {
        let mut auth = PolymarketAuth::new();
        assert!(!auth.live_enabled());

        auth.login().await.unwrap();
        assert!(auth.validate().await.unwrap());

        let health = auth.health().await.unwrap();
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn polymarket_auth_refresh() {
        let mut auth = PolymarketAuth::new();
        auth.login().await.unwrap();
        auth.refresh().await.unwrap();
        assert!(auth.validate().await.unwrap());
    }

    #[tokio::test]
    async fn polymarket_auth_logout() {
        let mut auth = PolymarketAuth::new();
        auth.login().await.unwrap();
        auth.logout().await.unwrap();
        assert!(!auth.validate().await.unwrap());
    }

    #[test]
    fn polymarket_auth_safe_summary() {
        let auth = PolymarketAuth::new();
        let summary = auth.safe_summary();
        assert!(summary.contains("PolymarketAuth"));
        assert!(summary.contains("模拟"));
    }

    #[tokio::test]
    async fn kalshi_auth_all_return_error() {
        let mut auth = KalshiAuth::new();
        assert!(auth.login().await.is_err());
        assert!(auth.validate().await.is_err());
        // health should succeed (returns degraded status, not error)
        let h = auth.health().await.unwrap();
        assert!(!h.healthy);
    }

    #[tokio::test]
    async fn dex_wallet_auth_all_return_error() {
        let mut auth = DexWalletAuth::new();
        assert!(auth.login().await.is_err());
        // health should succeed
        let h = auth.health().await.unwrap();
        assert!(!h.healthy);
    }

    #[tokio::test]
    async fn auth_provider_trait_object_safe() {
        let auth: Box<dyn AuthenticationProvider> = Box::new(MockAuthProvider::new());
        assert_eq!(auth.name(), "MockAuth");
        assert!(!auth.live_enabled());
    }
}
