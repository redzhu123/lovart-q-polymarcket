//! 市场健康检查（P3.0 第七节）。
//!
//! 统一所有市场的健康检查：
//! - REST 连接
//! - WebSocket 连接
//! - Gateway 状态
//! - 认证状态
//! - 延迟
//! - 流数据
//!
//! 所有健康报告使用中文。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

// ============================================================================
// MarketHealthStatus
// ============================================================================

/// 市场健康状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarketHealthStatus {
    /// 健康 — 所有检查通过。
    Healthy,
    /// 降级 — 部分功能正常。
    Degraded,
    /// 异常 — 无法正常服务。
    Unhealthy,
    /// 未知 — 尚未执行健康检查。
    #[default]
    Unknown,
}

impl MarketHealthStatus {
    /// Emoji 表示。
    pub fn emoji(&self) -> &'static str {
        match self {
            MarketHealthStatus::Healthy => "✅",
            MarketHealthStatus::Degraded => "⚠️",
            MarketHealthStatus::Unhealthy => "❌",
            MarketHealthStatus::Unknown => "❓",
        }
    }

    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            MarketHealthStatus::Healthy => "健康",
            MarketHealthStatus::Degraded => "降级",
            MarketHealthStatus::Unhealthy => "异常",
            MarketHealthStatus::Unknown => "未知",
        }
    }

    /// 是否健康。
    pub fn is_healthy(&self) -> bool {
        matches!(self, MarketHealthStatus::Healthy)
    }
}

// ============================================================================
// HealthDimension
// ============================================================================

/// 健康检查维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthDimension {
    /// REST API 连接。
    Rest,
    /// WebSocket 连接。
    WebSocket,
    /// 网关状态。
    Gateway,
    /// 认证状态。
    Authentication,
    /// 延迟。
    Latency,
    /// 流数据。
    Streaming,
}

impl HealthDimension {
    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            HealthDimension::Rest => "REST API",
            HealthDimension::WebSocket => "WebSocket",
            HealthDimension::Gateway => "网关",
            HealthDimension::Authentication => "认证",
            HealthDimension::Latency => "延迟",
            HealthDimension::Streaming => "流数据",
        }
    }
}

// ============================================================================
// DimensionCheck
// ============================================================================

/// 单个维度的健康检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionCheck {
    /// 维度。
    pub dimension: HealthDimension,
    /// 状态。
    pub status: MarketHealthStatus,
    /// 延迟（毫秒）。
    pub latency_ms: u64,
    /// 详情消息。
    pub detail: String,
    /// 是否适用（某些市场可能没有 WebSocket）。
    pub applicable: bool,
}

impl DimensionCheck {
    /// 创建成功的检查结果。
    pub fn ok(dimension: HealthDimension, latency_ms: u64) -> Self {
        Self {
            dimension,
            status: MarketHealthStatus::Healthy,
            latency_ms,
            detail: "正常".to_string(),
            applicable: true,
        }
    }

    /// 创建失败的检查结果。
    pub fn fail(dimension: HealthDimension, detail: impl Into<String>) -> Self {
        Self {
            dimension,
            status: MarketHealthStatus::Unhealthy,
            latency_ms: 0,
            detail: detail.into(),
            applicable: true,
        }
    }

    /// 创建不适用的检查结果。
    pub fn not_applicable(dimension: HealthDimension) -> Self {
        Self {
            dimension,
            status: MarketHealthStatus::Healthy,
            latency_ms: 0,
            detail: "不适用".to_string(),
            applicable: false,
        }
    }

    /// 单行中文摘要。
    pub fn line_zh(&self) -> String {
        if !self.applicable {
            return format!("  ⏭️  {:<15} 不适用", self.dimension.as_zh());
        }
        format!(
            "  {}  {:<15} {} ({}ms)",
            self.status.emoji(),
            self.dimension.as_zh(),
            self.detail,
            self.latency_ms
        )
    }
}

// ============================================================================
// MarketHealthReport
// ============================================================================

/// 市场健康检查报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketHealthReport {
    /// 市场名称。
    pub market_name: String,
    /// 时间戳。
    pub timestamp: DateTime<Local>,
    /// 总体状态。
    pub overall: MarketHealthStatus,
    /// 各项检查。
    pub checks: Vec<DimensionCheck>,
}

impl MarketHealthReport {
    /// 创建健康的报告。
    pub fn healthy(market_name: impl Into<String>) -> Self {
        Self {
            market_name: market_name.into(),
            timestamp: Local::now(),
            overall: MarketHealthStatus::Healthy,
            checks: vec![
                DimensionCheck::ok(HealthDimension::Rest, 0),
                DimensionCheck::ok(HealthDimension::Gateway, 0),
                DimensionCheck::ok(HealthDimension::Authentication, 0),
                DimensionCheck::ok(HealthDimension::Latency, 0),
            ],
        }
    }

    /// 创建未知状态的报告（尚未检查）。
    pub fn unknown(market_name: impl Into<String>) -> Self {
        Self {
            market_name: market_name.into(),
            timestamp: Local::now(),
            overall: MarketHealthStatus::Unknown,
            checks: Vec::new(),
        }
    }

    /// 创建详细的健康报告。
    pub fn with_checks(market_name: impl Into<String>, checks: Vec<DimensionCheck>) -> Self {
        let overall = if checks
            .iter()
            .any(|c| c.applicable && c.status == MarketHealthStatus::Unhealthy)
        {
            MarketHealthStatus::Unhealthy
        } else if checks
            .iter()
            .any(|c| c.applicable && c.status == MarketHealthStatus::Degraded)
        {
            MarketHealthStatus::Degraded
        } else {
            MarketHealthStatus::Healthy
        };

        Self {
            market_name: market_name.into(),
            timestamp: Local::now(),
            overall,
            checks,
        }
    }

    /// 是否整体健康。
    pub fn overall_healthy(&self) -> bool {
        self.overall.is_healthy()
    }

    /// 健康组件数量。
    pub fn healthy_count(&self) -> usize {
        self.checks.iter().filter(|c| c.status.is_healthy()).count()
    }

    /// 总组件数量（仅适用）。
    pub fn applicable_count(&self) -> usize {
        self.checks.iter().filter(|c| c.applicable).count()
    }

    /// 中文完整报告。
    pub fn report_zh(&self) -> String {
        let mut lines = vec![
            format!("══════ {} 健康报告 ══════", self.market_name),
            format!("时间: {}", self.timestamp.format("%Y-%m-%d %H:%M:%S")),
            format!(
                "总体状态: {} {}",
                self.overall.emoji(),
                self.overall.as_zh()
            ),
            String::new(),
            "--- 维度检查 ---".to_string(),
        ];

        for check in &self.checks {
            lines.push(check.line_zh());
        }

        lines.push(String::new());
        lines.push(format!(
            "健康率: {}/{}",
            self.healthy_count(),
            self.applicable_count()
        ));
        lines.push("════════════════════════════".to_string());
        lines.join("\n")
    }

    /// 单行状态摘要。
    pub fn status_line_zh(&self) -> String {
        format!(
            "{} {}: {} ({}/{})",
            self.overall.emoji(),
            self.market_name,
            self.overall.as_zh(),
            self.healthy_count(),
            self.applicable_count()
        )
    }

    /// 合并多个市场健康报告。
    pub fn merge(reports: &[MarketHealthReport]) -> MarketHealthReport {
        let all_checks: Vec<DimensionCheck> =
            reports.iter().flat_map(|r| r.checks.clone()).collect();

        let overall = if reports
            .iter()
            .any(|r| r.overall == MarketHealthStatus::Unhealthy)
        {
            MarketHealthStatus::Unhealthy
        } else if reports
            .iter()
            .any(|r| r.overall == MarketHealthStatus::Degraded)
        {
            MarketHealthStatus::Degraded
        } else if reports
            .iter()
            .all(|r| r.overall == MarketHealthStatus::Healthy)
        {
            MarketHealthStatus::Healthy
        } else {
            MarketHealthStatus::Unknown
        };

        MarketHealthReport {
            market_name: format!("全部市场（{} 个）", reports.len()),
            timestamp: Local::now(),
            overall,
            checks: all_checks,
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
    fn healthy_report() {
        let report = MarketHealthReport::healthy("Polymarket");
        assert!(report.overall_healthy());
        assert_eq!(report.healthy_count(), 4);
        assert!(report.report_zh().contains("Polymarket"));
        assert!(report.report_zh().contains("健康"));
    }

    #[test]
    fn unhealthy_report() {
        let checks = vec![
            DimensionCheck::fail(HealthDimension::Rest, "连接超时"),
            DimensionCheck::ok(HealthDimension::Gateway, 5),
        ];
        let report = MarketHealthReport::with_checks("Binance", checks);
        assert!(!report.overall_healthy());
        assert_eq!(report.overall, MarketHealthStatus::Unhealthy);
    }

    #[test]
    fn degraded_report() {
        let checks = vec![
            DimensionCheck::ok(HealthDimension::Rest, 10),
            DimensionCheck {
                dimension: HealthDimension::WebSocket,
                status: MarketHealthStatus::Degraded,
                latency_ms: 1000,
                detail: "延迟偏高".to_string(),
                applicable: true,
            },
        ];
        let report = MarketHealthReport::with_checks("OKX", checks);
        assert!(!report.overall_healthy());
        assert_eq!(report.overall, MarketHealthStatus::Degraded);
    }

    #[test]
    fn not_applicable_doesnt_degrade() {
        let checks = vec![
            DimensionCheck::ok(HealthDimension::Rest, 5),
            DimensionCheck::not_applicable(HealthDimension::WebSocket),
        ];
        let report = MarketHealthReport::with_checks("Test", checks);
        assert!(report.overall_healthy());
    }

    #[test]
    fn health_status_zh() {
        assert_eq!(MarketHealthStatus::Healthy.as_zh(), "健康");
        assert_eq!(MarketHealthStatus::Degraded.as_zh(), "降级");
        assert_eq!(MarketHealthStatus::Unhealthy.as_zh(), "异常");
        assert_eq!(MarketHealthStatus::Unknown.as_zh(), "未知");
    }

    #[test]
    fn dimension_zh() {
        assert_eq!(HealthDimension::Rest.as_zh(), "REST API");
        assert_eq!(HealthDimension::WebSocket.as_zh(), "WebSocket");
        assert_eq!(HealthDimension::Gateway.as_zh(), "网关");
        assert_eq!(HealthDimension::Authentication.as_zh(), "认证");
        assert_eq!(HealthDimension::Latency.as_zh(), "延迟");
        assert_eq!(HealthDimension::Streaming.as_zh(), "流数据");
    }

    #[test]
    fn merge_reports() {
        let r1 = MarketHealthReport::healthy("市场A");
        let r2 = MarketHealthReport::healthy("市场B");
        let merged = MarketHealthReport::merge(&[r1, r2]);
        assert!(merged.overall_healthy());
        assert!(merged.market_name.contains("2 个"));
    }

    #[test]
    fn merge_with_unhealthy() {
        let r1 = MarketHealthReport::healthy("市场A");
        let checks = vec![DimensionCheck::fail(HealthDimension::Rest, "故障")];
        let r2 = MarketHealthReport::with_checks("市场B", checks);
        let merged = MarketHealthReport::merge(&[r1, r2]);
        assert!(!merged.overall_healthy());
    }

    #[test]
    fn status_line_zh() {
        let report = MarketHealthReport::healthy("测试");
        let line = report.status_line_zh();
        assert!(line.contains("测试"));
        assert!(line.contains("健康"));
    }

    #[test]
    fn unknown_report() {
        let report = MarketHealthReport::unknown("新市场");
        assert_eq!(report.overall, MarketHealthStatus::Unknown);
        assert!(report.checks.is_empty());
    }
}
