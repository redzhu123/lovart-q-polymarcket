//! 速率限制测试（P2-03）。
//!
//! 验证 TokenBucket 和 RateLimiter 的行为。

use pm_gateway::ratelimit::{RateLimiter, TokenBucket};
use std::sync::Arc;

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}

// ============================================================================
// TokenBucket 测试
// ============================================================================

#[test]
fn token_bucket_first_acquire_returns_zero() {
    init_logging();
    let tb = TokenBucket::new(10, 1000);
    assert_eq!(tb.acquire(), 0);
}

#[test]
fn token_bucket_exhaustion_returns_wait() {
    init_logging();
    let tb = TokenBucket::new(2, 1000);
    assert_eq!(tb.acquire(), 0);
    assert_eq!(tb.acquire(), 0);

    let wait = tb.acquire();
    assert!(wait > 0);
}

#[test]
fn token_bucket_remaining_decreases() {
    init_logging();
    let tb = TokenBucket::new(10, 1000);
    let r0 = tb.remaining();
    assert!((r0 - 1.0).abs() < 0.01);

    tb.acquire();
    let r1 = tb.remaining();
    assert!(r1 < r0);
    assert!(r1 > 0.0);
}

#[test]
fn token_bucket_reset_restores_full() {
    init_logging();
    let tb = TokenBucket::new(3, 1000);
    tb.acquire();
    tb.acquire();
    assert!(tb.remaining() < 1.0);

    tb.reset();
    assert!((tb.remaining() - 1.0).abs() < 0.01);
}

// ============================================================================
// RateLimiter 测试
// ============================================================================

#[test]
fn rate_limiter_combines_both_windows() {
    init_logging();
    let rl = RateLimiter::new(100, 1000);
    // 两个窗口都充裕，应该立即获得
    assert_eq!(rl.acquire(), 0);
}

#[test]
fn rate_limiter_remaining_returns_min() {
    init_logging();
    let rl = RateLimiter::new(100, 50);
    // per_second 和 per_minute 的剩余比例的最小值
    let r = rl.remaining();
    assert!(r >= 0.0 && r <= 1.0);
}

#[test]
fn rate_limiter_stats() {
    init_logging();
    let rl = RateLimiter::new(10, 300);
    rl.acquire();
    rl.acquire();

    let stats = rl.stats();
    assert_eq!(stats.acquisitions, 2);
    assert!(stats.summary_zh().contains("每秒限制"));
    assert!(stats.summary_zh().contains("每分钟限制"));
}

#[test]
fn rate_limiter_reset() {
    init_logging();
    let rl = RateLimiter::new(10, 300);
    rl.acquire();
    rl.acquire();

    rl.reset();
    let stats = rl.stats();
    assert_eq!(stats.acquisitions, 0);
}

// ============================================================================
// 并发测试
// ============================================================================

#[tokio::test]
async fn rate_limiter_concurrent_access() {
    init_logging();
    let rl = Arc::new(RateLimiter::new(100, 1000));

    let mut handles = vec![];
    for _ in 0..10 {
        let rl = rl.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..5 {
                rl.acquire();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let stats = rl.stats();
    assert_eq!(stats.acquisitions, 50);
}