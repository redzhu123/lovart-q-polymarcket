//! Session 管理（P2-06 第二节）。
//!
//! 在 pm-trading SessionManager 基础上扩展企业级特性：
//! - Session 生命周期管理
//! - Token 存储与脱敏
//! - 过期检测与续期提示
//! - 多 Session 支持

use chrono::{DateTime, Duration, Local};

// ============================================================================
// AuthSession — 认证会话
// ============================================================================

/// 认证会话。
#[derive(Debug, Clone)]
pub struct AuthSession {
    /// 会话 ID。
    pub session_id: String,
    /// Provider 名称。
    pub provider: String,
    /// 是否已认证。
    pub authenticated: bool,
    /// Access Token（脱敏）。
    pub access_token: Option<String>,
    /// Refresh Token（脱敏）。
    pub refresh_token: Option<String>,
    /// 会话创建时间。
    pub created_at: DateTime<Local>,
    /// 会话过期时间。
    pub expires_at: DateTime<Local>,
    /// Token 类型（如 "Bearer"）。
    pub token_type: String,
    /// 权限范围。
    pub scopes: Vec<String>,
    /// 会话元数据。
    pub metadata: std::collections::HashMap<String, String>,
}

impl AuthSession {
    /// 创建新会话。
    pub fn new(
        provider: &str,
        ttl_secs: i64,
        access_token: Option<String>,
        refresh_token: Option<String>,
    ) -> Self {
        let now = Local::now();
        let session_id = generate_session_id();
        Self {
            session_id,
            provider: provider.to_string(),
            authenticated: access_token.is_some(),
            access_token,
            refresh_token,
            created_at: now,
            expires_at: now + Duration::seconds(ttl_secs),
            token_type: "Bearer".to_string(),
            scopes: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 创建未认证的空会话。
    pub fn unauthenticated(provider: &str) -> Self {
        Self::new(provider, 0, None, None)
    }

    /// 是否已过期。
    pub fn is_expired(&self) -> bool {
        Local::now() >= self.expires_at
    }

    /// 剩余有效时间（秒）。
    pub fn remaining_secs(&self) -> i64 {
        let remaining = (self.expires_at - Local::now()).num_seconds();
        remaining.max(0)
    }

    /// 是否即将过期（在指定秒数内）。
    pub fn expires_soon(&self, within_secs: i64) -> bool {
        let remaining = self.remaining_secs();
        remaining > 0 && remaining <= within_secs
    }

    /// 是否需要续期。
    pub fn needs_renewal(&self) -> bool {
        self.is_expired() || self.expires_soon(300)
    }

    /// 安全摘要（中文，Token 脱敏）。
    pub fn safe_summary(&self) -> String {
        let access_masked = self
            .access_token
            .as_ref()
            .map(|t| pm_trading::mask::mask_api_key(t))
            .unwrap_or_else(|| "无".to_string());
        let refresh_masked = if self.refresh_token.is_some() {
            "[REFRESH_TOKEN]"
        } else {
            "无"
        };
        format!(
            "Session: {} | Provider: {} | 已认证: {} | 过期: {} | 剩余: {}秒 | Token: {} | Refresh: {}",
            &self.session_id[..8.min(self.session_id.len())],
            self.provider,
            if self.authenticated {
                "✅ 是"
            } else {
                "❌ 否"
            },
            if self.is_expired() {
                "⚠️ 是"
            } else {
                "✅ 否"
            },
            self.remaining_secs(),
            access_masked,
            refresh_masked,
        )
    }
}

// ============================================================================
// AuthSessionManager — 会话管理器
// ============================================================================

/// 认证会话管理器（P2-06 第二节）。
///
/// 管理多个 Provider 的会话生命周期。
pub struct AuthSessionManager {
    /// Provider 名称 -> 会话。
    sessions: std::collections::HashMap<String, AuthSession>,
    /// 默认会话 TTL（秒）。
    default_ttl: i64,
}

impl AuthSessionManager {
    /// 创建新的会话管理器。
    pub fn new(default_ttl_secs: i64) -> Self {
        Self {
            sessions: std::collections::HashMap::new(),
            default_ttl: default_ttl_secs,
        }
    }

    /// 使用默认 TTL 创建（3600 秒 = 1 小时）。
    pub fn with_defaults() -> Self {
        Self::new(3600)
    }

    /// 创建或更新会话。
    pub fn create_session(
        &mut self,
        provider: &str,
        access_token: Option<String>,
        refresh_token: Option<String>,
    ) -> &AuthSession {
        let session = AuthSession::new(provider, self.default_ttl, access_token, refresh_token);
        tracing::info!(
            provider = %provider,
            session_id = %session.session_id,
            authenticated = session.authenticated,
            "创建认证会话"
        );
        self.sessions.insert(provider.to_string(), session);
        self.sessions.get(provider).unwrap()
    }

    /// 获取指定 Provider 的会话。
    pub fn get(&self, provider: &str) -> Option<&AuthSession> {
        self.sessions.get(provider)
    }

    /// 获取当前活跃会话（第一个已认证的）。
    pub fn current(&self) -> Option<&AuthSession> {
        self.sessions
            .values()
            .find(|s| s.authenticated && !s.is_expired())
    }

    /// 注销会话。
    pub fn invalidate(&mut self, provider: &str) {
        if let Some(session) = self.sessions.get_mut(provider) {
            session.authenticated = false;
            session.access_token = None;
            session.refresh_token = None;
            tracing::info!(provider = %provider, "会话已注销");
        }
    }

    /// 续期会话。
    pub fn renew(
        &mut self,
        provider: &str,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Option<&AuthSession> {
        if let Some(session) = self.sessions.get_mut(provider) {
            session.access_token = Some(access_token);
            session.refresh_token = refresh_token;
            session.expires_at = Local::now() + Duration::seconds(self.default_ttl);
            session.authenticated = true;
            tracing::info!(
                provider = %provider,
                "会话续期完成"
            );
            Some(session)
        } else {
            tracing::warn!(provider = %provider, "会话不存在，无法续期");
            None
        }
    }

    /// 检查是否有会话需要续期。
    pub fn needs_renewal(&self) -> bool {
        self.sessions
            .values()
            .any(|s| s.authenticated && s.needs_renewal())
    }

    /// 列出所有 Provider 名称。
    pub fn provider_names(&self) -> Vec<&str> {
        self.sessions.keys().map(|s| s.as_str()).collect()
    }

    /// 会话数量。
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// 全部会话过期检查。
    pub fn all_expired(&self) -> bool {
        self.sessions.values().all(|s| s.is_expired())
    }

    /// 移除过期会话。
    pub fn purge_expired(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| !s.is_expired());
        let removed = before - self.sessions.len();
        if removed > 0 {
            tracing::info!(removed = removed, "清理过期会话");
        }
        removed
    }

    /// 安全摘要（中文）。
    pub fn safe_summary(&self) -> String {
        if self.sessions.is_empty() {
            return "无活跃会话".to_string();
        }
        let mut lines: Vec<String> = vec![format!("会话数量: {}", self.sessions.len())];
        for (provider, session) in &self.sessions {
            lines.push(format!("  {}: {}", provider, session.safe_summary()));
        }
        lines.join("\n")
    }
}

impl Default for AuthSessionManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// 辅助
// ============================================================================

fn generate_session_id() -> String {
    let bytes: [u8; 16] = rand::random();
    format!(
        "sess-{}-{}",
        chrono::Local::now().format("%Y%m%d%H%M%S"),
        hex_encode(&bytes)
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_session_creation() {
        let session = AuthSession::new("polymarket", 3600, Some("token-123".into()), None);
        assert!(session.authenticated);
        assert!(!session.is_expired());
        assert!(session.remaining_secs() > 0);
        assert_eq!(session.provider, "polymarket");
    }

    #[test]
    fn auth_session_unauthenticated() {
        let session = AuthSession::unauthenticated("kalshi");
        assert!(!session.authenticated);
        assert!(session.is_expired()); // TTL = 0
    }

    #[test]
    fn auth_session_safe_summary_masks_token() {
        let session = AuthSession::new(
            "polymarket",
            3600,
            Some("sk-abcdefghijklmnopqrstuv".into()),
            Some("rt-secret".into()),
        );
        let summary = session.safe_summary();
        assert!(!summary.contains("sk-abcdefghijklmnopqrstuv"));
        assert!(!summary.contains("rt-secret"));
        assert!(summary.contains("[REFRESH_TOKEN]"));
    }

    #[test]
    fn session_manager_create_and_get() {
        let mut mgr = AuthSessionManager::with_defaults();
        mgr.create_session("polymarket", Some("tok".into()), None);
        assert!(mgr.get("polymarket").is_some());
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn session_manager_current_returns_authenticated() {
        let mut mgr = AuthSessionManager::with_defaults();
        mgr.create_session("p1", None, None);
        mgr.create_session("p2", Some("tok".into()), None);
        let current = mgr.current().unwrap();
        assert_eq!(current.provider, "p2");
    }

    #[test]
    fn session_manager_invalidate() {
        let mut mgr = AuthSessionManager::with_defaults();
        mgr.create_session("polymarket", Some("tok".into()), None);
        mgr.invalidate("polymarket");
        let s = mgr.get("polymarket").unwrap();
        assert!(!s.authenticated);
    }

    #[test]
    fn session_manager_renew() {
        let mut mgr = AuthSessionManager::with_defaults();
        mgr.create_session("polymarket", Some("old".into()), None);
        mgr.renew("polymarket", "new-token".into(), Some("new-rt".into()));
        let s = mgr.get("polymarket").unwrap();
        assert!(s.authenticated);
    }

    #[test]
    fn session_manager_purge_expired() {
        let mut mgr = AuthSessionManager::new(0); // TTL=0 means immediate expiry
        mgr.create_session("p1", Some("tok".into()), None);
        assert_eq!(mgr.purge_expired(), 1);
        assert!(mgr.is_empty());
    }

    #[test]
    fn session_manager_default_is_empty() {
        let mgr = AuthSessionManager::with_defaults();
        assert!(mgr.is_empty());
    }
}
