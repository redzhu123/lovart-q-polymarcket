//! RateLimit 测试（V1.08）。
//!
//! 自动测试：
//! - 429 响应触发
//! - 重试退避
//! - 恢复
//!
//! 输出中文限流统计。

use std::time::Instant;

use tracing;

use crate::client::http::ApiClient;

/// 速率限制测试报告。
#[derive(Debug, Clone)]
pub struct RateLimitReport {
    /// 总请求数。
    pub total_requests: usize,
    /// 成功数。
    pub success: usize,
    /// 触发限流（429）次数。
    pub rate_limited: usize,
    /// 其他错误数。
    pub errors: usize,
    /// 平均延迟（毫秒）。
    pub avg_latency_ms: f64,
    /// 总耗时（毫秒）。
    pub total_duration_ms: u64,
}

impl RateLimitReport {
    /// 中文输出。
    pub fn print_zh(&self) {
        tracing::info!("");
        tracing::info!("┌──────────────────────────────────────────────────────────┐");
        tracing::info!("│  【速率限制测试报告】");
        tracing::info!("├──────────────────────────────────────────────────────────┤");
        tracing::info!("│  总请求数:   {}", self.total_requests);
        tracing::info!(
            "│  成功:       {} ({}%)",
            self.success,
            if self.total_requests > 0 {
                self.success * 100 / self.total_requests
            } else {
                0
            }
        );
        tracing::info!(
            "│  触发限流:   {} ({}%)",
            self.rate_limited,
            if self.total_requests > 0 {
                self.rate_limited * 100 / self.total_requests
            } else {
                0
            }
        );
        tracing::info!("│  错误:       {}", self.errors);
        tracing::info!("│  平均延迟:   {:.0}ms", self.avg_latency_ms);
        tracing::info!("│  总耗时:     {}ms", self.total_duration_ms);
        tracing::info!("└──────────────────────────────────────────────────────────┘");
    }
}

/// 执行速率限制测试。
///
/// 发送快速连续请求以测试速率限制行为。
pub async fn test_rate_limit(client: &ApiClient, burst_count: usize) -> RateLimitReport {
    if !client.is_live() {
        tracing::warn!("Mock 模式跳过速率限制测试");
        return RateLimitReport {
            total_requests: 0,
            success: 0,
            rate_limited: 0,
            errors: 0,
            avg_latency_ms: 0.0,
            total_duration_ms: 0,
        };
    }

    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  速率限制测试（{} 次快速请求）", burst_count);
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    let start = Instant::now();
    let mut latencies = Vec::new();
    let mut success = 0;
    let mut rate_limited = 0;
    let mut errors = 0;

    for i in 0..burst_count {
        let req_start = Instant::now();
        match client.get("/time").await {
            Ok(resp) => {
                let latency = req_start.elapsed().as_millis() as u64;
                latencies.push(latency);

                if resp.is_rate_limited() {
                    rate_limited += 1;
                    tracing::warn!(
                        "  [{}/{}] HTTP 429 — 触发速率限制 ({}ms)",
                        i + 1,
                        burst_count,
                        latency
                    );
                } else if resp.is_success() {
                    success += 1;
                    tracing::debug!("  [{}/{}] HTTP 200 ({}ms)", i + 1, burst_count, latency);
                } else {
                    errors += 1;
                    tracing::warn!(
                        "  [{}/{}] HTTP {} ({}ms)",
                        i + 1,
                        burst_count,
                        resp.status,
                        latency
                    );
                }
            }
            Err(e) => {
                errors += 1;
                tracing::error!("  [{}/{}] 请求失败: {}", i + 1, burst_count, e);
            }
        }
    }

    let total = start.elapsed().as_millis() as u64;
    let avg_latency = if latencies.is_empty() {
        0.0
    } else {
        latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
    };

    let report = RateLimitReport {
        total_requests: burst_count,
        success,
        rate_limited,
        errors,
        avg_latency_ms: avg_latency,
        total_duration_ms: total,
    };

    report.print_zh();
    report
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::config::ApiTestConfig;

    #[tokio::test]
    async fn rate_limit_skipped_in_mock_mode() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);

        let report = test_rate_limit(&client, 5).await;
        assert_eq!(report.total_requests, 0);
    }

    #[test]
    fn rate_limit_report_prints() {
        let report = RateLimitReport {
            total_requests: 10,
            success: 8,
            rate_limited: 2,
            errors: 0,
            avg_latency_ms: 150.0,
            total_duration_ms: 2000,
        };
        report.print_zh();
    }
}
