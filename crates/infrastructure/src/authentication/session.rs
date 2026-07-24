//! 会话管理模块。
//!
//! 从 `pm-auth::session` 提取并统一。

use chrono::{DateTime, Duration, Local};
use std::collections::HashMap;

/// 认证会话
#[derive(Debug, Clone)]
pub struct Session {
    /// 会话 ID
    pub session_id: String,
    /// 提供商标识
    pub provider: String,
    /// 访问令牌
    pub access_token: Option<String>,
    /// 刷新令牌
    pub refresh_token: Option<String>,
    /// 是否已认证
    pub authenticated: bool,
    /// 创建时间
    pub created_at: DateTime<Local>,
    /// 过期时间
    pub expires_at: DateTime<Local>,
    /// 存活时间（秒）
    pub ttl_secs: i64,
}

impl Session {
    /// 创建新会话
    pub fn new(session_id: impl Into<String>, provider: impl Into<String>, ttl_secs: i64) -> Self {
        let now = Local::now();
        Self {
            session_id: session_id.into(),
            provider: provider.into(),
            access_token: None,
            refresh_token: None,
            authenticated: false,
            created_at: now,
            expires_at: now + Duration::seconds(ttl_secs),
            ttl_secs,
        }
    }

    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        Local::now() > self.expires_at
    }

    /// 是否即将过期（在阈值秒数内）
    pub fn expires_soon(&self, threshold_secs: i64) -> bool {
        !self.is_expired() && Local::now() + Duration::seconds(threshold_secs) > self.expires_at
    }

    /// 是否需要续期（默认 5 分钟内过期）
    pub fn needs_renewal(&self) -> bool {
        self.expires_soon(300)
    }

    /// 剩余有效时间（秒）
    pub fn remaining_secs(&self) -> i64 {
        let remaining = self.expires_at - Local::now();
        remaining.num_seconds().max(0)
    }
}

/// 会话管理器
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    default_ttl_secs: i64,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(default_ttl_secs: i64) -> Self {
        Self {
            sessions: HashMap::new(),
            default_ttl_secs,
        }
    }

    /// 创建会话
    pub fn create_session(
        &mut self,
        provider: &str,
        token: Option<String>,
        refresh: Option<String>,
    ) -> &Session {
        let session_id = format!("{}-{}", provider, rand::random::<u32>());
        let mut session = Session::new(&session_id, provider, self.default_ttl_secs);
        session.access_token = token;
        session.refresh_token = refresh;
        session.authenticated = true;
        self.sessions.insert(provider.to_string(), session);
        tracing::info!(
            "创建认证会话: {} (TTL={}s)",
            provider,
            self.default_ttl_secs
        );
        self.sessions.get(provider).unwrap()
    }

    /// 获取会话
    pub fn get(&self, provider: &str) -> Option<&Session> {
        self.sessions.get(provider)
    }

    /// 获取当前活跃会话（第一个未过期的）
    pub fn current(&self) -> Option<&Session> {
        self.sessions
            .values()
            .find(|s| s.authenticated && !s.is_expired())
    }

    /// 续期会话
    pub fn renew(
        &mut self,
        provider: &str,
        new_token: String,
        new_refresh: Option<String>,
    ) -> Option<&Session> {
        if let Some(session) = self.sessions.get_mut(provider) {
            session.access_token = Some(new_token);
            session.refresh_token = new_refresh;
            let now = Local::now();
            session.expires_at = now + Duration::seconds(session.ttl_secs);
            tracing::info!("续期认证会话: {}", provider);
            Some(session)
        } else {
            None
        }
    }

    /// 使会话失效
    pub fn invalidate(&mut self, provider: &str) {
        if self.sessions.remove(provider).is_some() {
            tracing::info!("已使无效认证会话: {}", provider);
        }
    }

    /// 清除过期会话
    pub fn purge_expired(&mut self) {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| !s.is_expired());
        let removed = before - self.sessions.len();
        if removed > 0 {
            tracing::debug!("清除 {} 个过期会话", removed);
        }
    }

    /// 会话数量
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// 所有提供商标识
    pub fn provider_names(&self) -> Vec<&str> {
        self.sessions.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_is_not_expired_initially() {
        let session = Session::new("s1", "test", 3600);
        assert!(!session.is_expired());
        assert!(session.authenticated == false);
    }

    #[test]
    fn session_needs_renewal_near_expiry() {
        let session = Session::new("s1", "test", 1); // 1秒 TTL
        // 即将过期
        assert!(session.expires_soon(3600));
    }

    #[test]
    fn session_manager_create_and_get() {
        let mut mgr = SessionManager::new(3600);
        mgr.create_session("polymarket", Some("token-123".to_string()), None);
        let session = mgr.get("polymarket");
        assert!(session.is_some());
        assert!(session.unwrap().authenticated);
    }

    #[test]
    fn session_manager_current() {
        let mut mgr = SessionManager::new(3600);
        mgr.create_session("a", Some("tok-a".to_string()), None);
        mgr.create_session("b", Some("tok-b".to_string()), None);
        let current = mgr.current();
        assert!(current.is_some());
    }

    #[test]
    fn session_manager_renew() {
        let mut mgr = SessionManager::new(3600);
        mgr.create_session("test", Some("old".to_string()), None);
        let renewed = mgr.renew("test", "new-token".to_string(), None);
        assert!(renewed.is_some());
        assert_eq!(renewed.unwrap().access_token.as_deref(), Some("new-token"));
    }

    #[test]
    fn session_manager_invalidate() {
        let mut mgr = SessionManager::new(3600);
        mgr.create_session("test", Some("tok".to_string()), None);
        mgr.invalidate("test");
        assert!(mgr.get("test").is_none());
    }

    #[test]
    fn session_manager_purge_expired() {
        let mut mgr = SessionManager::new(0); // TTL=0 立即过期
        mgr.create_session("expired", Some("tok".to_string()), None);
        mgr.purge_expired();
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn session_manager_is_empty() {
        let mgr = SessionManager::new(3600);
        assert!(mgr.is_empty());
    }
}
