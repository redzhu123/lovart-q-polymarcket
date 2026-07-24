//! Token 刷新管理（P2-06 第二节）。
//!
//! Session / Token 续期逻辑：
//! - 自动检测即将过期的 Session
//! - 使用 Refresh Token 获取新 Access Token
//! - 刷新失败时的降级策略

use crate::session::AuthSessionManager;

// ============================================================================
// TokenRefreshScheduler
// ============================================================================

/// Token 续期调度器（P2-06 第二节）。
///
/// 负责定期检查会话过期状态并执行续期。
/// 当前为 Simulation Only —— 不连接真实认证服务。
pub struct TokenRefreshScheduler {
    /// 提前续期时间（秒），默认 300（5 分钟）。
    pub renewal_threshold_secs: i64,
    /// 最大续期尝试次数。
    pub max_retries: u32,
    /// 当前累计续期次数。
    pub renewal_count: u64,
    /// 当前累计续期失败次数。
    pub failure_count: u64,
}

impl TokenRefreshScheduler {
    /// 创建续期调度器。
    pub fn new(renewal_threshold_secs: i64, max_retries: u32) -> Self {
        Self {
            renewal_threshold_secs,
            max_retries,
            renewal_count: 0,
            failure_count: 0,
        }
    }

    /// 使用默认配置创建（5 分钟提前续期，最多 3 次尝试）。
    pub fn with_defaults() -> Self {
        Self::new(300, 3)
    }

    /// 检查是否需要续期。
    pub fn check_and_refresh(
        &mut self,
        session_mgr: &mut AuthSessionManager,
    ) -> Vec<RefreshOutcome> {
        let mut outcomes = Vec::new();

        // 收集需要刷新的 provider 名称（避免借用冲突）
        let providers_to_refresh: Vec<String> = session_mgr
            .provider_names()
            .iter()
            .filter_map(|p| {
                let session = session_mgr.get(p)?;
                if session.authenticated && session.expires_soon(self.renewal_threshold_secs) {
                    Some(p.to_string())
                } else {
                    None
                }
            })
            .collect();

        for provider in &providers_to_refresh {
            let outcome = self.try_refresh(session_mgr, provider);
            outcomes.push(outcome);
        }

        outcomes
    }

    fn try_refresh(
        &mut self,
        session_mgr: &mut AuthSessionManager,
        provider: &str,
    ) -> RefreshOutcome {
        tracing::info!(
            provider = %provider,
            threshold = self.renewal_threshold_secs,
            "检测到会话即将过期，尝试续期"
        );

        // Simulation Only：模拟续期
        let new_token = format!("refreshed-token-{}", chrono::Local::now().timestamp());
        let new_refresh = format!("refreshed-rt-{}", rand::random::<u64>());

        match session_mgr.renew(provider, new_token, Some(new_refresh)) {
            Some(_) => {
                self.renewal_count += 1;
                tracing::info!(
                    provider = %provider,
                    renewal_count = self.renewal_count,
                    "会话续期成功"
                );
                RefreshOutcome::success(provider)
            }
            None => {
                self.failure_count += 1;
                tracing::warn!(
                    provider = %provider,
                    failure_count = self.failure_count,
                    "会话续期失败"
                );
                RefreshOutcome::failure(provider, "会话不存在")
            }
        }
    }

    /// 续期统计（中文）。
    pub fn stats_zh(&self) -> String {
        format!(
            "续期成功: {} 次 | 续期失败: {} 次 | 阈值: {}秒 | 最大重试: {}",
            self.renewal_count, self.failure_count, self.renewal_threshold_secs, self.max_retries,
        )
    }
}

impl Default for TokenRefreshScheduler {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// RefreshOutcome
// ============================================================================

/// 续期结果。
#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    pub provider: String,
    pub success: bool,
    pub message: String,
}

impl RefreshOutcome {
    pub fn success(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            success: true,
            message: "续期成功".to_string(),
        }
    }

    pub fn failure(provider: &str, reason: &str) -> Self {
        Self {
            provider: provider.to_string(),
            success: false,
            message: format!("续期失败: {}", reason),
        }
    }

    pub fn summary_zh(&self) -> String {
        format!(
            "{}: {} - {}",
            self.provider,
            if self.success { "✅" } else { "❌" },
            self.message,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_scheduler_creation() {
        let scheduler = TokenRefreshScheduler::new(600, 5);
        assert_eq!(scheduler.renewal_threshold_secs, 600);
        assert_eq!(scheduler.max_retries, 5);
        assert_eq!(scheduler.renewal_count, 0);
        assert_eq!(scheduler.failure_count, 0);
    }

    #[test]
    fn refresh_scheduler_defaults() {
        let scheduler = TokenRefreshScheduler::with_defaults();
        assert_eq!(scheduler.renewal_threshold_secs, 300);
        assert_eq!(scheduler.max_retries, 3);
    }

    #[test]
    fn refresh_scheduler_stats_chinese() {
        let scheduler = TokenRefreshScheduler::with_defaults();
        let stats = scheduler.stats_zh();
        assert!(stats.contains("续期成功"));
        assert!(stats.contains("续期失败"));
    }

    #[test]
    fn refresh_outcome_success() {
        let outcome = RefreshOutcome::success("polymarket");
        assert!(outcome.success);
        assert!(outcome.summary_zh().contains("✅"));
    }

    #[test]
    fn refresh_outcome_failure() {
        let outcome = RefreshOutcome::failure("kalshi", "接口未实现");
        assert!(!outcome.success);
        assert!(outcome.summary_zh().contains("❌"));
    }

    #[test]
    fn check_and_refresh_on_fresh_sessions_returns_empty() {
        let mut session_mgr = AuthSessionManager::new(86400); // 24h TTL, far from expiry
        let mut scheduler = TokenRefreshScheduler::with_defaults();
        session_mgr.create_session("polymarket", Some("tok".into()), None);
        let outcomes = scheduler.check_and_refresh(&mut session_mgr);
        assert!(outcomes.is_empty());
    }
}
