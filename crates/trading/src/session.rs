//! Session Manager（V1.07 第四节）。
//!
//! 管理登录状态、Token、Session 续期与自动恢复。
//! 统一 Session 管理，避免各模块各自维护。

use chrono::{DateTime, Duration, Utc};
use tracing::{info, warn};

// ============================================================================
// Session
// ============================================================================

/// 会话信息。
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID。
    pub session_id: String,
    /// 认证 Token。
    pub token: Option<String>,
    /// 刷新 Token。
    pub refresh_token: Option<String>,
    /// Session 创建时间。
    pub created_at: DateTime<Utc>,
    /// Session 过期时间。
    pub expires_at: DateTime<Utc>,
    /// 最后活跃时间。
    pub last_active: DateTime<Utc>,
    /// 是否已认证。
    pub authenticated: bool,
    /// Session 元数据。
    pub metadata: Vec<(String, String)>,
}

impl Session {
    /// 创建新 Session。
    pub fn new(session_id: &str, token: Option<String>, ttl_secs: i64) -> Self {
        let now = Utc::now();
        Self {
            session_id: session_id.to_string(),
            token,
            refresh_token: None,
            created_at: now,
            expires_at: now + Duration::seconds(ttl_secs),
            last_active: now,
            authenticated: false,
            metadata: Vec::new(),
        }
    }

    /// 是否已过期。
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// 是否即将过期（在指定秒数内）。
    pub fn expires_soon(&self, within_secs: i64) -> bool {
        Utc::now() + Duration::seconds(within_secs) >= self.expires_at
    }

    /// 剩余有效时间（秒）。
    pub fn remaining_secs(&self) -> i64 {
        let remaining = (self.expires_at - Utc::now()).num_seconds();
        remaining.max(0)
    }

    /// 更新最后活跃时间。
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// 续期。
    pub fn renew(&mut self, new_token: Option<String>, ttl_secs: i64) {
        let now = Utc::now();
        self.token = new_token.or_else(|| self.token.clone());
        self.expires_at = now + Duration::seconds(ttl_secs);
        self.last_active = now;
        info!("Session {} 已续期，新过期时间: {}", self.session_id, self.expires_at);
    }

    /// 安全摘要（脱敏）。
    pub fn safe_summary(&self) -> String {
        let token_status = if self.token.is_some() {
            "[TOKEN]"
        } else {
            "无"
        };
        let refresh_status = if self.refresh_token.is_some() {
            "[REFRESH_TOKEN]"
        } else {
            "无"
        };
        let auth_status = if self.authenticated { "是" } else { "否" };
        format!(
            "Session: {} | Token: {} | Refresh: {} | 已认证: {} | 剩余: {}s | 过期: {}",
            self.session_id,
            token_status,
            refresh_status,
            auth_status,
            self.remaining_secs(),
            self.expires_at
        )
    }
}

// ============================================================================
// Session Manager
// ============================================================================

/// Session 管理器（V1.07 第四节）。
///
/// 负责：登录状态 / Token / Session 续期 / 重连 / 过期 / 自动恢复。
pub struct SessionManager {
    /// 当前 Session。
    current_session: Option<Session>,
    /// Session TTL（秒）。
    session_ttl_secs: i64,
    /// 续期阈值（剩余多少秒时触发续期）。
    renew_threshold_secs: i64,
    /// 自动续期开关。
    auto_renew: bool,
    /// Session 计数器（用于生成 ID）。
    session_counter: u64,
}

impl SessionManager {
    /// 创建 Session 管理器。
    pub fn new(session_ttl_secs: i64) -> Self {
        Self {
            current_session: None,
            session_ttl_secs,
            renew_threshold_secs: session_ttl_secs / 3, // 剩余 1/3 时续期
            auto_renew: true,
            session_counter: 0,
        }
    }

    /// 默认配置。
    pub fn with_defaults() -> Self {
        Self::new(3600) // 1 小时 TTL
    }

    /// 设置续期阈值。
    pub fn with_renew_threshold(mut self, secs: i64) -> Self {
        self.renew_threshold_secs = secs;
        self
    }

    /// 设置自动续期。
    pub fn with_auto_renew(mut self, enabled: bool) -> Self {
        self.auto_renew = enabled;
        self
    }

    /// 创建新 Session（登录）。
    pub fn create_session(&mut self, token: Option<String>) -> &Session {
        self.session_counter += 1;
        let id = format!("SESS-{:04}", self.session_counter);
        let mut session = Session::new(&id, token, self.session_ttl_secs);
        session.authenticated = true;
        info!(
            "创建新 Session: {} (TTL: {}s, 已认证)",
            id, self.session_ttl_secs
        );
        self.current_session = Some(session);
        self.current_session.as_ref().unwrap()
    }

    /// 获取当前 Session。
    pub fn current(&self) -> Option<&Session> {
        self.current_session.as_ref()
    }

    /// 获取当前 Session（可变）。
    pub fn current_mut(&mut self) -> Option<&mut Session> {
        self.current_session.as_mut()
    }

    /// 当前 Session 是否有效。
    pub fn is_valid(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.authenticated && !s.is_expired())
            .unwrap_or(false)
    }

    /// 检查是否需要续期。
    pub fn needs_renewal(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.expires_soon(self.renew_threshold_secs))
            .unwrap_or(false)
    }

    /// 续期当前 Session。
    pub fn renew_current(&mut self, new_token: Option<String>) -> bool {
        if let Some(session) = self.current_session.as_mut() {
            session.renew(new_token, self.session_ttl_secs);
            true
        } else {
            warn!("续期失败：无当前 Session");
            false
        }
    }

    /// 检查并自动续期。
    pub fn check_and_renew(&mut self) -> bool {
        if !self.auto_renew {
            return false;
        }
        if self.needs_renewal() {
            info!("Session 即将过期，触发自动续期");
            self.renew_current(None)
        } else {
            false
        }
    }

    /// 销毁当前 Session（登出）。
    pub fn destroy(&mut self) {
        if let Some(session) = self.current_session.take() {
            info!("Session {} 已销毁", session.session_id);
        }
    }

    /// 是否能恢复（Session 过期但未超过太久）。
    pub fn can_recover(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.expires_soon(300)) // 过期 5 分钟内可恢复
            .unwrap_or(false)
    }

    /// Session 状态摘要。
    pub fn status_summary(&self) -> String {
        match &self.current_session {
            Some(s) => s.safe_summary(),
            None => "无活跃 Session".to_string(),
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_new_not_expired() {
        let session = Session::new("S-001", Some("tok".into()), 3600);
        assert!(!session.is_expired());
        assert!(session.remaining_secs() > 0);
        assert!(!session.authenticated);
    }

    #[test]
    fn session_expires_soon() {
        let session = Session::new("S-001", Some("tok".into()), 3600);
        assert!(!session.expires_soon(60));
        // 长时间 TTL 开头不应立即过期
        assert!(!session.is_expired());
    }

    #[test]
    fn session_touch_updates_active() {
        let mut session = Session::new("S-001", Some("tok".into()), 3600);
        let before = session.last_active;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();
        assert!(session.last_active > before);
    }

    #[test]
    fn session_manager_create_session() {
        let mut mgr = SessionManager::with_defaults();
        assert!(!mgr.is_valid());

        mgr.create_session(Some("test-token".into()));
        assert!(mgr.is_valid());
        assert!(mgr.current().unwrap().authenticated);
    }

    #[test]
    fn session_manager_needs_no_renewal_when_fresh() {
        let mut mgr = SessionManager::new(3600);
        mgr.create_session(Some("tok".into()));
        assert!(!mgr.needs_renewal());
    }

    #[test]
    fn session_manager_destroy() {
        let mut mgr = SessionManager::with_defaults();
        mgr.create_session(Some("tok".into()));
        assert!(mgr.is_valid());
        mgr.destroy();
        assert!(!mgr.is_valid());
    }

    #[test]
    fn session_manager_default() {
        let mgr = SessionManager::default();
        assert_eq!(mgr.session_ttl_secs, 3600);
    }

    #[test]
    fn session_safe_summary_no_leak() {
        let session = Session::new("S-001", Some("secret-token-value".into()), 3600);
        let summary = session.safe_summary();
        assert!(!summary.contains("secret-token-value"));
        assert!(summary.contains("[TOKEN]"));
        assert!(summary.contains("S-001"));
    }
}
