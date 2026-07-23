//! Gateway Health Check（V1.08 第六节）。
//!
//! 健康检查：连接状态 / API 延迟 / WebSocket / Rate Limit。
//! 全部中文。

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::traits::ExchangeGateway;

// ============================================================================
// HealthChecker
// ============================================================================

/// 健康检查器。
pub struct HealthChecker {
    /// 上次健康检查时间。
    last_check: Option<DateTime<Local>>,
    /// 健康检查间隔（秒）。
    check_interval_secs: u64,
    /// 累计检查次数。
    total_checks: u64,
    /// 健康次数。
    healthy_checks: u64,
    /// 不健康次数。
    unhealthy_checks: u64,
}

impl HealthChecker {
    /// 创建新的健康检查器。
    pub fn new(check_interval_secs: u64) -> Self {
        Self {
            last_check: None,
            check_interval_secs,
            total_checks: 0,
            healthy_checks: 0,
            unhealthy_checks: 0,
        }
    }

    /// 是否需要检查。
    pub fn needs_check(&self, now: DateTime<Local>) -> bool {
        match self.last_check {
            None => true,
            Some(last) => {
                let elapsed = (now - last).num_seconds();
                elapsed >= self.check_interval_secs as i64
            }
        }
    }

    /// 执行健康检查。
    pub async fn check(&mut self, gateway: &dyn ExchangeGateway, now: DateTime<Local>) -> HealthReport {
        self.total_checks += 1;

        // 1. Ping
        let ping_start = std::time::Instant::now();
        let ping_ok = gateway.ping().await;
        let ping_ms = ping_start.elapsed().as_millis() as u64;

        // 2. 完整健康检查
        let info = gateway.health().await;

        // 3. 余额检查（快速验证 API 可达）
        let balance_ok = gateway.get_balance().await.is_ok();

        let healthy = ping_ok && info.healthy && balance_ok;

        if healthy {
            self.healthy_checks += 1;
        } else {
            self.unhealthy_checks += 1;
        }

        self.last_check = Some(now);

        HealthReport {
            timestamp: now,
            healthy,
            gateway_name: gateway.name().to_string(),
            gateway_type: gateway.gateway_type().to_string(),
            live_enabled: gateway.live_enabled(),
            ping_ok,
            ping_latency_ms: ping_ms,
            api_latency_ms: info.api_latency_ms,
            http_success_rate: info.http_success_rate,
            ws_connected: info.ws_connected,
            rate_limit_remaining: info.rate_limit_remaining,
            balance_ok,
            connection_status: info.connection_status,
            total_checks: self.total_checks,
            healthy_checks: self.healthy_checks,
            unhealthy_checks: self.unhealthy_checks,
        }
    }

    /// 获取健康状态摘要。
    pub fn status_zh(&self) -> String {
        format!(
            "健康检查: {} 次 | ✅ {} | ❌ {}",
            self.total_checks, self.healthy_checks, self.unhealthy_checks,
        )
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(30)
    }
}

// ============================================================================
// HealthReport
// ============================================================================

/// 健康检查报告。
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// 时间戳。
    pub timestamp: DateTime<Local>,
    /// 是否健康。
    pub healthy: bool,
    /// Gateway 名称。
    pub gateway_name: String,
    /// Gateway 类型。
    pub gateway_type: String,
    /// 是否启用真实交易。
    pub live_enabled: bool,
    /// Ping 是否成功。
    pub ping_ok: bool,
    /// Ping 延迟（毫秒）。
    pub ping_latency_ms: u64,
    /// API 延迟（毫秒）。
    pub api_latency_ms: u64,
    /// HTTP 成功率。
    pub http_success_rate: f64,
    /// WebSocket 是否连接。
    pub ws_connected: bool,
    /// Rate Limit 剩余比例。
    pub rate_limit_remaining: f64,
    /// 余额检查是否通过。
    pub balance_ok: bool,
    /// 连接状态描述（中文）。
    pub connection_status: String,
    /// 累计检查次数。
    pub total_checks: u64,
    /// 健康次数。
    pub healthy_checks: u64,
    /// 不健康次数。
    pub unhealthy_checks: u64,
}

impl HealthReport {
    /// 中文报告。
    pub fn report_zh(&self) -> String {
        let overall = if self.healthy { "✅ 健康" } else { "❌ 异常" };
        let live = if self.live_enabled { "⚠️ 真实交易" } else { "🔒 模拟交易" };
        let ping = if self.ping_ok { "✅" } else { "❌" };
        let ws = if self.ws_connected { "✅ 已连接" } else { "❌ 未连接" };
        let balance = if self.balance_ok { "✅" } else { "❌" };

        format!(
            "══════════════════════════════════════════════════\n\
             【Gateway 健康报告】{}\n\
             ══════════════════════════════════════════════════\n\
             \n\
              网关            : {} ({})\n\
              模式            : {}\n\
             综合状态        : {}\n\
             \n\
             ── 连接状态 ──\n\
              Ping           : {} {}ms\n\
              API 延迟       : {} ms\n\
              HTTP 成功率    : {:.1}%\n\
              WebSocket      : {}\n\
              Rate Limit     : {:.0}% 剩余\n\
              余额检查       : {}\n\
             \n\
             ── 累计统计 ──\n\
              总检查次数     : {}\n\
              健康次数       : {}\n\
              异常次数       : {}\n\
             \n\
             ── 详情 ──\n\
              {}\n\
             ══════════════════════════════════════════════════",
            "",
            self.gateway_name,
            self.gateway_type,
            live,
            overall,
            ping,
            self.ping_latency_ms,
            self.api_latency_ms,
            self.http_success_rate * 100.0,
            ws,
            self.rate_limit_remaining,
            balance,
            self.total_checks,
            self.healthy_checks,
            self.unhealthy_checks,
            self.connection_status,
        )
    }

    /// 简短状态行。
    pub fn status_line_zh(&self) -> String {
        let status = if self.healthy { "✅" } else { "❌" };
        format!(
            "{} {} | Ping: {}ms | API: {}ms | HTTP: {:.0}% | WS: {} | RL: {:.0}%",
            status,
            self.gateway_name,
            self.ping_latency_ms,
            self.api_latency_ms,
            self.http_success_rate * 100.0,
            if self.ws_connected { "✅" } else { "❌" },
            self.rate_limit_remaining,
        )
    }
}

/// Gateway Health CSV 记录。
#[derive(Debug, Clone, Serialize)]
pub struct HealthRecord {
    pub timestamp: String,
    pub healthy: bool,
    pub ping_latency_ms: u64,
    pub api_latency_ms: u64,
    pub http_success_rate: f64,
    pub ws_connected: bool,
    pub rate_limit_remaining: f64,
    pub balance_ok: bool,
}

impl HealthRecord {
    pub fn header() -> &'static str {
        "timestamp,healthy,ping_latency_ms,api_latency_ms,http_success_rate,ws_connected,rate_limit_remaining,balance_ok"
    }

    pub fn from_report(r: &HealthReport) -> Self {
        Self {
            timestamp: r.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            healthy: r.healthy,
            ping_latency_ms: r.ping_latency_ms,
            api_latency_ms: r.api_latency_ms,
            http_success_rate: r.http_success_rate,
            ws_connected: r.ws_connected,
            rate_limit_remaining: r.rate_limit_remaining,
            balance_ok: r.balance_ok,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_checker_initial_needs_check() {
        let checker = HealthChecker::default();
        assert!(checker.needs_check(Local::now()));
    }

    #[test]
    fn health_report_zh_contains_info() {
        let now = Local::now();
        let report = HealthReport {
            timestamp: now,
            healthy: true,
            gateway_name: "MockGateway".into(),
            gateway_type: "mock".into(),
            live_enabled: false,
            ping_ok: true,
            ping_latency_ms: 5,
            api_latency_ms: 10,
            http_success_rate: 1.0,
            ws_connected: true,
            rate_limit_remaining: 100.0,
            balance_ok: true,
            connection_status: "模拟网关始终健康".into(),
            total_checks: 1,
            healthy_checks: 1,
            unhealthy_checks: 0,
        };

        let r = report.report_zh();
        assert!(r.contains("健康"));
        assert!(r.contains("MockGateway"));
        assert!(r.contains("模拟交易"));
    }

    #[test]
    fn health_report_unhealthy() {
        let now = Local::now();
        let report = HealthReport {
            timestamp: now,
            healthy: false,
            gateway_name: "PolymarketGateway".into(),
            gateway_type: "polymarket".into(),
            live_enabled: true,
            ping_ok: false,
            ping_latency_ms: 0,
            api_latency_ms: 0,
            http_success_rate: 0.0,
            ws_connected: false,
            rate_limit_remaining: 0.0,
            balance_ok: false,
            connection_status: "API 不可达".into(),
            total_checks: 5,
            healthy_checks: 2,
            unhealthy_checks: 3,
        };

        let r = report.report_zh();
        assert!(r.contains("异常"));
        assert!(r.contains("真实交易"));
        assert!(r.contains("API 不可达"));
    }

    #[test]
    fn status_line_zh() {
        let now = Local::now();
        let report = HealthReport {
            timestamp: now,
            healthy: true,
            gateway_name: "Test".into(),
            gateway_type: "mock".into(),
            live_enabled: false,
            ping_ok: true,
            ping_latency_ms: 5,
            api_latency_ms: 10,
            http_success_rate: 0.99,
            ws_connected: true,
            rate_limit_remaining: 85.0,
            balance_ok: true,
            connection_status: "正常".into(),
            total_checks: 1,
            healthy_checks: 1,
            unhealthy_checks: 0,
        };
        let line = report.status_line_zh();
        assert!(line.contains("✅"));
        assert!(line.contains("5ms"));
    }

    #[test]
    fn health_record_header() {
        assert!(HealthRecord::header().contains("timestamp"));
        assert!(HealthRecord::header().contains("healthy"));
    }
}
