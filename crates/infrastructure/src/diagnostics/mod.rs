//! 诊断工具：统一的诊断接口和报告生成。
//!
//! 从 `pm-auth::diagnostics`、`pm-gateway::diagnostics`、`pm-trading::diagnostics` 提取并统一。

use crate::health::HealthStatus;
use async_trait::async_trait;
use chrono::{DateTime, Local};

/// 诊断项
#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    /// 名称
    pub name: String,
    /// 状态
    pub status: HealthStatus,
    /// 详情
    pub detail: String,
}

/// 诊断报告
#[derive(Debug, Clone)]
pub struct DiagnosticReport {
    /// 标题
    pub title: String,
    /// 时间戳
    pub timestamp: DateTime<Local>,
    /// 诊断项列表
    pub items: Vec<DiagnosticItem>,
    /// 建议
    pub suggestions: Vec<String>,
}

impl DiagnosticReport {
    /// 创建新的诊断报告
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            timestamp: Local::now(),
            items: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// 添加诊断项
    pub fn add_item(&mut self, name: &str, status: HealthStatus, detail: &str) {
        self.items.push(DiagnosticItem {
            name: name.to_string(),
            status,
            detail: detail.to_string(),
        });
    }

    /// 添加建议
    pub fn add_suggestion(&mut self, suggestion: impl Into<String>) {
        self.suggestions.push(suggestion.into());
    }

    /// 格式化中文报告
    pub fn format_zh(&self) -> String {
        let mut lines = vec![
            format!("══════ {} ══════", self.title),
            format!("时间: {}", self.timestamp.format("%Y-%m-%d %H:%M:%S")),
            String::new(),
        ];

        for item in &self.items {
            lines.push(format!(
                "  {} {}: {}",
                item.status.emoji(),
                item.name,
                item.detail
            ));
        }

        if !self.suggestions.is_empty() {
            lines.push(String::new());
            lines.push("建议:".to_string());
            for (i, s) in self.suggestions.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, s));
            }
        }

        lines.push("════════════════════════".to_string());
        lines.join("\n")
    }
}

/// 可诊断的组件
#[async_trait]
pub trait Diagnosable: Send + Sync {
    /// 诊断名称
    fn diagnostic_name(&self) -> &str;

    /// 执行诊断
    async fn run_diagnostics(&self) -> DiagnosticReport;
}

/// 诊断中心
pub struct DiagnosticsCenter {
    components: Vec<Box<dyn Diagnosable>>,
}

impl Default for DiagnosticsCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticsCenter {
    /// 创建新的诊断中心
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// 注册诊断组件
    pub fn register(&mut self, component: Box<dyn Diagnosable>) {
        self.components.push(component);
    }

    /// 诊断指定组件
    pub async fn diagnose(&self, name: &str) -> Option<DiagnosticReport> {
        for comp in &self.components {
            if comp.diagnostic_name() == name {
                return Some(comp.run_diagnostics().await);
            }
        }
        None
    }

    /// 诊断所有组件
    pub async fn diagnose_all(&self) -> Vec<DiagnosticReport> {
        let mut reports = Vec::new();
        for comp in &self.components {
            reports.push(comp.run_diagnostics().await);
        }
        reports
    }
}

// --- 便捷诊断函数 ---
// 从各 crate 的 diagnose_* 函数提取

use crate::cache::Cache;
use crate::event_bus::EventBus;
use crate::health::HealthCenter;
use crate::scheduler::Scheduler;
use crate::secret::SecretManager;
use crate::storage::Storage;

/// 诊断密钥管理器
pub async fn diagnose_secret(manager: &dyn SecretManager) -> DiagnosticReport {
    let mut report = DiagnosticReport::new("密钥管理器诊断");
    let providers = manager.provider_names();
    report.add_item(
        "提供者数量",
        HealthStatus::Healthy,
        &format!("{} 个", providers.len()),
    );
    report.add_item(
        "真实凭证",
        if manager.has_real_credentials() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        },
        if manager.has_real_credentials() {
            "已加载"
        } else {
            "未加载真实凭证"
        },
    );
    if manager.has_real_credentials() {
        report.add_suggestion("凭证已加载，请确保不输出明文到日志");
    }
    report
}

/// 诊断缓存
pub async fn diagnose_cache(cache: &dyn Cache) -> DiagnosticReport {
    let mut report = DiagnosticReport::new(format!("缓存诊断: {}", cache.name()));
    let size = cache.size().await;
    let stats = cache.stats();
    report.add_item("条目数", HealthStatus::Healthy, &format!("{}", size));
    report.add_item(
        "命中率",
        if stats.hit_rate() > 50.0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        },
        &format!(
            "{:.1}% (命中 {}/未命中 {})",
            stats.hit_rate(),
            stats.hits,
            stats.misses
        ),
    );
    report
}

/// 诊断存储
pub async fn diagnose_storage(storage: &dyn Storage) -> DiagnosticReport {
    let mut report = DiagnosticReport::new(format!("存储诊断: {}", storage.name()));
    let health = storage.health();
    let count = storage.count().await.unwrap_or(0);
    report.add_item(
        "健康状态",
        if health.healthy {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        },
        &health.message,
    );
    report.add_item("记录数", HealthStatus::Healthy, &format!("{}", count));
    report
}

/// 诊断调度器
pub async fn diagnose_scheduler(scheduler: &dyn Scheduler) -> DiagnosticReport {
    let mut report = DiagnosticReport::new(format!("调度器诊断: {}", scheduler.name()));
    let stats = scheduler.stats();
    report.add_item(
        "注册任务",
        HealthStatus::Healthy,
        &format!("{} 个", stats.registered_tasks),
    );
    report.add_item(
        "执行统计",
        HealthStatus::Healthy,
        &format!(
            "总数 {} / 成功 {} / 失败 {}",
            stats.total_executions, stats.total_successes, stats.total_failures
        ),
    );
    if stats.total_failures > 0 {
        report.add_suggestion("存在失败任务，建议检查日志排查原因");
    }
    report
}

/// 诊断事件总线
pub fn diagnose_event_bus(bus: &EventBus) -> DiagnosticReport {
    let mut report = DiagnosticReport::new("事件总线诊断");
    report.add_item(
        "订阅者",
        HealthStatus::Healthy,
        &format!("{} 个", bus.subscriber_count()),
    );
    report.add_item(
        "已发布",
        HealthStatus::Healthy,
        &format!("{} 个事件", bus.published_count()),
    );
    report
}

/// 诊断健康中心
pub async fn diagnose_health_center(center: &HealthCenter) -> DiagnosticReport {
    let mut report = DiagnosticReport::new("健康中心诊断");
    let health_report = center.check_all().await;
    report.add_item(
        "组件数",
        HealthStatus::Healthy,
        &format!("{} 个", center.component_count()),
    );
    report.add_item("总体状态", health_report.overall, "");
    for check in &health_report.checks {
        report.add_item(
            &format!("组件: {}", check.component),
            check.status,
            &check.detail,
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_report_format_zh() {
        let mut report = DiagnosticReport::new("测试诊断");
        report.add_item("组件A", HealthStatus::Healthy, "正常");
        report.add_item("组件B", HealthStatus::Unhealthy, "连接失败");
        report.add_suggestion("检查网络连接");
        report.add_suggestion("检查配置文件");

        let zh = report.format_zh();
        assert!(zh.contains("测试诊断"));
        assert!(zh.contains("组件A"));
        assert!(zh.contains("连接失败"));
        assert!(zh.contains("建议"));
        assert!(zh.contains("检查网络连接"));
    }

    #[test]
    fn diagnostic_item_creation() {
        let item = DiagnosticItem {
            name: "test".to_string(),
            status: HealthStatus::Healthy,
            detail: "一切正常".to_string(),
        };
        assert_eq!(item.name, "test");
        assert_eq!(item.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn diagnose_event_bus_works() {
        let bus = EventBus::new();
        let report = diagnose_event_bus(&bus);
        assert!(report.format_zh().contains("事件总线"));
        assert!(report.items.iter().any(|i| i.name == "订阅者"));
    }
}
