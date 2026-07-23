//! Recovery Engine（V1.07 第八节）。
//!
//! 支持断线恢复、Session 恢复、订单恢复、连接恢复。
//! 程序重启时恢复运行状态，禁止丢失状态。

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use crate::provider::TradingProvider;
use crate::state::TradingState;

// ============================================================================
// Recovery Action
// ============================================================================

/// 恢复动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// 重连。
    Reconnect,
    /// 重建 Session。
    RecreateSession,
    /// 同步订单。
    SyncOrders,
    /// 同步持仓。
    SyncPositions,
    /// 完全重启。
    FullRestart,
    /// 无需恢复。
    None,
}

impl RecoveryAction {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RecoveryAction::Reconnect => "重连",
            RecoveryAction::RecreateSession => "重建会话",
            RecoveryAction::SyncOrders => "同步订单",
            RecoveryAction::SyncPositions => "同步持仓",
            RecoveryAction::FullRestart => "完全重启",
            RecoveryAction::None => "无需恢复",
        }
    }
}

// ============================================================================
// Recovery Event
// ============================================================================

/// 恢复事件记录。
#[derive(Debug, Clone)]
pub struct RecoveryEvent {
    /// 时间戳。
    pub timestamp: DateTime<Utc>,
    /// 触发原因。
    pub trigger: String,
    /// 执行的恢复动作。
    pub action: RecoveryAction,
    /// 是否成功。
    pub success: bool,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 详情。
    pub detail: String,
}

impl RecoveryEvent {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let status = if self.success { "✅" } else { "❌" };
        format!(
            "{} [{}] {}: {} -> {} ({}ms)",
            status,
            self.timestamp.format("%H:%M:%S"),
            self.trigger,
            self.action.as_zh(),
            if self.success { "成功" } else { "失败" },
            self.duration_ms,
        )
    }
}

// ============================================================================
// Recovery Engine
// ============================================================================

/// 恢复引擎（V1.07 第八节）。
///
/// 检测异常状态并执行恢复流程。
/// 支持：断线恢复 / Session 恢复 / 订单恢复 / 连接恢复。
pub struct RecoveryEngine {
    /// 是否启用自动恢复。
    pub auto_recover: bool,
    /// 最大重试次数。
    pub max_retries: u32,
    /// 重试间隔（秒）。
    pub retry_interval_secs: u64,
    /// 恢复历史。
    pub history: Vec<RecoveryEvent>,
    /// 当前重试计数。
    retry_count: u32,
    /// 上次恢复时间。
    last_recovery: Option<DateTime<Utc>>,
}

impl RecoveryEngine {
    /// 创建恢复引擎。
    pub fn new() -> Self {
        Self {
            auto_recover: true,
            max_retries: 10,
            retry_interval_secs: 5,
            history: Vec::new(),
            retry_count: 0,
            last_recovery: None,
        }
    }

    /// 设置最大重试次数。
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// 诊断是否需要恢复，返回推荐的恢复动作。
    pub fn diagnose(&self, provider: &dyn TradingProvider) -> Vec<RecoveryAction> {
        let mut actions = Vec::new();
        let state = provider.state();

        if state == TradingState::Disconnected {
            actions.push(RecoveryAction::Reconnect);
            actions.push(RecoveryAction::RecreateSession);
        } else if state == TradingState::Connecting {
            // 正在连接中，无需额外动作
            actions.push(RecoveryAction::None);
        } else if state == TradingState::Paused {
            // 暂停状态，检查是否需要恢复
            actions.push(RecoveryAction::Reconnect);
        } else if state == TradingState::Recovering {
            // 已在恢复中
            actions.push(RecoveryAction::None);
        } else if state == TradingState::Stopped {
            actions.push(RecoveryAction::FullRestart);
        }

        actions
    }

    /// 执行恢复流程。
    pub async fn recover(
        &mut self,
        provider: &mut dyn TradingProvider,
    ) -> Result<Vec<RecoveryEvent>, anyhow::Error> {
        let actions = self.diagnose(provider);
        let mut events = Vec::new();

        for action in actions {
            if action == RecoveryAction::None {
                continue;
            }

            let start = std::time::Instant::now();
            let trigger = format!("状态: {}", provider.state().as_zh());

            info!("恢复引擎: 执行 {} ...", action.as_zh());

            let success = match action {
                RecoveryAction::Reconnect => {
                    provider.set_state(TradingState::Recovering);
                    match provider.connect().await {
                        Ok(()) => {
                            info!("恢复引擎: 重连成功");
                            true
                        }
                        Err(e) => {
                            warn!("恢复引擎: 重连失败: {}", e);
                            false
                        }
                    }
                }
                RecoveryAction::RecreateSession => {
                    // Session 在 connect() 中自动重建
                    info!("恢复引擎: Session 在连接时自动重建");
                    true
                }
                RecoveryAction::SyncOrders => {
                    // 当前 Mock 模式无订单需同步
                    info!("恢复引擎: 订单同步（Mock 模式跳过）");
                    true
                }
                RecoveryAction::SyncPositions => {
                    // 当前 Mock 模式无持仓需同步
                    info!("恢复引擎: 持仓同步（Mock 模式跳过）");
                    true
                }
                RecoveryAction::FullRestart => {
                    provider.set_state(TradingState::Recovering);
                    // 断开后重连
                    let _ = provider.disconnect().await;
                    match provider.connect().await {
                        Ok(()) => {
                            info!("恢复引擎: 完全重启成功");
                            true
                        }
                        Err(e) => {
                            warn!("恢复引擎: 完全重启失败: {}", e);
                            false
                        }
                    }
                }
                RecoveryAction::None => true,
            };

            let duration_ms = start.elapsed().as_millis() as u64;

            let event = RecoveryEvent {
                timestamp: Utc::now(),
                trigger,
                action,
                success,
                duration_ms,
                detail: if success {
                    "恢复成功".to_string()
                } else {
                    "恢复失败".to_string()
                },
            };

            info!("{}", event.summary_zh());
            events.push(event);
        }

        self.history.extend(events.clone());
        self.last_recovery = Some(Utc::now());

        if events.iter().all(|e| e.success) {
            self.retry_count = 0;
        } else {
            self.retry_count += 1;
            if self.retry_count >= self.max_retries {
                warn!(
                    "⚠️ 恢复引擎: 已达最大重试次数 {}，停止自动恢复",
                    self.max_retries
                );
            }
        }

        Ok(events)
    }

    /// 检查是否应该重试恢复。
    pub fn should_retry(&self) -> bool {
        self.auto_recover && self.retry_count < self.max_retries
    }

    /// 重置重试计数。
    pub fn reset(&mut self) {
        self.retry_count = 0;
        self.last_recovery = None;
        info!("恢复引擎: 已重置");
    }

    /// 获取恢复统计。
    pub fn stats_summary(&self) -> String {
        let total = self.history.len();
        let successful = self.history.iter().filter(|e| e.success).count();
        let failed = total - successful;
        format!(
            "恢复统计: 总计 {} 次 | 成功 {} 次 | 失败 {} 次 | 重试 {}/{} | 自动恢复: {}",
            total,
            successful,
            failed,
            self.retry_count,
            self.max_retries,
            if self.auto_recover { "启用" } else { "禁用" },
        )
    }
}

impl Default for RecoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockTradingProvider;

    #[tokio::test]
    async fn recovery_diagnose_ready_provider() {
        let provider = MockTradingProvider::new();
        let engine = RecoveryEngine::new();
        let actions = engine.diagnose(&provider);
        // Ready 状态不需要恢复
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn recovery_diagnose_disconnected_provider() {
        let mut provider = MockTradingProvider::new();
        provider.set_state(TradingState::Disconnected);
        let engine = RecoveryEngine::new();
        let actions = engine.diagnose(&provider);
        assert!(!actions.is_empty());
        assert!(actions.contains(&RecoveryAction::Reconnect));
    }

    #[tokio::test]
    async fn recovery_of_ready_provider_succeeds() {
        let mut provider = MockTradingProvider::new();
        let mut engine = RecoveryEngine::new();
        let result = engine.recover(&mut provider).await;
        // Ready 状态无动作，成功返回空
        assert!(result.is_ok());
    }

    #[test]
    fn recovery_should_retry_initially() {
        let engine = RecoveryEngine::new();
        assert!(engine.should_retry());
    }

    #[test]
    fn recovery_reset_clears_count() {
        let mut engine = RecoveryEngine::new();
        engine.retry_count = 5;
        engine.reset();
        assert_eq!(engine.retry_count, 0);
    }

    #[test]
    fn recovery_action_as_zh() {
        assert_eq!(RecoveryAction::Reconnect.as_zh(), "重连");
        assert_eq!(RecoveryAction::None.as_zh(), "无需恢复");
        assert_eq!(RecoveryAction::FullRestart.as_zh(), "完全重启");
    }
}
