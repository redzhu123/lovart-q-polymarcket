//! 健康中心：统一的健康检查框架。
//!
//! 从 `pm-gateway::health` 和 `pm-scanner::health` 提取并统一。
//!
//! # 核心能力
//!
//! - [`HealthCheckable`] trait：可被健康检查的组件
//! - [`HealthCenter`]：集中化的健康检查中心
//! - [`HealthReport`]：中文健康报告

use async_trait::async_trait;
use chrono::{DateTime, Local};

/// 健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 降级
    Degraded,
    /// 不健康
    Unhealthy,
}

impl HealthStatus {
    /// Emoji 表示
    pub fn emoji(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "✅",
            HealthStatus::Degraded => "⚠️",
            HealthStatus::Unhealthy => "❌",
        }
    }

    /// 中文名称
    pub fn zh(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "健康",
            HealthStatus::Degraded => "降级",
            HealthStatus::Unhealthy => "异常",
        }
    }

    /// 从布尔值创建
    pub fn from_bool(healthy: bool) -> Self {
        if healthy {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        }
    }
}

/// 单项健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// 组件名称
    pub component: String,
    /// 健康状态
    pub status: HealthStatus,
    /// 详情
    pub detail: String,
    /// 检查耗时（毫秒）
    pub latency_ms: u64,
}

impl HealthCheck {
    /// 创建健康的检查结果
    pub fn healthy(component: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            status: HealthStatus::Healthy,
            detail: detail.into(),
            latency_ms: 0,
        }
    }

    /// 创建不健康的检查结果
    pub fn unhealthy(component: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            status: HealthStatus::Unhealthy,
            detail: detail.into(),
            latency_ms: 0,
        }
    }

    /// 一行摘要
    pub fn status_line_zh(&self) -> String {
        format!(
            "{} {}: {} ({}ms)",
            self.status.emoji(),
            self.component,
            self.detail,
            self.latency_ms
        )
    }
}

/// 健康报告
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// 时间戳
    pub timestamp: DateTime<Local>,
    /// 总体健康状态
    pub overall: HealthStatus,
    /// 各项检查结果
    pub checks: Vec<HealthCheck>,
}

impl HealthReport {
    /// 是否全部健康
    pub fn all_healthy(&self) -> bool {
        self.overall == HealthStatus::Healthy
    }

    /// 中文完整报告
    pub fn report_zh(&self) -> String {
        let mut lines = vec![
            "══════ 健康检查报告 ══════".to_string(),
            format!("时间: {}", self.timestamp.format("%Y-%m-%d %H:%M:%S")),
            format!("总体状态: {} {}", self.overall.emoji(), self.overall.zh()),
            "".to_string(),
            "--- 组件检查 ---".to_string(),
        ];

        for check in &self.checks {
            lines.push(check.status_line_zh());
        }

        lines.push("════════════════════════".to_string());
        lines.join("\n")
    }

    /// 单行状态
    pub fn status_line_zh(&self) -> String {
        format!(
            "{} 总体: {}, 组件: {}/{} 健康",
            self.overall.emoji(),
            self.overall.zh(),
            self.checks
                .iter()
                .filter(|c| c.status == HealthStatus::Healthy)
                .count(),
            self.checks.len(),
        )
    }
}

/// 可被健康检查的组件
#[async_trait]
pub trait HealthCheckable: Send + Sync {
    /// 组件名称
    fn component_name(&self) -> &str;

    /// 执行健康检查
    async fn health_check(&self) -> HealthCheck;
}

/// 健康检查中心
pub struct HealthCenter {
    components: Vec<Box<dyn HealthCheckable>>,
}

impl HealthCenter {
    /// 创建新的健康中心
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// 注册健康检查组件
    pub fn register(&mut self, component: Box<dyn HealthCheckable>) {
        tracing::info!("注册健康检查组件: {}", component.component_name());
        self.components.push(component);
    }

    /// 执行全部健康检查
    pub async fn check_all(&self) -> HealthReport {
        let mut checks = Vec::new();
        for component in &self.components {
            let start = std::time::Instant::now();
            let mut check = component.health_check().await;
            check.latency_ms = start.elapsed().as_millis() as u64;
            tracing::debug!("健康检查 {}: {}", check.component, check.status.zh());
            checks.push(check);
        }

        let overall = if checks.iter().any(|c| c.status == HealthStatus::Unhealthy) {
            HealthStatus::Unhealthy
        } else if checks.iter().any(|c| c.status == HealthStatus::Degraded) {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        HealthReport {
            timestamp: Local::now(),
            overall,
            checks,
        }
    }

    /// 组件数量
    pub fn component_count(&self) -> usize {
        self.components.len()
    }
}

impl Default for HealthCenter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestComponent {
        name: String,
        healthy: bool,
        detail: String,
    }

    #[async_trait]
    impl HealthCheckable for TestComponent {
        fn component_name(&self) -> &str {
            &self.name
        }

        async fn health_check(&self) -> HealthCheck {
            HealthCheck {
                component: self.name.clone(),
                status: if self.healthy {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy
                },
                detail: self.detail.clone(),
                latency_ms: 0,
            }
        }
    }

    #[tokio::test]
    async fn health_center_all_healthy() {
        let mut center = HealthCenter::new();
        center.register(Box::new(TestComponent {
            name: "Gateway".to_string(),
            healthy: true,
            detail: "正常".to_string(),
        }));
        center.register(Box::new(TestComponent {
            name: "OMS".to_string(),
            healthy: true,
            detail: "正常".to_string(),
        }));

        let report = center.check_all().await;
        assert!(report.all_healthy());
        assert_eq!(report.overall, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn health_center_one_unhealthy() {
        let mut center = HealthCenter::new();
        center.register(Box::new(TestComponent {
            name: "Gateway".to_string(),
            healthy: false,
            detail: "连接失败".to_string(),
        }));

        let report = center.check_all().await;
        assert!(!report.all_healthy());
        assert_eq!(report.overall, HealthStatus::Unhealthy);
    }

    #[test]
    fn health_status_emoji() {
        assert_eq!(HealthStatus::Healthy.emoji(), "✅");
        assert_eq!(HealthStatus::Unhealthy.emoji(), "❌");
    }

    #[test]
    fn health_status_zh() {
        assert_eq!(HealthStatus::Healthy.zh(), "健康");
        assert_eq!(HealthStatus::Degraded.zh(), "降级");
    }

    #[test]
    fn health_report_zh_format() {
        let report = HealthReport {
            timestamp: Local::now(),
            overall: HealthStatus::Healthy,
            checks: vec![HealthCheck::healthy("Test", "正常")],
        };
        let zh = report.report_zh();
        assert!(zh.contains("健康检查报告"));
        assert!(zh.contains("健康"));
    }

    #[test]
    fn health_check_status_line() {
        let check = HealthCheck::healthy("API", "延迟 10ms");
        let line = check.status_line_zh();
        assert!(line.contains("API"));
    }
}
