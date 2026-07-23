//! Execution Scheduler（V1.06 第七节）。
//!
//! 控制订单发送节奏，支持 Rate Limit。
//! 使用令牌桶算法（token bucket）实现平滑速率控制。
//!
//! 未来：Provider 可配置不同速率限制。

use std::time::Instant;

// ============================================================================
// Scheduler Configuration
// ============================================================================

/// 调度器配置（从 config.toml [execution] 段读取，禁止写死）。
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 每秒最大订单数。
    pub max_orders_per_second: u32,
    /// 每分钟最大订单数（0 = 不限）。
    pub max_orders_per_minute: u32,
    /// 突发容量（一次可发送的最大 burst 数）。
    pub burst_size: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_orders_per_second: 10,
            max_orders_per_minute: 0,
            burst_size: 5,
        }
    }
}

// ============================================================================
// Token Bucket Rate Limiter
// ============================================================================

/// 令牌桶速率限制器。
struct RateLimiter {
    /// 每秒生成令牌数。
    rate: f64,
    /// 当前令牌数。
    tokens: f64,
    /// 最大令牌数（burst 容量）。
    max_tokens: f64,
    /// 上次更新时间。
    last_update: Instant,
}

impl RateLimiter {
    fn new(rate_per_second: u32, burst: u32) -> Self {
        let max_tokens = burst.max(1) as f64;
        Self {
            rate: rate_per_second as f64,
            tokens: max_tokens,
            max_tokens,
            last_update: Instant::now(),
        }
    }

    /// 尝试获取一个令牌。成功返回 true。
    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        // 补充令牌
        self.tokens = (self.tokens + elapsed * self.rate).min(self.max_tokens);
        self.last_update = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 到下一个令牌可用还需等待的毫秒数。
    fn wait_ms(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let needed = 1.0 - self.tokens;
        let wait_secs = needed / self.rate;
        (wait_secs * 1000.0).ceil() as u64
    }

    /// 动态调整速率。
    fn set_rate(&mut self, rate_per_second: u32) {
        self.rate = rate_per_second as f64;
    }
}

// ============================================================================
// Execution Scheduler
// ============================================================================

/// 执行调度器（V1.06 第七节）。
///
/// 控制订单发送频率，确保不超过配置的速率限制。
/// 使用令牌桶实现平滑限流。
pub struct ExecutionScheduler {
    /// 令牌桶速率限制器。
    limiter: RateLimiter,
    /// 调度器配置。
    config: SchedulerConfig,
    /// 总发送计数。
    total_sent: u64,
    /// 总等待计数。
    total_waited: u64,
}

impl ExecutionScheduler {
    /// 创建新的调度器。
    pub fn new(config: SchedulerConfig) -> Self {
        let rate = config.max_orders_per_second;
        let burst = config.burst_size;
        Self {
            limiter: RateLimiter::new(rate, burst),
            config,
            total_sent: 0,
            total_waited: 0,
        }
    }

    /// 使用默认配置创建。
    pub fn with_defaults() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// 尝试获取发送许可（非阻塞）。
    ///
    /// 返回 true 表示可以立即发送。
    /// 返回 false 表示需要等待。
    pub fn try_acquire(&mut self) -> bool {
        if self.limiter.try_acquire() {
            self.total_sent += 1;
            true
        } else {
            false
        }
    }

    /// 获取发送许可（阻塞，直到有可用令牌）。
    ///
    /// 内部使用 tokio::time::sleep 实现异步等待。
    pub async fn acquire(&mut self) {
        while !self.try_acquire() {
            let wait_ms = self.limiter.wait_ms().max(10);
            self.total_waited += 1;
            tracing::debug!(
                wait_ms = %wait_ms,
                total_waited = %self.total_waited,
                "速率限制等待"
            );
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        }
    }

    /// 动态设置速率。
    pub fn set_rate(&mut self, per_second: u32) {
        self.config.max_orders_per_second = per_second;
        self.limiter.set_rate(per_second);
        tracing::info!(rate = %per_second, "速率限制已更新");
    }

    /// 获取当前配置。
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    /// 获取统计。
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            total_sent: self.total_sent,
            total_waited: self.total_waited,
            rate_limit: self.config.max_orders_per_second,
        }
    }

    /// 打印状态（中文）。
    pub fn print_status(&self) {
        let s = self.stats();
        println!("【调度器】");
        println!();
        println!("  速率限制 : {} 单/秒", s.rate_limit);
        println!("  已发送   : {}", s.total_sent);
        println!("  等待次数 : {}", s.total_waited);
    }
}

/// 调度器统计。
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub total_sent: u64,
    pub total_waited: u64,
    pub rate_limit: u32,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_acquire_works_within_burst() {
        let mut s = ExecutionScheduler::new(SchedulerConfig {
            max_orders_per_second: 100,
            burst_size: 10,
            ..SchedulerConfig::default()
        });
        // 突发容量内应全部通过
        for _ in 0..10 {
            assert!(s.try_acquire());
        }
    }

    #[test]
    fn rate_limit_exhausted_after_burst() {
        let mut s = ExecutionScheduler::new(SchedulerConfig {
            max_orders_per_second: 1,
            burst_size: 1,
            ..SchedulerConfig::default()
        });
        // 第一个通过
        assert!(s.try_acquire());
        // 第二个被限制
        assert!(!s.try_acquire());
    }

    #[test]
    fn set_rate_updates_limiter() {
        let mut s = ExecutionScheduler::new(SchedulerConfig {
            max_orders_per_second: 1,
            burst_size: 1,
            ..SchedulerConfig::default()
        });
        assert!(s.try_acquire());
        assert!(!s.try_acquire());

        // 调高速率并等待一小段时间让令牌桶补充
        s.set_rate(1000);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(s.try_acquire());
    }

    #[tokio::test]
    async fn acquire_blocks_and_returns() {
        let mut s = ExecutionScheduler::new(SchedulerConfig {
            max_orders_per_second: 1000,
            burst_size: 10,
            ..SchedulerConfig::default()
        });
        // 高限速下 acquire 应快速返回
        s.acquire().await;
        assert!(s.stats().total_sent >= 1);
    }

    #[test]
    fn default_scheduler_config() {
        let c = SchedulerConfig::default();
        assert_eq!(c.max_orders_per_second, 10);
        assert_eq!(c.burst_size, 5);
    }
}
