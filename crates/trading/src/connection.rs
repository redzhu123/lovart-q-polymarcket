//! Connection Manager（V1.07 第五节）。
//!
//! 统一管理 HTTP / WebSocket 连接。
//! Execution 禁止直接 HTTP — 所有连接通过 ConnectionManager。
//!
//! 功能：
//! - HTTP 连接池
//! - WebSocket 连接
//! - 自动重连
//! - 指数退避
//! - 重试策略
//! - Heartbeat 集成
//! - 连接健康检查

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

// ============================================================================
// Retry Policy
// ============================================================================

/// 重试策略。
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数。
    pub max_retries: u32,
    /// 基础退避时间（毫秒）。
    pub base_backoff_ms: u64,
    /// 最大退避时间（毫秒）。
    pub max_backoff_ms: u64,
    /// 退避乘数（指数退避）。
    pub backoff_multiplier: f64,
    /// 是否添加随机抖动。
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_backoff_ms: 100,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// 计算第 n 次重试的退避时间。
    pub fn backoff_ms(&self, attempt: u32) -> u64 {
        let base = self.base_backoff_ms as f64;
        let multiplier = self.backoff_multiplier.powi(attempt as i32);
        let raw_ms = base * multiplier;
        let ms = if self.jitter {
            // 添加 ±25% 抖动（在 capping 之前应用）
            let jitter_range = raw_ms * 0.25;
            let jitter = (rand::random::<f64>() * 2.0 - 1.0) * jitter_range;
            (raw_ms + jitter).max(0.0)
        } else {
            raw_ms
        };
        ms.min(self.max_backoff_ms as f64) as u64
    }
}

// ============================================================================
// Connection State
// ============================================================================

/// 连接状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// 未连接。
    Disconnected,
    /// 连接中。
    Connecting,
    /// 已连接。
    Connected,
    /// 连接失败。
    Failed,
    /// 重连中。
    Reconnecting,
}

impl ConnectionState {
    pub fn as_zh(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "未连接",
            ConnectionState::Connecting => "连接中",
            ConnectionState::Connected => "已连接",
            ConnectionState::Failed => "连接失败",
            ConnectionState::Reconnecting => "重连中",
        }
    }
}

// ============================================================================
// Connection Stats
// ============================================================================

/// 连接统计。
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    /// 总连接次数。
    pub total_connections: u64,
    /// 成功连接次数。
    pub successful_connections: u64,
    /// 失败连接次数。
    pub failed_connections: u64,
    /// 总重试次数。
    pub total_retries: u64,
    /// 断线次数。
    pub disconnects: u64,
    /// 最后连接时间。
    pub last_connected: Option<DateTime<Utc>>,
    /// 最后断线时间。
    pub last_disconnected: Option<DateTime<Utc>>,
    /// 当前连续成功连接时长（秒）。
    pub uptime_secs: u64,
}

impl ConnectionStats {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let success_rate = if self.total_connections > 0 {
            (self.successful_connections as f64 / self.total_connections as f64) * 100.0
        } else {
            0.0
        };
        format!(
            "连接统计: 总计 {} 次 | 成功 {} 次 ({:.1}%) | 失败 {} 次 | 重试 {} 次 | 断线 {} 次 | 运行 {}s",
            self.total_connections,
            self.successful_connections,
            success_rate,
            self.failed_connections,
            self.total_retries,
            self.disconnects,
            self.uptime_secs,
        )
    }
}

// ============================================================================
// Connection Manager
// ============================================================================

/// 连接管理器（V1.07 第五节）。
///
/// 统一管理 HTTP / WebSocket 连接。
/// 提供：自动重连 / 指数退避 / 重试 / Heartbeat 集成 / 连接池。
pub struct ConnectionManager {
    /// HTTP 基础 URL。
    pub http_base_url: Option<String>,
    /// WebSocket URL。
    pub ws_url: Option<String>,
    /// HTTP 连接状态。
    pub http_state: ConnectionState,
    /// WebSocket 连接状态。
    pub ws_state: ConnectionState,
    /// 重试策略。
    pub retry_policy: RetryPolicy,
    /// 连接统计。
    pub stats: ConnectionStats,
    /// 连接超时（毫秒）。
    pub connect_timeout_ms: u64,
    /// 请求超时（毫秒）。
    pub request_timeout_ms: u64,
    /// 最大连接池大小。
    pub max_pool_size: usize,
    /// 当前重试计数。
    retry_count: u32,
}

impl ConnectionManager {
    /// 创建连接管理器。
    pub fn new() -> Self {
        Self {
            http_base_url: None,
            ws_url: None,
            http_state: ConnectionState::Disconnected,
            ws_state: ConnectionState::Disconnected,
            retry_policy: RetryPolicy::default(),
            stats: ConnectionStats::default(),
            connect_timeout_ms: 5000,
            request_timeout_ms: 30000,
            max_pool_size: 10,
            retry_count: 0,
        }
    }

    /// 设置 HTTP 基础 URL。
    pub fn with_http(mut self, url: &str) -> Self {
        self.http_base_url = Some(url.to_string());
        self
    }

    /// 设置 WebSocket URL。
    pub fn with_ws(mut self, url: &str) -> Self {
        self.ws_url = Some(url.to_string());
        self
    }

    /// 设置重试策略。
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// 标记 HTTP 连接尝试开始。
    pub fn http_connecting(&mut self) {
        self.http_state = ConnectionState::Connecting;
        self.stats.total_connections += 1;
        debug!("HTTP 连接中... ({})", self.stats.total_connections);
    }

    /// 标记 HTTP 连接成功。
    pub fn http_connected(&mut self) {
        self.http_state = ConnectionState::Connected;
        self.stats.successful_connections += 1;
        self.stats.last_connected = Some(Utc::now());
        self.retry_count = 0;
        info!("HTTP 已连接");
    }

    /// 标记 HTTP 连接失败。
    pub fn http_failed(&mut self, reason: &str) {
        self.stats.failed_connections += 1;
        warn!(
            "HTTP 连接失败: {} (重试 {}/{})",
            reason, self.retry_count, self.retry_policy.max_retries
        );

        if self.retry_count < self.retry_policy.max_retries {
            self.http_state = ConnectionState::Reconnecting;
            self.retry_count += 1;
            self.stats.total_retries += 1;
            let backoff = self.retry_policy.backoff_ms(self.retry_count);
            debug!("将在 {}ms 后重连", backoff);
        } else {
            self.http_state = ConnectionState::Failed;
            warn!(
                "HTTP 重连已达上限 ({} 次)，放弃",
                self.retry_policy.max_retries
            );
        }
    }

    /// 标记断线。
    pub fn http_disconnected(&mut self) {
        self.http_state = ConnectionState::Disconnected;
        self.stats.disconnects += 1;
        self.stats.last_disconnected = Some(Utc::now());
        info!("HTTP 已断开");
    }

    /// 标记 WebSocket 连接成功。
    pub fn ws_connected(&mut self) {
        self.ws_state = ConnectionState::Connected;
        info!("WebSocket 已连接");
    }

    /// 标记 WebSocket 断线。
    pub fn ws_disconnected(&mut self) {
        self.ws_state = ConnectionState::Disconnected;
        info!("WebSocket 已断开");
    }

    /// HTTP 是否连接正常。
    pub fn http_ok(&self) -> bool {
        self.http_state == ConnectionState::Connected
    }

    /// WebSocket 是否连接正常。
    pub fn ws_ok(&self) -> bool {
        self.ws_state == ConnectionState::Connected
    }

    /// 所有连接是否正常。
    pub fn all_ok(&self) -> bool {
        self.http_ok() && (self.ws_url.is_none() || self.ws_ok())
    }

    /// 重试退避时间（毫秒）。
    pub fn current_backoff_ms(&self) -> u64 {
        self.retry_policy.backoff_ms(self.retry_count)
    }

    /// 重置重试计数。
    pub fn reset_retries(&mut self) {
        self.retry_count = 0;
    }

    /// 连接状态摘要（中文）。
    pub fn status_summary(&self) -> String {
        let http_status = if let Some(ref url) = self.http_base_url {
            format!("{} ({})", self.http_state.as_zh(), url)
        } else {
            "未配置".to_string()
        };
        let ws_status = if let Some(ref url) = self.ws_url {
            format!("{} ({})", self.ws_state.as_zh(), url)
        } else {
            "未配置".to_string()
        };
        format!(
            "HTTP: {}\nWebSocket: {}\n{}",
            http_status,
            ws_status,
            self.stats.summary_zh()
        )
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_backoff_increases() {
        let policy = RetryPolicy::default();
        let b0 = policy.backoff_ms(0);
        let b1 = policy.backoff_ms(1);
        let b2 = policy.backoff_ms(2);
        assert!(b1 > b0);
        assert!(b2 > b1);
    }

    #[test]
    fn retry_policy_capped() {
        let mut policy = RetryPolicy::default();
        policy.max_backoff_ms = 1000;
        let b10 = policy.backoff_ms(10);
        assert!(b10 <= 1000);
    }

    #[test]
    fn connection_manager_starts_disconnected() {
        let mgr = ConnectionManager::new();
        assert_eq!(mgr.http_state, ConnectionState::Disconnected);
        assert!(!mgr.http_ok());
    }

    #[test]
    fn connection_manager_http_lifecycle() {
        let mut mgr = ConnectionManager::new();
        assert_eq!(mgr.http_state, ConnectionState::Disconnected);

        mgr.http_connecting();
        assert_eq!(mgr.http_state, ConnectionState::Connecting);
        assert_eq!(mgr.stats.total_connections, 1);

        mgr.http_connected();
        assert_eq!(mgr.http_state, ConnectionState::Connected);
        assert_eq!(mgr.stats.successful_connections, 1);
        assert!(mgr.http_ok());

        mgr.http_disconnected();
        assert_eq!(mgr.http_state, ConnectionState::Disconnected);
        assert_eq!(mgr.stats.disconnects, 1);
    }

    #[test]
    fn connection_manager_retry_gives_up() {
        let mut mgr = ConnectionManager::new();
        for _ in 0..=mgr.retry_policy.max_retries {
            mgr.http_failed("测试失败");
        }
        assert_eq!(mgr.http_state, ConnectionState::Failed);
    }

    #[test]
    fn connection_manager_reset_retries() {
        let mut mgr = ConnectionManager::new();
        mgr.http_failed("失败1");
        mgr.http_failed("失败2");
        assert!(mgr.retry_count > 0);
        mgr.reset_retries();
        assert_eq!(mgr.retry_count, 0);
    }

    #[test]
    fn connection_state_as_zh() {
        assert_eq!(ConnectionState::Disconnected.as_zh(), "未连接");
        assert_eq!(ConnectionState::Connected.as_zh(), "已连接");
        assert_eq!(ConnectionState::Reconnecting.as_zh(), "重连中");
    }

    #[test]
    fn stats_summary_zh() {
        let stats = ConnectionStats {
            total_connections: 10,
            successful_connections: 9,
            failed_connections: 1,
            total_retries: 3,
            disconnects: 1,
            ..Default::default()
        };
        let summary = stats.summary_zh();
        assert!(summary.contains("90.0%"));
        assert!(summary.contains("10 次"));
    }
}
