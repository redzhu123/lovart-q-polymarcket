//! Trading Diagnostics（V1.07 第十一节）。
//!
//! 提供 CLI 诊断命令：
//! - `cargo run -- provider`：查看 Provider 信息
//! - `cargo run -- health`：查看 Health 状态
//! - `cargo run -- session`：查看 Session 状态
//! - `cargo run -- connection`：查看 Connection 状态
//!
//! 全部输出中文。

use crate::connection::ConnectionManager;
use crate::credential::CredentialManager;
use crate::provider::TradingProvider;
use crate::session::SessionManager;

// ============================================================================
// Provider Diagnostics
// ============================================================================

/// 打印 Provider 诊断信息（V1.07 第十一节 `cargo run -- provider`）。
pub async fn diagnose_provider(provider: &dyn TradingProvider) -> String {
    let cap = provider.capability();
    let health = provider.health().await;

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("  Provider 诊断".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());
    lines.push(format!("  Provider 名称 : {}", provider.name()));
    lines.push(format!("  状态          : {}", provider.state().as_zh()));
    lines.push(format!(
        "  Gateway       : {}",
        provider.gateway_name()
    ));
    lines.push(String::new());
    lines.push(cap.render_table());
    lines.push(String::new());
    lines.push("── 健康状态 ──".to_string());
    lines.push(format!("  整体健康      : {}", if health.healthy { "✅ 是" } else { "❌ 否" }));
    lines.push(format!(
        "  HTTP          : {}",
        if health.http_ok { "✅ 正常" } else { "❌ 异常" }
    ));
    lines.push(format!(
        "  WebSocket     : {}",
        if health.ws_ok { "✅ 正常" } else { "❌ 异常" }
    ));
    lines.push(format!(
        "  Session       : {}",
        if health.session_valid {
            "✅ 有效"
        } else {
            "❌ 无效"
        }
    ));
    lines.push(format!("  延迟          : {}ms", health.latency_ms));
    if !health.detail.is_empty() {
        lines.push(format!("  详情          : {}", health.detail));
    }
    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Health Diagnostics
// ============================================================================

/// 打印 Health 诊断信息（V1.07 第十一节 `cargo run -- health`）。
pub async fn diagnose_health(provider: &dyn TradingProvider) -> String {
    let health = provider.health().await;
    let cap = provider.capability();

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("  Health 诊断".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    let overall = if health.healthy {
        "✅ 系统健康"
    } else {
        "❌ 系统异常"
    };
    lines.push(format!("  整体状态      : {}", overall));
    lines.push(format!("  Provider      : {}", provider.name()));
    lines.push(format!("  当前状态      : {}", provider.state().as_zh()));
    lines.push(format!(
        "  真实交易      : {}",
        if cap.can_real_trading {
            "⚠️ 是"
        } else {
            "✅ 否（Dry Run）"
        }
    ));
    lines.push(String::new());

    lines.push("── 连接检查 ──".to_string());
    lines.push(format!(
        "  HTTP 连接     : {}",
        if health.http_ok { "✅" } else { "❌" }
    ));
    lines.push(format!(
        "  WS 连接       : {}",
        if health.ws_ok { "✅" } else { "❌" }
    ));
    lines.push(format!(
        "  Session       : {}",
        if health.session_valid { "✅" } else { "❌" }
    ));
    lines.push(format!("  延迟          : {}ms", health.latency_ms));
    lines.push(String::new());

    lines.push("── 建议 ──".to_string());
    if health.healthy {
        lines.push("  无需操作，系统运行正常。".to_string());
    } else {
        if !health.http_ok {
            lines.push("  ⚠️ HTTP 连接异常，检查网络或 Provider 状态。".to_string());
        }
        if !health.ws_ok {
            lines.push("  ⚠️ WebSocket 连接异常，检查 WS 端点。".to_string());
        }
        if !health.session_valid {
            lines.push("  ⚠️ Session 无效，需要重新登录。".to_string());
        }
    }
    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Session Diagnostics
// ============================================================================

/// 打印 Session 诊断信息（V1.07 第十一节 `cargo run -- session`）。
pub fn diagnose_session(session_mgr: &SessionManager) -> String {
    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("  Session 诊断".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    match session_mgr.current() {
        Some(session) => {
            lines.push(format!("  Session ID    : {}", session.session_id));
            lines.push(format!(
                "  已认证        : {}",
                if session.authenticated { "✅ 是" } else { "❌ 否" }
            ));
            lines.push(format!(
                "  已过期        : {}",
                if session.is_expired() { "⚠️ 是" } else { "✅ 否" }
            ));
            lines.push(format!("  剩余时间      : {}秒", session.remaining_secs()));
            lines.push(format!("  创建时间      : {}", session.created_at));
            lines.push(format!("  过期时间      : {}", session.expires_at));
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
                if session_mgr.needs_renewal() {
                    "⚠️ 是"
                } else {
                    "否"
                }
            ));
            if let Some(ref token) = session.token {
                let masked = crate::mask::mask_api_key(token);
                lines.push(format!("  Token（脱敏） : {}", masked));
            }
            lines.push(format!(
                "  Refresh Token : {}",
                if session.refresh_token.is_some() {
                    "[REFRESH_TOKEN]"
                } else {
                    "无"
                }
            ));
        }
        None => {
            lines.push("  无活跃 Session".to_string());
            lines.push(String::new());
            lines.push("  建议: 运行 `cargo run -- connect` 创建 Session".to_string());
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());
    lines.join("\n")
}

// ============================================================================
// Connection Diagnostics
// ============================================================================

/// 打印 Connection 诊断信息（V1.07 第十一节 `cargo run -- connection`）。
pub fn diagnose_connection(conn_mgr: &ConnectionManager) -> String {
    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("  Connection 诊断".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    // HTTP
    lines.push("── HTTP ──".to_string());
    lines.push(format!(
        "  状态          : {}",
        conn_mgr.http_state.as_zh()
    ));
    if let Some(ref url) = conn_mgr.http_base_url {
        lines.push(format!("  URL           : {}", url));
    } else {
        lines.push("  URL           : 未配置".to_string());
    }
    lines.push(format!(
        "  健康          : {}",
        if conn_mgr.http_ok() { "✅ 正常" } else { "❌ 异常" }
    ));
    lines.push(String::new());

    // WebSocket
    lines.push("── WebSocket ──".to_string());
    if let Some(ref url) = conn_mgr.ws_url {
        lines.push(format!("  URL           : {}", url));
        lines.push(format!(
            "  状态          : {}",
            conn_mgr.ws_state.as_zh()
        ));
        lines.push(format!(
            "  健康          : {}",
            if conn_mgr.ws_ok() { "✅ 正常" } else { "❌ 异常" }
        ));
    } else {
        lines.push("  未配置".to_string());
    }
    lines.push(String::new());

    // 统计
    lines.push("── 统计 ──".to_string());
    lines.push(conn_mgr.status_summary());
    lines.push(String::new());

    // 重试
    lines.push("── 重试策略 ──".to_string());
    lines.push(format!(
        "  最大重试      : {} 次",
        conn_mgr.retry_policy.max_retries
    ));
    lines.push(format!(
        "  基础退避      : {}ms",
        conn_mgr.retry_policy.base_backoff_ms
    ));
    lines.push(format!(
        "  最大退避      : {}ms",
        conn_mgr.retry_policy.max_backoff_ms
    ));
    lines.push(format!(
        "  退避乘数      : {}x",
        conn_mgr.retry_policy.backoff_multiplier
    ));
    lines.push(format!(
        "  抖动          : {}",
        if conn_mgr.retry_policy.jitter {
            "是"
        } else {
            "否"
        }
    ));
    lines.push(format!(
        "  当前退避      : {}ms",
        conn_mgr.current_backoff_ms()
    ));
    lines.push(format!(
        "  连接超时      : {}ms",
        conn_mgr.connect_timeout_ms
    ));
    lines.push(format!(
        "  请求超时      : {}ms",
        conn_mgr.request_timeout_ms
    ));
    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Credential Diagnostics
// ============================================================================

/// 打印 Credential 诊断信息（安全脱敏）。
pub fn diagnose_credential(cred_mgr: &CredentialManager) -> String {
    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("  Credential 诊断".to_string());
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
    lines.push(String::new());
    lines.push(cred_mgr.safe_summary());
    lines.push(String::new());

    if !cred_mgr.has_real_credentials() {
        lines.push("  当前无真实凭据，使用 Mock 模式。".to_string());
        lines.push("  如需真实交易，请配置凭据（环境变量或 .env 文件）。".to_string());
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockTradingProvider;

    #[tokio::test]
    async fn diagnose_provider_output() {
        let provider = MockTradingProvider::new();
        let output = diagnose_provider(&provider).await;
        assert!(output.contains("Provider 诊断"));
        assert!(output.contains("MockTradingProvider"));
        assert!(output.contains("Mock"));
    }

    #[tokio::test]
    async fn diagnose_health_output() {
        let provider = MockTradingProvider::new();
        let output = diagnose_health(&provider).await;
        assert!(output.contains("Health 诊断"));
        assert!(output.contains("系统健康"));
    }

    #[test]
    fn diagnose_session_output() {
        let mut mgr = SessionManager::with_defaults();
        mgr.create_session(Some("test-token".into()));
        let output = diagnose_session(&mgr);
        assert!(output.contains("Session 诊断"));
        assert!(output.contains("已认证"));
        assert!(!output.contains("test-token")); // 脱敏
    }

    #[test]
    fn diagnose_session_empty() {
        let mgr = SessionManager::with_defaults();
        let output = diagnose_session(&mgr);
        assert!(output.contains("无活跃 Session"));
    }

    #[test]
    fn diagnose_connection_output() {
        let mgr = ConnectionManager::new();
        let output = diagnose_connection(&mgr);
        assert!(output.contains("Connection 诊断"));
        assert!(output.contains("HTTP"));
    }

    #[test]
    fn diagnose_credential_output() {
        let mgr = CredentialManager::new();
        let output = diagnose_credential(&mgr);
        assert!(output.contains("Credential 诊断"));
        assert!(output.contains("Mock"));
    }
}
