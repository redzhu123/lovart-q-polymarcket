//! 熔断器与重试执行器。
//!
//! 从 `pm-gateway::retry` 提取并统一。

use super::backoff::Backoff;
use std::fmt;
use std::future::Future;
use std::time::{Duration, Instant};

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 闭合：正常状态，请求通过
    Closed,
    /// 断开：熔断激活，请求被拒绝
    Open,
    /// 半开：探测性恢复，部分请求通过
    HalfOpen,
}

impl CircuitState {
    pub fn as_zh(&self) -> &'static str {
        match self {
            CircuitState::Closed => "闭合",
            CircuitState::Open => "断开",
            CircuitState::HalfOpen => "半开",
        }
    }
}

/// 熔断器
///
/// 跟踪失败次数，当失败超过阈值时断开电路。
/// 断开后经过恢复超时进入半开状态，探测成功则闭合，失败则重新断开。
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    failure_threshold: u32,
    recovery_timeout_ms: u64,
    half_open_max: u32,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
    total_trips: u64,
}

impl CircuitBreaker {
    /// 创建新的熔断器
    ///
    /// # 参数
    /// - `failure_threshold`：连续失败多少次后断开
    /// - `recovery_timeout_ms`：断开后多久进入半开状态
    /// - `half_open_max`：半开状态最多允许几次成功探测
    pub fn new(failure_threshold: u32, recovery_timeout_ms: u64, half_open_max: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold,
            recovery_timeout_ms,
            half_open_max,
            last_failure_time: None,
            last_state_change: Instant::now(),
            total_trips: 0,
        }
    }

    /// 检查是否允许请求通过
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否到了恢复超时
                if let Some(last_fail) = self.last_failure_time {
                    let elapsed = last_fail.elapsed().as_millis() as u64;
                    if elapsed >= self.recovery_timeout_ms {
                        tracing::info!("熔断器进入半开状态，开始探测性恢复");
                        self.state = CircuitState::HalfOpen;
                        self.success_count = 0;
                        self.last_state_change = Instant::now();
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => {
                // 半开状态允许有限请求
                self.success_count < self.half_open_max
            }
        }
    }

    /// 记录一次成功
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.half_open_max {
                    tracing::info!("熔断器恢复：半开探测全部成功，闭合电路");
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.last_state_change = Instant::now();
                }
            }
            CircuitState::Open => {
                // 开状态不应该有成功记录，忽略
            }
        }
    }

    /// 记录一次失败
    pub fn record_failure(&mut self) {
        self.last_failure_time = Some(Instant::now());
        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.failure_threshold {
                    tracing::warn!(
                        "熔断器断开：连续失败 {} 次达到阈值 {}",
                        self.failure_count,
                        self.failure_threshold
                    );
                    self.state = CircuitState::Open;
                    self.total_trips += 1;
                    self.last_state_change = Instant::now();
                }
            }
            CircuitState::HalfOpen => {
                tracing::warn!("熔断器半开探测失败，重新断开电路");
                self.state = CircuitState::Open;
                self.failure_count = self.failure_threshold;
                self.total_trips += 1;
                self.last_state_change = Instant::now();
            }
            CircuitState::Open => {
                // 保持断开
            }
        }
    }

    /// 重置熔断器
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_failure_time = None;
        self.last_state_change = Instant::now();
    }

    /// 当前状态
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// 总跳闸次数
    pub fn total_trips(&self) -> u64 {
        self.total_trips
    }

    /// 中文状态信息
    pub fn stats_zh(&self) -> String {
        format!(
            "熔断器: 状态={}, 失败次数={}, 总跳闸={}",
            self.state.as_zh(),
            self.failure_count,
            self.total_trips,
        )
    }
}

/// 重试错误
#[derive(Debug, thiserror::Error)]
pub enum RetryError {
    /// 重试耗尽
    #[error("重试耗尽: 操作={operation}, 尝试={attempts}次, 最后错误: {last_error}")]
    Exhausted {
        operation: String,
        attempts: u32,
        last_error: String,
    },
    /// 熔断器断开
    #[error("熔断器断开: 操作={operation}, 尝试={attempt}")]
    CircuitOpen { operation: String, attempt: u32 },
}

/// 重试执行器：组合退避策略和熔断器
pub struct RetryExecutor {
    backoff: Backoff,
    breaker: CircuitBreaker,
    max_retries: u32,
}

impl RetryExecutor {
    /// 创建新的重试执行器
    pub fn new(backoff: Backoff, breaker: CircuitBreaker, max_retries: u32) -> Self {
        Self {
            backoff,
            breaker,
            max_retries,
        }
    }

    /// 使用默认配置创建
    pub fn default_with(max_retries: u32) -> Self {
        Self::new(
            Backoff::default(),
            CircuitBreaker::new(5, 30_000, 3),
            max_retries,
        )
    }

    /// 执行带重试的操作
    ///
    /// 操作成功则返回 Ok，如果熔断器断开或重试耗尽则返回 Err。
    pub async fn execute<F, Fut, T, E>(&mut self, name: &str, mut op: F) -> Result<T, RetryError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: fmt::Display,
    {
        for attempt in 0..self.max_retries {
            // 检查熔断器
            if !self.breaker.allow_request() {
                return Err(RetryError::CircuitOpen {
                    operation: name.to_string(),
                    attempt,
                });
            }

            match op().await {
                Ok(result) => {
                    self.breaker.record_success();
                    tracing::debug!("操作成功: {} (尝试 {})", name, attempt + 1);
                    return Ok(result);
                }
                Err(e) => {
                    self.breaker.record_failure();
                    tracing::warn!(
                        "操作失败: {} (尝试 {}/{}): {}",
                        name,
                        attempt + 1,
                        self.max_retries,
                        e
                    );

                    if attempt + 1 < self.max_retries {
                        let delay = self.backoff.next_delay_ms();
                        tracing::debug!("等待 {}ms 后重试", delay);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    } else {
                        return Err(RetryError::Exhausted {
                            operation: name.to_string(),
                            attempts: self.max_retries,
                            last_error: e.to_string(),
                        });
                    }
                }
            }
        }

        Err(RetryError::Exhausted {
            operation: name.to_string(),
            attempts: self.max_retries,
            last_error: "未知错误".to_string(),
        })
    }

    /// 获取熔断器状态
    pub fn circuit_state(&self) -> CircuitState {
        self.breaker.state()
    }

    /// 重置所有状态
    pub fn reset(&mut self) {
        self.backoff.reset();
        self.breaker.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, 1000, 2);
        assert!(cb.allow_request());
        cb.record_failure();
        assert!(cb.allow_request());
        cb.record_failure();
        assert!(cb.allow_request());
        cb.record_failure();
        // 第三次失败后应该断开
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn circuit_breaker_recovers() {
        let mut cb = CircuitBreaker::new(2, 10, 1);
        // 触发断开
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        // 等待恢复超时
        std::thread::sleep(Duration::from_millis(15));
        assert!(cb.allow_request());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_breaker_reset() {
        let mut cb = CircuitBreaker::new(2, 1000, 1);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.total_trips(), 1); // trips 不重置
    }

    #[test]
    fn circuit_breaker_stats_zh() {
        let cb = CircuitBreaker::new(5, 30000, 3);
        let stats = cb.stats_zh();
        assert!(stats.contains("闭合"));
        assert!(stats.contains("熔断器"));
    }

    #[tokio::test]
    async fn retry_executor_success_on_first_try() {
        let mut executor = RetryExecutor::default_with(3);
        let result = executor
            .execute("test-op", || async { Ok::<_, &str>(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_executor_exhausted() {
        let mut executor = RetryExecutor::default_with(3);
        let result = executor
            .execute("test-op", || async { Err::<i32, _>("persistent failure") })
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RetryError::Exhausted {
                operation,
                attempts,
                ..
            } => {
                assert_eq!(operation, "test-op");
                assert_eq!(attempts, 3);
            }
            _ => panic!("应为 Exhausted 错误"),
        }
    }

    #[test]
    fn circuit_state_zh_names() {
        assert_eq!(CircuitState::Closed.as_zh(), "闭合");
        assert_eq!(CircuitState::Open.as_zh(), "断开");
        assert_eq!(CircuitState::HalfOpen.as_zh(), "半开");
    }
}
