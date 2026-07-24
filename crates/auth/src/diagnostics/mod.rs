//! Auth Diagnostics（P2-06 第七节）。
//!
//! 提供 CLI 诊断命令：
//! - `cargo run -- auth health`：认证健康检查
//! - `cargo run -- auth session`：会话诊断
//! - `cargo run -- auth credential`：凭据诊断
//!
//! 全部中文输出，敏感信息自动脱敏。

use crate::auth_provider::AuthenticationProvider;
use crate::credential::CredentialManager;
use crate::session::AuthSessionManager;

// ============================================================================
// Auth Health Diagnostics
// ============================================================================

/// `cargo run -- auth health`：认证健康检查（P2-06 第七节）。
///
/// 检查：
/// - Credential：凭据是否已加载
/// - Session：会话是否有效
/// - Token：Token 是否有效
/// - Authentication：认证状态
pub async fn diagnose_auth_health(provider: &dyn AuthenticationProvider) -> String {
    let health = provider.health().await.unwrap_or_else(|e| {
        let mut h = crate::auth_provider::AuthHealth::new(provider.name());
        h.detail = format!("健康检查失败: {}", e);
        h
    });

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【认证健康诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    // 基本信息
    lines.push(format!("  Provider      : {}", provider.name()));
    lines.push(format!("  类型          : {}", provider.provider_type()));
    lines.push(format!(
        "  模式          : {}",
        if provider.live_enabled() {
            "⚠️ 真实认证"
        } else {
            "🔒 模拟认证"
        }
    ));
    lines.push(String::new());

    // 健康状况
    lines.push("── 健康状态 ──".to_string());
    lines.push(format!(
        "  整体健康      : {}",
        if health.healthy {
            "✅ 健康"
        } else {
            "❌ 异常"
        }
    ));
    lines.push(format!(
        "  已认证        : {}",
        if health.authenticated {
            "✅ 是"
        } else {
            "❌ 否"
        }
    ));
    lines.push(format!(
        "  Session       : {}",
        if health.session_valid {
            "✅ 有效"
        } else {
            "❌ 无效"
        }
    ));
    lines.push(format!(
        "  Token         : {}",
        if health.token_valid {
            "✅ 有效"
        } else {
            "❌ 无效"
        }
    ));
    lines.push(format!(
        "  凭据已加载    : {}",
        if health.credential_loaded {
            "✅ 是"
        } else {
            "❌ 否"
        }
    ));
    if !health.detail.is_empty() {
        lines.push(format!("  详情          : {}", health.detail));
    }
    lines.push(String::new());

    // 建议
    lines.push("── 建议 ──".to_string());
    if health.healthy {
        lines.push("  无需操作，认证系统运行正常。".to_string());
    } else {
        if !health.authenticated {
            lines.push("  ⚠️ 未认证：运行 login 获取认证。".to_string());
        }
        if !health.session_valid {
            lines.push("  ⚠️ Session 无效：需要重新登录或续期。".to_string());
        }
        if !health.credential_loaded {
            lines.push(
                "  ⚠️ 凭据未加载：检查环境变量（POLYMARKET_API_KEY 等）或 .env 文件。".to_string(),
            );
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Auth Session Diagnostics
// ============================================================================

/// `cargo run -- auth session`：会话诊断（P2-06 第七节）。
pub async fn diagnose_auth_session(provider: &dyn AuthenticationProvider) -> String {
    let health = provider.health().await;

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【认证会话诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    match health {
        Ok(h) => {
            lines.push(format!("  Provider      : {}", h.provider_name));
            lines.push(format!(
                "  Session       : {}",
                if h.session_valid {
                    "✅ 有效"
                } else {
                    "❌ 无效"
                }
            ));
            lines.push(format!(
                "  Token         : {}",
                if h.token_valid {
                    "✅ 有效"
                } else {
                    "❌ 无效"
                }
            ));
            lines.push(format!(
                "  认证状态      : {}",
                if h.authenticated {
                    "✅ 已认证"
                } else {
                    "❌ 未认证"
                }
            ));

            if h.is_live {
                lines.push("  模式          : ⚠️ 真实认证".to_string());
            } else {
                lines.push("  模式          : 🔒 模拟认证".to_string());
            }
        }
        Err(e) => {
            lines.push(format!("  ❌ 诊断失败: {}", e));
        }
    }

    lines.push(String::new());
    lines.push("── 建议 ──".to_string());
    lines.push("  如果 Session 无效，请执行 login 获取新会话。".to_string());
    lines.push("  如果 Token 即将过期，请执行 refresh 续期。".to_string());

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Auth Credential Diagnostics
// ============================================================================

/// `cargo run -- auth credential`：凭据诊断（P2-06 第七节）。
///
/// 显示凭据状态（全部脱敏）。
pub fn diagnose_auth_credential(cred_mgr: &CredentialManager) -> String {
    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【认证凭据诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.push(format!(
        "  默认 Provider : {}",
        cred_mgr.default_provider_name()
    ));
    lines.push(format!(
        "  已注册 Provider: {} 个",
        cred_mgr.provider_names().len()
    ));
    lines.push(format!(
        "  已初始化      : {}",
        if cred_mgr.is_initialized() {
            "✅ 是"
        } else {
            "❌ 否"
        }
    ));
    lines.push(format!(
        "  真实凭据      : {}",
        if cred_mgr.has_real_credentials() {
            "⚠️ 是（已脱敏显示）"
        } else {
            "否（Mock 模式）"
        }
    ));
    lines.push(String::new());

    // 脱敏后的凭据详情
    lines.push("── 凭据详情（脱敏）──".to_string());
    lines.push(cred_mgr.safe_summary());
    lines.push(String::new());

    if !cred_mgr.has_real_credentials() {
        lines.push("  当前无真实凭据，使用 Mock 模式。".to_string());
        lines.push("  如需真实交易，请配置环境变量：".to_string());
        lines.push("    POLYMARKET_API_KEY=<your-key>".to_string());
        lines.push("    POLYMARKET_API_SECRET=<your-secret>".to_string());
        lines.push("    POLYMARKET_WALLET_ADDRESS=<your-address>".to_string());
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Auth Token Diagnostics
// ============================================================================

/// `cargo run -- auth token`：Token 诊断（P2-06 第七节）。
pub fn diagnose_auth_token(session_mgr: &AuthSessionManager) -> String {
    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【认证 Token 诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    if let Some(session) = session_mgr.current() {
        lines.push(format!("  Session ID    : {}", session.session_id));
        lines.push(format!(
            "  已认证        : {}",
            if session.authenticated {
                "✅ 是"
            } else {
                "❌ 否"
            }
        ));
        lines.push(format!(
            "  已过期        : {}",
            if session.is_expired() {
                "⚠️ 是"
            } else {
                "✅ 否"
            }
        ));
        lines.push(format!("  剩余时间      : {}秒", session.remaining_secs()));
        lines.push(format!(
            "  即将过期      : {}",
            if session.expires_soon(300) {
                "⚠️ 是（5分钟内）"
            } else {
                "否"
            }
        ));
        lines.push(format!(
            "  需要续期      : {}",
            if session.needs_renewal() {
                "⚠️ 是"
            } else {
                "否"
            }
        ));

        if let Some(ref token) = session.access_token {
            let masked = pm_trading::mask::mask_api_key(token);
            lines.push(format!("  Token（脱敏） : {}", masked));
        }
        if session.refresh_token.is_some() {
            lines.push("  Refresh Token : [REFRESH_TOKEN]".to_string());
        }
    } else {
        lines.push("  无活跃 Token（未认证或已过期）".to_string());
        lines.push(String::new());
        lines.push("  建议：执行 login 获取新 Token。".to_string());
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_provider::MockAuthProvider;

    #[tokio::test]
    async fn diagnose_auth_health_output() {
        let auth = MockAuthProvider::new();
        let output = diagnose_auth_health(&auth).await;
        assert!(output.contains("认证健康诊断"));
        assert!(output.contains("MockAuth"));
        assert!(output.contains("模拟"));
        assert!(output.contains("健康"));
    }

    #[tokio::test]
    async fn diagnose_auth_session_output() {
        let auth = MockAuthProvider::new();
        let output = diagnose_auth_session(&auth).await;
        assert!(output.contains("认证会话诊断"));
    }

    #[test]
    fn diagnose_auth_credential_output() {
        let mgr = CredentialManager::new();
        let output = diagnose_auth_credential(&mgr);
        assert!(output.contains("认证凭据诊断"));
        assert!(output.contains("Mock"));
    }

    #[test]
    fn diagnose_auth_token_empty() {
        let mgr = AuthSessionManager::with_defaults();
        let output = diagnose_auth_token(&mgr);
        assert!(output.contains("认证 Token 诊断"));
        assert!(output.contains("无活跃"));
    }

    #[test]
    fn diagnose_auth_token_with_session() {
        let mut mgr = AuthSessionManager::with_defaults();
        mgr.create_session("test", Some("my-token-value".into()), None);
        let output = diagnose_auth_token(&mgr);
        assert!(output.contains("认证 Token 诊断"));
        // Token value should be masked
        assert!(!output.contains("my-token-value"));
    }

    #[test]
    fn all_diagnostics_chinese() {
        // Verify Chinese characters are present in all diagnostics
        let mgr = CredentialManager::new();
        let session_mgr = AuthSessionManager::with_defaults();

        let cred_output = diagnose_auth_credential(&mgr);
        assert!(cred_output.contains("凭据"));
        assert!(cred_output.contains("诊断"));

        let token_output = diagnose_auth_token(&session_mgr);
        assert!(token_output.contains("Token"));
        assert!(token_output.contains("无活跃"));
    }
}
