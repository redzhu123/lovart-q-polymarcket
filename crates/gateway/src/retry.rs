//! Gateway Retry（V1.08 第七节）。
//!
//! 统一重试 / 退避 / 断路器。
//! Execution 禁止重复实现。

use std::fmt;
use std::time::{Duration, Instant};
use tracing;

use crate::config::GatewayConfig;

// ============================================================================
// Backoff（指数退避）
// ============================================================================

/// 指数退避策略。
#[derive(Debug, Clone)]
pub struct Backoff {
    /// 基础延迟（毫秒）。
    base_ms: u64,
    /// 最大延迟（毫秒）。
    max_ms: u64,
    /// 退避乘数。
    multiplier: f64,
    /// 当前重试次数。
    attempt: u32,
    /// 总已等待时间（毫秒）。
    total_waited_ms: u64,
}

impl Backoff {
    /// 创建新的退避策略。
    pub fn new(base_ms: u64, max_ms: u64, multiplier: f64) -> Self {
        Self {
            base_ms,
            max_ms,
            multiplier,
            attempt: 0,
            total_waited_ms: 0,
        }
    }

    /// 从 GatewayConfig 创建。
    pub fn from_config(cfg: &GatewayConfig) -> Self {
        Self::new(cfg.retry_base_ms, cfg.retry_max_ms, cfg.backoff_multiplier)
    }

    /// 计算下一次等待时间（毫秒）。
    pub fn next_delay_ms(&mut self) -> u64 {
        let delay = (self.base_ms as f64 * self.multiplier.powi(self.attempt as i32)) as u64;
        let delay = delay.min(self.max_ms);
        self.attempt += 1;
        self.total_waited_ms += delay;
        delay
    }

    /// 计算下一次等待的 Duration。
    pub fn next_delay(&mut self) -> Duration {
        Duration::from_millis(self.next_delay_ms())
    }

    /// 重置退避状态。
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.total_waited_ms = 0;
    }

    /// 当前重试次数。
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// 总已等待时间（毫秒）。
    pub fn total_waited_ms(&self) -> u64 {
        self.total_waited_ms
    }

    /// 是否已超过最大重试次数。
    pub fn exhausted(&self, max_retries: u32) -> bool {
        self.attempt >= max_retries
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(500, 15000, 2.0)
    }
}

// ============================================================================
// Circuit Breaker（断路器）
// ============================================================================

/// 断路器状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 关闭（正常操作）。
    Closed,
    /// 打开（拒绝所有请求）。
    Open,
    /// 半开（允许有限请求通过测试）。
    HalfOpen,
}

impl CircuitState {
    pub fn as_zh(&self) -> &'static str {
        match self {
            CircuitState::Closed => "关闭（正常）",
            CircuitState::Open => "打开（熔断）",
            CircuitState::HalfOpen => "半开（测试中）",
        }
    }
}

/// 断路器（Circuit Breaker）。
///
/// 当连续失败超过阈值时，断路器打开，拒绝所有请求。
/// 经过恢复超时后，进入半开状态，允许有限请求测试。
/// 测试成功则关闭，失败则重新打开。
#[derive(Debug)]
pub struct CircuitBreaker {
    /// 当前状态。
    state: CircuitState,
    /// 连续失败次数。
    failure_count: u32,
    /// 失败阈值（打开断路器）。
    failure_threshold: u32,
    /// 恢复超时（毫秒）。
    recovery_timeout_ms: u64,
    /// 半开状态最大请求数。
    half_open_max: u32,
    /// 半开状态已通过请求数。
    half_open_passed: u32,
    /// 断路器打开时间。
    opened_at: Option<Instant>,
    /// 最后一次失败时间。
    last_failure_at: Option<Instant>,
    /// 总成功次数。
    total_successes: u64,
    /// 总失败次数。
    total_failures: u64,
    /// 总熔断次数。
    total_trips: u64,
}

impl CircuitBreaker {
    /// 创建新的断路器。
    pub fn new(failure_threshold: u32, recovery_timeout_ms: u64, half_open_max: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            recovery_timeout_ms,
            half_open_max,
            half_open_passed: 0,
            opened_at: None,
            last_failure_at: None,
            total_successes: 0,
            total_failures: 0,
            total_trips: 0,
        }
    }

    /// 从 GatewayConfig 创建。
    pub fn from_config(cfg: &GatewayConfig) -> Self {
        Self::new(
            cfg.cb_failure_threshold,
            cfg.cb_recovery_timeout_ms,
            cfg.cb_half_open_max,
        )
    }

    /// 检查请求是否允许通过。
    ///
    /// 返回 `true` 表示允许，`false` 表示被断路器拦截。
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否到了恢复时间
                if let Some(opened) = self.opened_at {
                    let elapsed = opened.elapsed().as_millis() as u64;
                    if elapsed >= self.recovery_timeout_ms {
                        tracing::info!(
                            elapsed_ms = %elapsed,
                            "断路器进入半开状态，允许测试请求"
                        );
                        self.state = CircuitState::HalfOpen;
                        self.half_open_passed = 0;
                        return true;
                    }
                }
                tracing::warn!("断路器已打开，拒绝请求");
                false
            }
            CircuitState::HalfOpen => {
                if self.half_open_passed < self.half_open_max {
                    self.half_open_passed += 1;
                    true
                } else {
                    tracing::warn!("断路器半开状态已达最大测试数，等待其他测试完成");
                    false
                }
            }
        }
    }

    /// 记录成功。
    pub fn record_success(&mut self) {
        self.total_successes += 1;
        match self.state {
            CircuitState::HalfOpen => {
                // 半开状态成功 -> 关闭断路器
                tracing::info!("断路器测试成功，恢复关闭状态");
                self.state = CircuitState::Closed;
                self.failure_count = 0;
                self.half_open_passed = 0;
                self.opened_at = None;
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            _ => {}
        }
    }

    /// 记录失败。
    pub fn record_failure(&mut self) {
        self.total_failures += 1;
        self.last_failure_at = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.failure_threshold {
                    tracing::warn!(
                        failure_count = %self.failure_count,
                        threshold = %self.failure_threshold,
                        "断路器打开！连续失败超过阈值"
                    );
                    self.state = CircuitState::Open;
                    self.opened_at = Some(Instant::now());
                    self.total_trips += 1;
                }
            }
            CircuitState::HalfOpen => {
                // 半开状态失败 -> 重新打开
                tracing::warn!("断路器半开测试失败，重新打开");
                self.state = CircuitState::Open;
                self.opened_at = Some(Instant::now());
                self.total_trips += 1;
            }
            _ => {}
        }
    }

    /// 手动重置断路器。
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.half_open_passed = 0;
        self.opened_at = None;
    }

    /// 当前状态。
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// 统计信息（中文）。
    pub fn stats_zh(&self) -> String {
        format!(
            "断路器状态: {} | 失败计数: {} | 总成功: {} | 总失败: {} | 熔断次数: {}",
            self.state.as_zh(),
            self.failure_count,
            self.total_successes,
            self.total_failures,
            self.total_trips,
        )
    }

    /// 中文摘要（简短）。
    pub fn summary_zh(&self) -> String {
        match self.state {
            CircuitState::Closed => "断路器：关闭 ✅".to_string(),
            CircuitState::Open => {
                let remaining = if let Some(opened) = self.opened_at {
                    let elapsed = opened.elapsed().as_millis() as u64;
                    if elapsed < self.recovery_timeout_ms {
                        format!("（剩余 {}s）", (self.recovery_timeout_ms - elapsed) / 1000)
                    } else {
                        "（可恢复）".to_string()
                    }
                } else {
                    String::new()
                };
                format!("断路器：打开 ❌ {}", remaining)
            }
            CircuitState::HalfOpen => "断路器：半开 ⚠️".to_string(),
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 30000, 3)
    }
}

// ============================================================================
// RetryExecutor（重试执行器）
// ============================================================================

/// 重试执行器：组合 Backoff + CircuitBreaker，自动重试。
pub struct RetryExecutor {
    /// 退避策略。
    backoff: Backoff,
    /// 断路器。
    breaker: CircuitBreaker,
    /// 最大重试次数。
    max_retries: u32,
}

impl RetryExecutor {
    /// 创建新的重试执行器。
    pub fn new(backoff: Backoff, breaker: CircuitBreaker, max_retries: u32) -> Self {
        Self {
            backoff,
            breaker,
            max_retries,
        }
    }

    /// 从 GatewayConfig 创建。
    pub fn from_config(cfg: &GatewayConfig) -> Self {
        Self::new(
            Backoff::from_config(cfg),
            CircuitBreaker::from_config(cfg),
            cfg.max_retries,
        )
    }

    /// 执行带重试的异步操作。
    ///
    /// `operation` 是异步闭包，返回 `Result<T, E>`，`E` 实现 `fmt::Display`。
    /// 自动处理断路器检查、重试、退避。
    pub async fn execute<F, Fut, T, E>(
        &mut self,
        operation_name: &str,
        mut operation: F,
    ) -> Result<T, RetryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: fmt::Display,
    {
        loop {
            // 检查断路器
            if !self.breaker.allow_request() {
                return Err(RetryError::CircuitOpen {
                    operation: operation_name.to_string(),
                    attempt: self.backoff.attempt,
                });
            }

            match operation().await {
                Ok(value) => {
                    self.breaker.record_success();
                    self.backoff.reset();
                    return Ok(value);
                }
                Err(e) => {
                    self.breaker.record_failure();

                    if self.backoff.exhausted(self.max_retries) {
                        tracing::error!(
                            operation = %operation_name,
                            attempts = %self.backoff.attempt,
                            error = %e,
                            "操作重试耗尽"
                        );
                        return Err(RetryError::Exhausted {
                            operation: operation_name.to_string(),
                            attempts: self.backoff.attempt,
                            last_error: e.to_string(),
                        });
                    }

                    let delay = self.backoff.next_delay();
                    tracing::warn!(
                        operation = %operation_name,
                        attempt = %self.backoff.attempt,
                        delay_ms = %delay.as_millis(),
                        error = %e,
                        "操作失败，等待重试"
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// 获取断路器引用。
    pub fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// 获取断路器可变引用。
    pub fn breaker_mut(&mut self) -> &mut CircuitBreaker {
        &mut self.breaker
    }

    /// 获取退避引用。
    pub fn backoff(&self) -> &Backoff {
        &self.backoff
    }

    /// 重置所有状态。
    pub fn reset(&mut self) {
        self.backoff.reset();
        self.breaker.reset();
    }
}

impl Default for RetryExecutor {
    fn default() -> Self {
        Self::from_config(&GatewayConfig::default())
    }
}

// ============================================================================
// RetryError
// ============================================================================

/// 重试错误。
#[derive(Debug, thiserror::Error)]
pub enum RetryError {
    /// 重试耗尽。
    #[error("操作 '{operation}' 重试耗尽（{attempts} 次），最后错误: {last_error}")]
    Exhausted {
        operation: String,
        attempts: u32,
        last_error: String,
    },

    /// 断路器打开。
    #[error("操作 '{operation}' 被断路器拦截（第 {attempt} 次尝试）")]
    CircuitOpen {
        operation: String,
        attempt: u32,
    },
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn backoff_exponential_growth() {
        let mut b = Backoff::new(100, 10000, 2.0);
        let d0 = b.next_delay_ms();
        let d1 = b.next_delay_ms();
        let d2 = b.next_delay_ms();
        let d3 = b.next_delay_ms();

        assert_eq!(d0, 100);  // 100 * 2^0
        assert_eq!(d1, 200);  // 100 * 2^1
        assert_eq!(d2, 400);  // 100 * 2^2
        assert_eq!(d3, 800);  // 100 * 2^3
    }

    #[test]
    fn backoff_respects_max() {
        let mut b = Backoff::new(1000, 2000, 2.0);
        let d0 = b.next_delay_ms();
        let d1 = b.next_delay_ms();
        let d2 = b.next_delay_ms();

        assert_eq!(d0, 1000);
        assert_eq!(d1, 2000);
        assert_eq!(d2, 2000); // capped at max
    }

    #[test]
    fn backoff_exhausted() {
        let mut b = Backoff::new(100, 1000, 2.0);
        assert!(!b.exhausted(3));
        b.next_delay_ms(); // attempt 0
        assert!(!b.exhausted(3));
        b.next_delay_ms(); // attempt 1
        assert!(!b.exhausted(3));
        b.next_delay_ms(); // attempt 2
        assert!(b.exhausted(3)); // attempt >= 3
    }

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 1000, 2);
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert_eq!(cb.total_trips, 1);
    }

    #[test]
    fn circuit_breaker_blocks_when_open() {
        let mut cb = CircuitBreaker::new(2, 10000, 2);

        // 触发打开
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn circuit_breaker_success_resets_count() {
        let mut cb = CircuitBreaker::new(3, 1000, 2);
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        // 失败计数已重置
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_breaker_reset() {
        let mut cb = CircuitBreaker::new(2, 1000, 2);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn retry_executor_success_first_try() {
        let mut executor = RetryExecutor::default();

        let result = executor
            .execute("测试操作", || async { Ok::<_, &str>("成功") })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "成功");
    }

    #[tokio::test]
    async fn retry_executor_retries_on_failure() {
        let mut executor = RetryExecutor::new(
            Backoff::new(10, 100, 2.0),
            CircuitBreaker::new(10, 1000, 2),
            3,
        );

        let attempts = AtomicU32::new(0);

        let result = executor
            .execute("重试测试", || async {
                let a = attempts.fetch_add(1, Ordering::SeqCst);
                if a < 2 {
                    Err("模拟失败")
                } else {
                    Ok("最终成功")
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 失败 2 次 + 成功 1 次
    }

    #[tokio::test]
    async fn retry_executor_exhausted() {
        let mut executor = RetryExecutor::new(
            Backoff::new(10, 100, 2.0),
            CircuitBreaker::new(10, 1000, 2),
            2,
        );

        let result = executor
            .execute("耗尽测试", || async { Err::<(), _>("永远失败") })
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn circuit_breaker_stats_zh() {
        let mut cb = CircuitBreaker::new(3, 1000, 2);
        cb.record_failure();
        let stats = cb.stats_zh();
        assert!(stats.contains("关闭"));
        assert!(stats.contains("失败计数: 1"));
    }
}
