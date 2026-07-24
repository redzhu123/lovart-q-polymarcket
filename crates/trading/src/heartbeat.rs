//! Heartbeat（V1.07 第六节）。
//!
//! 周期检查 Provider、连接、Session、Gateway 健康状态。
//! 启动时打印连接状态。

use chrono::{DateTime, Duration, Utc};
use tracing::{debug, info, warn};

use crate::provider::TradingProvider;
use crate::state::TradingState;

// ============================================================================
// Heartbeat Result
// ============================================================================

/// 单次心跳检查结果。
#[derive(Debug, Clone)]
pub struct HeartbeatResult {
    /// 时间戳。
    pub timestamp: DateTime<Utc>,
    /// Provider 名称。
    pub provider: String,
    /// 是否全部健康。
    pub all_healthy: bool,
    /// Provider 健康。
    pub provider_healthy: bool,
    /// HTTP 连接健康。
    pub http_healthy: bool,
    /// WebSocket 连接健康（如无 WS，视为 healthy）。
    pub ws_healthy: bool,
    /// Session 有效。
    pub session_valid: bool,
    /// 延迟（毫秒）。
    pub latency_ms: u64,
    /// 当前状态。
    pub state: TradingState,
    /// 详情。
    pub details: Vec<String>,
}

impl HeartbeatResult {
    /// 健康摘要（单行中文）。
    pub fn summary_one_line(&self) -> String {
        let status = if self.all_healthy { "✅" } else { "❌" };
        format!(
            "{} [{}] {} | HTTP:{} WS:{} Session:{} | {}ms",
            status,
            self.timestamp.format("%H:%M:%S"),
            self.provider,
            if self.http_healthy { "✅" } else { "❌" },
            if self.ws_healthy { "✅" } else { "❌" },
            if self.session_valid { "✅" } else { "❌" },
            self.latency_ms,
        )
    }

    /// 详细摘要（多行中文）。
    pub fn summary_detailed(&self) -> String {
        let status = if self.all_healthy {
            "✅ 系统健康"
        } else {
            "❌ 系统异常"
        };
        let mut lines = vec![
            format!("════════════════════════════════"),
            format!(
                "  {} - {}",
                status,
                self.timestamp.format("%Y-%m-%d %H:%M:%S")
            ),
            format!("════════════════════════════════"),
            format!("  Provider   : {} ({})", self.provider, self.state.as_zh()),
            format!(
                "  HTTP       : {}",
                if self.http_healthy {
                    "✅ 正常"
                } else {
                    "❌ 异常"
                }
            ),
            format!(
                "  WebSocket  : {}",
                if self.ws_healthy {
                    "✅ 正常"
                } else {
                    "❌ 异常"
                }
            ),
            format!(
                "  Session    : {}",
                if self.session_valid {
                    "✅ 有效"
                } else {
                    "❌ 无效"
                }
            ),
            format!("  延迟       : {}ms", self.latency_ms),
        ];

        if !self.details.is_empty() {
            lines.push(String::new());
            lines.push("  详情:".to_string());
            for detail in &self.details {
                lines.push(format!("    - {}", detail));
            }
        }

        lines.push(String::new());
        lines.join("\n")
    }
}

// ============================================================================
// Heartbeat
// ============================================================================

/// Heartbeat 引擎（V1.07 第六节）。
///
/// 周期调用，检查所有组件健康状态。
/// 启动时打印连接状态块。
pub struct Heartbeat {
    /// 心跳间隔（秒）。
    pub interval_secs: u64,
    /// 上次心跳时间。
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// 心跳计数。
    pub beat_count: u64,
    /// 连续失败次数。
    pub consecutive_failures: u32,
    /// 最大连续失败次数（超过触发告警）。
    pub max_consecutive_failures: u32,
    /// 历史心跳结果（最近 N 次）。
    pub history: Vec<HeartbeatResult>,
    /// 历史保留数量。
    pub history_size: usize,
}

impl Heartbeat {
    /// 创建 Heartbeat。
    pub fn new(interval_secs: u64) -> Self {
        Self {
            interval_secs,
            last_heartbeat: None,
            beat_count: 0,
            consecutive_failures: 0,
            max_consecutive_failures: 5,
            history: Vec::new(),
            history_size: 20,
        }
    }

    /// 默认配置。
    pub fn with_defaults() -> Self {
        Self::new(30)
    }

    /// 执行心跳检查。
    pub async fn beat(&mut self, provider: &dyn TradingProvider) -> HeartbeatResult {
        self.beat_count += 1;
        let now = Utc::now();
        self.last_heartbeat = Some(now);

        let health = provider.health().await;

        let result = HeartbeatResult {
            timestamp: now,
            provider: provider.name().to_string(),
            all_healthy: health.healthy,
            provider_healthy: health.healthy,
            http_healthy: health.http_ok,
            ws_healthy: health.ws_ok,
            session_valid: health.session_valid,
            latency_ms: health.latency_ms,
            state: provider.state(),
            details: if health.healthy {
                vec![]
            } else {
                vec![health.detail.clone()]
            },
        };

        if result.all_healthy {
            self.consecutive_failures = 0;
            debug!("{}", result.summary_one_line());
        } else {
            self.consecutive_failures += 1;
            warn!("{}", result.summary_one_line());

            if self.consecutive_failures >= self.max_consecutive_failures {
                warn!(
                    "⚠️ 心跳连续失败 {} 次，达到告警阈值！",
                    self.consecutive_failures
                );
            }
        }

        // 保留历史
        self.history.push(result.clone());
        if self.history.len() > self.history_size {
            self.history.remove(0);
        }

        result
    }

    /// 启动时打印连接状态（V1.07 第六节）。
    pub async fn startup_check(&mut self, provider: &dyn TradingProvider) {
        info!("════════════════════════════════");
        info!("  Trading 启动健康检查");
        info!("════════════════════════════════");

        let health = provider.health().await;
        info!("  Provider  : {}", provider.name());
        info!(
            "  HTTP      : {}",
            if health.http_ok {
                "✅ 正常"
            } else {
                "❌ 异常"
            }
        );
        info!(
            "  WebSocket : {}",
            if health.ws_ok {
                "✅ 正常"
            } else {
                "❌ 异常"
            }
        );
        info!(
            "  Session   : {}",
            if health.session_valid {
                "✅ 有效"
            } else {
                "❌ 无效"
            }
        );
        info!("  状态      : {}", provider.state().as_zh());
        info!(
            "  环境      : {}",
            if provider.capability().can_real_trading {
                "⚠️ 可真实交易"
            } else {
                "✅ Mock/Dry Run"
            }
        );
        info!("════════════════════════════════");

        // 打印能力表
        info!("\n{}", provider.capability().render_table());
    }

    /// 是否应该执行心跳（距上次心跳超过间隔）。
    pub fn should_beat(&self) -> bool {
        match self.last_heartbeat {
            Some(last) => Utc::now() - last >= Duration::seconds(self.interval_secs as i64),
            None => true,
        }
    }

    /// 获取可用率（历史中心跳健康的比例）。
    pub fn availability_rate(&self) -> f64 {
        if self.history.is_empty() {
            return 1.0;
        }
        let healthy_count = self.history.iter().filter(|r| r.all_healthy).count();
        healthy_count as f64 / self.history.len() as f64
    }

    /// 心跳统计摘要。
    pub fn stats_summary(&self) -> String {
        format!(
            "心跳统计: 总计 {} 次 | 连续失败 {} 次 | 可用率 {:.1}% | 间隔 {}s",
            self.beat_count,
            self.consecutive_failures,
            self.availability_rate() * 100.0,
            self.interval_secs,
        )
    }
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockTradingProvider;

    #[tokio::test]
    async fn heartbeat_beat_succeeds() {
        let provider = MockTradingProvider::new();
        let mut hb = Heartbeat::with_defaults();

        let result = hb.beat(&provider).await;
        assert!(result.all_healthy);
        assert_eq!(hb.beat_count, 1);
        assert!(hb.consecutive_failures == 0);
    }

    #[tokio::test]
    async fn heartbeat_startup_check() {
        let provider = MockTradingProvider::new();
        let mut hb = Heartbeat::with_defaults();
        hb.startup_check(&provider).await;
        assert!(hb.beat_count == 0); // startup_check 不计入 beat_count
    }

    #[test]
    fn heartbeat_should_beat_initially() {
        let hb = Heartbeat::with_defaults();
        assert!(hb.should_beat());
    }

    #[test]
    fn heartbeat_availability_rate() {
        let hb = Heartbeat {
            history: vec![],
            ..Heartbeat::with_defaults()
        };
        assert_eq!(hb.availability_rate(), 1.0);
    }

    #[test]
    fn heartbeat_result_one_line() {
        let result = HeartbeatResult {
            timestamp: Utc::now(),
            provider: "Mock".into(),
            all_healthy: true,
            provider_healthy: true,
            http_healthy: true,
            ws_healthy: true,
            session_valid: true,
            latency_ms: 5,
            state: TradingState::Ready,
            details: vec![],
        };
        let line = result.summary_one_line();
        assert!(line.contains("✅"));
        assert!(line.contains("Mock"));
    }

    #[test]
    fn default_heartbeat_creates() {
        let hb = Heartbeat::default();
        assert_eq!(hb.interval_secs, 30);
    }
}
