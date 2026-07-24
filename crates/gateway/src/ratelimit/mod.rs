//! Gateway 速率限制模块（P2-03）。
//!
//! 实现 Token Bucket 算法，支持每秒和每分钟两种速率限制。
//! 业务层通过 Middleware 自动调用，不直接使用此模块。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ============================================================================
// TokenBucket
// ============================================================================

/// Token Bucket 速率限制器（单窗口）。
///
/// 使用原子操作实现无锁 Token Bucket 算法。
/// 线程安全，可在多线程环境中使用。
pub struct TokenBucket {
    /// 每秒/每分钟填充的 token 数。
    rate: u32,
    /// 当前可用 token 数。
    tokens: AtomicU32,
    /// 上次填充时间（毫秒时间戳）。
    last_refill: AtomicU64,
    /// 窗口大小（毫秒）。
    window_ms: u64,
}

impl TokenBucket {
    /// 创建新的 Token Bucket。
    ///
    /// - `rate`: 窗口内允许的最大请求数。
    /// - `window_ms`: 窗口大小（毫秒），如 1000 表示每秒，60000 表示每分钟。
    pub fn new(rate: u32, window_ms: u64) -> Self {
        Self {
            rate,
            tokens: AtomicU32::new(rate),
            last_refill: AtomicU64::new(Self::now_ms()),
            window_ms,
        }
    }

    /// 当前毫秒时间戳。
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// 获取一个 token。
    ///
    /// 返回需要等待的毫秒数（0 表示立即获得）。
    pub fn acquire(&self) -> u64 {
        let now = Self::now_ms();
        let last = self.last_refill.load(Ordering::Relaxed);

        // 计算需要补充的 token 数
        let elapsed_ms = now.saturating_sub(last);
        let refill_rate = self.rate as f64 / self.window_ms as f64;
        let new_tokens = (elapsed_ms as f64 * refill_rate) as u32;

        if new_tokens > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let refilled = (current + new_tokens).min(self.rate);
            self.tokens.store(refilled, Ordering::Relaxed);
            self.last_refill.store(now, Ordering::Relaxed);
        }

        // 尝试消费一个 token（CAS 循环）
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current > 0 {
                if self
                    .tokens
                    .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return 0; // 获取成功
                }
                // CAS 失败，重试
            } else {
                // 计算需要等待的时间
                let wait_ms = (self.window_ms as f64 / self.rate as f64).ceil() as u64;
                return wait_ms;
            }
        }
    }

    /// 剩余 token 比例（0.0 ~ 1.0）。
    pub fn remaining(&self) -> f64 {
        let tokens = self.tokens.load(Ordering::Relaxed) as f64;
        (tokens / self.rate as f64).clamp(0.0, 1.0)
    }

    /// 重置 Token Bucket。
    pub fn reset(&self) {
        self.tokens.store(self.rate, Ordering::Relaxed);
        self.last_refill.store(Self::now_ms(), Ordering::Relaxed);
    }
}

// ============================================================================
// RateLimiter
// ============================================================================

/// 组合速率限制器（每秒 + 每分钟）。
///
/// 同时检查两个 Token Bucket，任一不足则返回需要等待的时间。
pub struct RateLimiter {
    /// 每秒限制。
    per_second: TokenBucket,
    /// 每分钟限制。
    per_minute: TokenBucket,
    /// 累计获取次数。
    total_acquisitions: AtomicU64,
    /// 累计等待次数。
    total_waits: AtomicU64,
    /// 累计拒绝次数。
    total_rejections: AtomicU64,
}

impl RateLimiter {
    /// 创建新的速率限制器。
    pub fn new(per_second_rate: u32, per_minute_rate: u32) -> Self {
        Self {
            per_second: TokenBucket::new(per_second_rate, 1000),
            per_minute: TokenBucket::new(per_minute_rate, 60000),
            total_acquisitions: AtomicU64::new(0),
            total_waits: AtomicU64::new(0),
            total_rejections: AtomicU64::new(0),
        }
    }

    /// 获取一个 token。
    ///
    /// 返回需要等待的毫秒数。0 表示立即获得。
    /// 同时检查每秒和每分钟限制，取较大者的等待时间。
    pub fn acquire(&self) -> u64 {
        self.total_acquisitions.fetch_add(1, Ordering::Relaxed);

        let wait_sec = self.per_second.acquire();
        let wait_min = self.per_minute.acquire();

        let wait_ms = wait_sec.max(wait_min);

        if wait_ms > 0 {
            self.total_waits.fetch_add(1, Ordering::Relaxed);
            if wait_ms > 5000 {
                self.total_rejections.fetch_add(1, Ordering::Relaxed);
            }
        }

        wait_ms
    }

    /// 剩余速率比例（取两个 bucket 的最小值，0.0 ~ 1.0）。
    pub fn remaining(&self) -> f64 {
        let sec = self.per_second.remaining();
        let min = self.per_minute.remaining();
        sec.min(min)
    }

    /// 统计信息。
    pub fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            acquisitions: self.total_acquisitions.load(Ordering::Relaxed),
            waits: self.total_waits.load(Ordering::Relaxed),
            rejections: self.total_rejections.load(Ordering::Relaxed),
            remaining_percent: self.remaining() * 100.0,
            per_second_rate: self.per_second.rate,
            per_minute_rate: self.per_minute.rate,
        }
    }

    /// 重置所有计数器。
    pub fn reset(&self) {
        self.per_second.reset();
        self.per_minute.reset();
        self.total_acquisitions.store(0, Ordering::Relaxed);
        self.total_waits.store(0, Ordering::Relaxed);
        self.total_rejections.store(0, Ordering::Relaxed);
    }
}

/// 速率限制统计。
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    /// 累计获取次数。
    pub acquisitions: u64,
    /// 累计等待次数。
    pub waits: u64,
    /// 累计拒绝次数。
    pub rejections: u64,
    /// 剩余比例（%）。
    pub remaining_percent: f64,
    /// 每秒限制。
    pub per_second_rate: u32,
    /// 每分钟限制。
    pub per_minute_rate: u32,
}

impl RateLimitStats {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "每秒限制: {} | 每分钟限制: {} | 累计获取: {} | 等待: {} | 拒绝: {} | 剩余: {:.0}%",
            self.per_second_rate,
            self.per_minute_rate,
            self.acquisitions,
            self.waits,
            self.rejections,
            self.remaining_percent,
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
    fn token_bucket_initial_acquire_succeeds() {
        let tb = TokenBucket::new(10, 1000);
        let wait = tb.acquire();
        assert_eq!(wait, 0);
    }

    #[test]
    fn token_bucket_exhaustion() {
        let tb = TokenBucket::new(2, 1000);
        assert_eq!(tb.acquire(), 0);
        assert_eq!(tb.acquire(), 0);
        // 第三个 token 需要等待
        let wait = tb.acquire();
        assert!(wait > 0);
    }

    #[test]
    fn token_bucket_remaining() {
        let tb = TokenBucket::new(10, 1000);
        assert!((tb.remaining() - 1.0).abs() < 0.01);
        tb.acquire();
        assert!((tb.remaining() - 0.9).abs() < 0.01);
    }

    #[test]
    fn token_bucket_reset() {
        let tb = TokenBucket::new(3, 1000);
        tb.acquire();
        tb.acquire();
        assert!((tb.remaining() - 1.0 / 3.0).abs() < 0.01);
        tb.reset();
        assert!((tb.remaining() - 1.0).abs() < 0.01);
    }

    #[test]
    fn rate_limiter_dual_window() {
        let rl = RateLimiter::new(10, 300);
        // 每秒和每分钟都应该通过
        let wait = rl.acquire();
        assert_eq!(wait, 0);
    }

    #[test]
    fn rate_limiter_remaining() {
        let rl = RateLimiter::new(100, 1000);
        assert!((rl.remaining() - 1.0).abs() < 0.01);
    }

    #[test]
    fn rate_limiter_stats() {
        let rl = RateLimiter::new(10, 300);
        rl.acquire();
        let stats = rl.stats();
        assert_eq!(stats.acquisitions, 1);
        assert!(stats.summary_zh().contains("每秒限制"));
        assert!(stats.summary_zh().contains("每分钟限制"));
    }

    #[test]
    fn rate_limiter_reset() {
        let rl = RateLimiter::new(10, 300);
        rl.acquire();
        rl.acquire();
        let stats = rl.stats();
        assert_eq!(stats.acquisitions, 2);
        rl.reset();
        let stats = rl.stats();
        assert_eq!(stats.acquisitions, 0);
    }
}