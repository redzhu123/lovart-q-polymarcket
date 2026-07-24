//! pm-trading：Trading Infrastructure（V1.07）。
//!
//! 统一管理所有交易基础设施。
//! Execution 只能通过 Trading 调用，禁止直接 HTTP / WS。
//!
//! 模块：
//! - [`provider`]：TradingProvider trait + MockTradingProvider
//! - [`credential`]：CredentialManager（凭据管理）
//! - [`session`]：SessionManager（会话管理）
//! - [`connection`]：ConnectionManager（连接管理）
//! - [`heartbeat`]：Heartbeat（心跳检查）
//! - [`recovery`]：RecoveryEngine（恢复引擎）
//! - [`state`]：TradingState（交易状态）
//! - [`capability`]：Capability（能力声明）
//! - [`config`]：TradingConfig（provider.toml 配置）
//! - [`diagnostics`]：诊断命令（provider/health/session/connection）
//! - [`mask`]：敏感信息脱敏工具
//!
//! Simulation Only -- MockTradingProvider 不产生真实交易。

pub mod capability;
pub mod config;
pub mod connection;
pub mod credential;
pub mod diagnostics;
pub mod heartbeat;
pub mod mask;
pub mod provider;
pub mod recovery;
pub mod session;
pub mod state;

// ---- 核心导出 ----
pub use capability::Capability;
pub use config::{DEFAULT_PROVIDER_TOML, TradingConfig, TradingEnvironment};
pub use connection::{ConnectionManager, ConnectionState, ConnectionStats, RetryPolicy};
pub use credential::{Credential, CredentialManager, CredentialTomlConfig};
pub use diagnostics::{
    diagnose_connection, diagnose_credential, diagnose_health, diagnose_provider, diagnose_session,
};
pub use heartbeat::{Heartbeat, HeartbeatResult};
pub use mask::{mask_address, mask_api_key, mask_passphrase, mask_private_key, mask_secret};
pub use provider::{
    AccountSummary, HealthStatus, MockTradingProvider, TradingMarket, TradingProvider,
};
pub use recovery::{RecoveryAction, RecoveryEngine, RecoveryEvent};
pub use session::{Session, SessionManager};
pub use state::TradingState;

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 完整集成测试：创建 Provider → 健康检查 → 心跳 → 诊断 → 断开。
    #[tokio::test]
    async fn full_trading_lifecycle() {
        // 1. 创建 Mock Provider
        let mut provider = MockTradingProvider::new();
        assert_eq!(provider.state(), TradingState::Ready);

        // 2. 心跳检查
        let mut heartbeat = Heartbeat::with_defaults();
        let result = heartbeat.beat(&provider).await;
        assert!(result.all_healthy);

        // 3. 健康诊断
        let health_output = diagnostics::diagnose_health(&provider).await;
        assert!(health_output.contains("系统健康"));

        // 4. Provider 诊断
        let provider_output = diagnostics::diagnose_provider(&provider).await;
        assert!(provider_output.contains("MockTradingProvider"));

        // 5. Session 诊断
        let session_output = diagnostics::diagnose_session(&provider.session_manager);
        assert!(session_output.contains("Session 诊断"));

        // 6. Connection 诊断
        let conn_output = diagnostics::diagnose_connection(&provider.connection_manager);
        assert!(conn_output.contains("Connection 诊断"));

        // 7. Credential 诊断
        let cred_output = diagnostics::diagnose_credential(&provider.credential_manager);
        assert!(cred_output.contains("Credential 诊断"));

        // 8. 断开
        provider.disconnect().await.unwrap();
        assert_eq!(provider.state(), TradingState::Stopped);

        // 9. 恢复
        let mut recovery = RecoveryEngine::new();
        let events = recovery.recover(&mut provider).await.unwrap();
        // 重连成功
        assert!(events.iter().all(|e| e.success));
        assert_eq!(provider.state(), TradingState::Ready);
    }

    /// 能力对比测试。
    #[test]
    fn capability_comparison() {
        let mock = Capability::mock();
        let pm = Capability::polymarket();

        assert!(!mock.can_real_trading);
        assert!(pm.can_real_trading);

        let diff = Capability::diff(&mock, &pm);
        assert!(diff.contains("⚠️ 差异"));
    }

    /// 脱敏验证。
    #[test]
    fn sensitive_data_masking() {
        let addr = "0x1234567890abcdef1234567890abcdef12345678";
        let masked = mask_address(addr);
        assert!(!masked.contains(addr));
        assert!(masked.starts_with("0x1234"));
        assert!(masked.ends_with("5678"));

        let secret = "my-super-secret-value";
        assert_eq!(mask_secret(secret), "[SECRET]");

        let api_key = "sk-1234567890abcdef";
        let masked_key = mask_api_key(api_key);
        assert!(!masked_key.contains(api_key));
    }

    /// 配置加载测试。
    #[test]
    fn trading_config_default_is_safe() {
        let cfg = TradingConfig::default();
        assert_eq!(cfg.environment, "paper");
        assert!(!cfg.allows_real_trading());
    }

    /// 状态转换测试。
    #[test]
    fn state_transitions_work() {
        let mut state = TradingState::default();
        assert_eq!(state, TradingState::Disconnected);

        state.transition_to(TradingState::Connecting);
        assert_eq!(state, TradingState::Connecting);

        state.transition_to(TradingState::Connected);
        assert_eq!(state, TradingState::Connected);

        state.transition_to(TradingState::Authenticated);
        assert_eq!(state, TradingState::Authenticated);

        state.transition_to(TradingState::Ready);
        assert_eq!(state, TradingState::Ready);
        assert!(state.is_operational());
    }

    /// 重试策略测试。
    #[test]
    fn retry_backoff_is_exponential() {
        let policy = RetryPolicy::default();
        let b0 = policy.backoff_ms(0);
        let b3 = policy.backoff_ms(3);
        assert!(b3 > b0 * 2); // 指数增长
    }
}
