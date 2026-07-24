//! pm-auth：Authentication Infrastructure（P2-06）。
//!
//! 企业级认证基础设施，作为 Gateway 与 Exchange 之间的认证层。
//! Execution / OMS / PMS 禁止直接处理认证。
//!
//! # 架构
//!
//! ```text
//! OMS / Execution / PMS
//!       │
//!       ▼
//! ┌──────────────────────────┐
//! │  AuthMiddleware          │  ← 统一认证入口
//! └─────────┬────────────────┘
//!           │
//! ┌─────────▼────────────────┐
//! │  AuthenticationProvider  │  ← PolymarketAuth / KalshiAuth / DexWalletAuth
//! └─────────┬────────────────┘
//!           │
//! ┌─────────▼────────────────┐
//! │  CredentialManager      │  ← 凭证管理
//! │  SessionManager         │  ← 会话管理
//! │  Signer                  │  ← 签名器（EIP-712 / EVM / Ed25519）
//! └──────────────────────────┘
//!           │
//!           ▼
//! Gateway → Exchange
//! ```
//!
//! # 模块
//!
//! - [`credential`]：扩展凭证管理（版本/来源/脱敏/KMS 预留）
//! - [`session`]：认证会话管理（创建/过期/续期）
//! - [`signer`]：统一签名接口（Polymarket/EVM/Ed25519）
//! - [`auth_provider`]：AuthenticationProvider trait + 实现
//! - [`middleware`]：AuthMiddleware 统一认证入口
//! - [`refresh`]：Token 续期调度器
//! - [`diagnostics`]：健康诊断命令
//!
//! # 业务约束
//!
//! - 禁止真实交易 / 真实私钥签名 / 自动签名发送订单。
//! - Execution / OMS / PMS 禁止直接处理认证。
//! - 所有日志使用 tracing，中文输出。
//! - 所有敏感信息自动脱敏。
//!
//! Simulation Only -- 不连接真实认证服务 / 不签名 / 不暴露私钥。

pub mod auth_provider;
pub mod credential;
pub mod diagnostics;
pub mod middleware;
pub mod refresh;
pub mod session;
pub mod signer;

// ---- 核心重导出 ----
pub use auth_provider::{
    AuthHealth, AuthenticationProvider, DexWalletAuth, KalshiAuth, MockAuthProvider, PolymarketAuth,
};
pub use credential::{
    CredentialManager, CredentialSource, CredentialVersion, ExtendedCredential, SensitiveString,
};
pub use diagnostics::{
    diagnose_auth_credential, diagnose_auth_health, diagnose_auth_session, diagnose_auth_token,
};
pub use middleware::AuthMiddleware;
pub use refresh::{RefreshOutcome, TokenRefreshScheduler};
pub use session::AuthSessionManager;
pub use signer::{
    NoopSigner, SignRequest, SignResponse, Signer, SignerHealth, ed25519::Ed25519Signer,
    evm::EvmSigner, polymarket::PolymarketSigner,
};

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::auth_provider::{
        AuthHealth, AuthenticationProvider, DexWalletAuth, KalshiAuth, MockAuthProvider,
        PolymarketAuth,
    };
    pub use crate::create_default_auth;
    pub use crate::create_mock_auth;
    pub use crate::credential::{
        CredentialManager, CredentialSource, CredentialVersion, ExtendedCredential, SensitiveString,
    };
    pub use crate::diagnostics::{
        diagnose_auth_credential, diagnose_auth_health, diagnose_auth_session, diagnose_auth_token,
    };
    pub use crate::middleware::AuthMiddleware;
    pub use crate::refresh::{RefreshOutcome, TokenRefreshScheduler};
    pub use crate::session::AuthSessionManager;
    pub use crate::signer::{
        NoopSigner, SignRequest, SignResponse, Signer, SignerHealth, ed25519::Ed25519Signer,
        evm::EvmSigner, polymarket::PolymarketSigner,
    };
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建默认 Auth（PolymarketAuth，模拟模式）。
pub fn create_default_auth() -> anyhow::Result<PolymarketAuth> {
    Ok(PolymarketAuth::new())
}

/// 创建 Mock Auth（测试 / 演示用）。
pub fn create_mock_auth() -> anyhow::Result<MockAuthProvider> {
    Ok(MockAuthProvider::new())
}

// ============================================================================
// 中文 tracing 初始化
// ============================================================================

/// 初始化 Auth 中文 tracing。
pub fn init_auth_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("PM_AUTH_LOG").unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use auth_provider::AuthHealth;

    #[test]
    fn prelude_exports_compile() {
        // 验证 prelude 全部导出可用
        let _health = AuthHealth::mock_healthy();
        let _source = CredentialSource::Environment;
        let _version = CredentialVersion::v1();
        let _sensitive = SensitiveString::new("test");
        let _ = PolymarketAuth::new();
        let _ = KalshiAuth::new();
        let _ = DexWalletAuth::new();
        let _ = MockAuthProvider::new();
        let _ = NoopSigner::new();
        let _ = EvmSigner::new(137);
        let _ = Ed25519Signer::new();
        let _ = PolymarketSigner::new();
        let _ = AuthSessionManager::with_defaults();
        let _ = CredentialManager::new();
        let _ = TokenRefreshScheduler::with_defaults();
    }

    #[test]
    fn default_factory_works() {
        let auth = create_default_auth().unwrap();
        assert_eq!(auth.name(), "PolymarketAuth");
        assert!(!auth.live_enabled());
    }

    #[test]
    fn mock_factory_works() {
        let auth = create_mock_auth().unwrap();
        assert_eq!(auth.name(), "MockAuth");
        assert!(!auth.live_enabled());
    }

    #[tokio::test]
    async fn full_auth_lifecycle() {
        let mut auth = create_default_auth().unwrap();

        // 1. 初始状态：未认证
        assert!(!auth.validate().await.unwrap());

        // 2. 登录
        auth.login().await.unwrap();
        assert!(auth.validate().await.unwrap());

        // 3. 健康检查
        let health = auth.health().await.unwrap();
        assert!(health.healthy);
        assert!(health.authenticated);

        // 4. 刷新
        auth.refresh().await.unwrap();
        assert!(auth.validate().await.unwrap());

        // 5. 登出
        auth.logout().await.unwrap();
        assert!(!auth.validate().await.unwrap());
    }

    #[tokio::test]
    async fn middleware_integration() {
        let auth = create_default_auth().unwrap();
        let mw = AuthMiddleware::new(Box::new(auth));

        assert!(!mw.is_live().await);
        let health = mw.check_health().await.unwrap();
        assert!(health.healthy);
    }

    #[test]
    fn credential_manager_integration() {
        let mut mgr = CredentialManager::new();
        assert!(mgr.is_empty());
        assert!(!mgr.has_real_credentials());

        // 注册一个凭据
        let cred = ExtendedCredential::empty();
        mgr.register("test-provider", cred);
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.provider_names(), vec!["test-provider"]);
    }

    #[test]
    fn session_manager_integration() {
        let mut mgr = AuthSessionManager::with_defaults();
        assert!(mgr.is_empty());

        mgr.create_session(
            "polymarket",
            Some("token-abc".into()),
            Some("rt-xyz".into()),
        );
        assert_eq!(mgr.len(), 1);
        assert!(mgr.current().is_some());
    }

    #[test]
    fn signer_integration() {
        let signer = PolymarketSigner::new();
        assert_eq!(signer.algorithm(), "secp256k1");
        assert!(!signer.can_sign_real());

        let req = SignRequest::new(b"test-payload".to_vec(), "eip712");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.sign_type, "eip712");
    }

    #[tokio::test]
    async fn kalshi_reserved_interface() {
        let mut auth = KalshiAuth::new();
        assert!(auth.login().await.is_err());
        assert!(auth.logout().await.is_err());
        assert!(auth.validate().await.is_err());
        // health should work (returns degraded)
        let h = auth.health().await.unwrap();
        assert!(!h.healthy);
    }

    #[tokio::test]
    async fn dex_wallet_reserved_interface() {
        let mut auth = DexWalletAuth::new();
        assert!(auth.login().await.is_err());
        // health should work
        let h = auth.health().await.unwrap();
        assert!(!h.healthy);
    }

    #[test]
    fn sensitive_string_debug_masks() {
        let s = SensitiveString::new("super-secret-api-key-12345");
        let debug_str = format!("{:?}", s);
        // Must not contain the raw secret
        assert!(!debug_str.contains("super-secret-api-key-12345"));
        // Must indicate it's a SensitiveString
        assert!(debug_str.contains("SensitiveString"));
    }

    #[test]
    fn refresh_scheduler_integration() {
        let scheduler = TokenRefreshScheduler::with_defaults();
        let stats = scheduler.stats_zh();
        assert!(stats.contains("续期成功"));
        assert!(stats.contains("续期失败"));
    }

    #[tokio::test]
    async fn all_diagnostics_produce_chinese_output() {
        let auth = create_mock_auth().unwrap();

        let health_out = diagnose_auth_health(&auth).await;
        assert!(health_out.contains("健康"));
        assert!(health_out.contains("诊断"));

        let session_out = diagnose_auth_session(&auth).await;
        assert!(session_out.contains("会话"));

        let cred_mgr = CredentialManager::new();
        let cred_out = diagnose_auth_credential(&cred_mgr);
        assert!(cred_out.contains("凭据"));

        let sess_mgr = AuthSessionManager::with_defaults();
        let token_out = diagnose_auth_token(&sess_mgr);
        assert!(token_out.contains("Token"));
    }
}
