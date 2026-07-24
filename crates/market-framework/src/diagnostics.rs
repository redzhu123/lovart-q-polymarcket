//! 市场框架诊断工具（P3.0）。
//!
//! 提供市场框架的诊断功能，包括：
//! - 插件列表生成
//! - 能力报告
//! - 健康汇总
//! - 注册表状态
//! - 完整诊断报告

use chrono::Local;

use crate::discovery::Discovery;
use crate::health::MarketHealthReport;
use crate::registry::MarketRegistry;

// ============================================================================
// MarketFrameworkReport
// ============================================================================

/// 市场框架完整诊断报告。
#[derive(Debug, Clone)]
pub struct MarketFrameworkReport {
    /// 生成时间。
    pub generated_at: String,
    /// 插件总数。
    pub total_plugins: usize,
    /// 插件摘要列表。
    pub plugin_summaries: Vec<String>,
    /// 能力矩阵。
    pub capability_matrix: String,
    /// 健康状态汇总。
    pub health_summaries: Vec<String>,
    /// 发现的插件列表。
    pub discovered: String,
    /// 注册表状态。
    pub registry_state: String,
    /// 已知限制。
    pub known_limitations: Vec<String>,
    /// 后续优化建议。
    pub optimization_suggestions: Vec<String>,
}

impl MarketFrameworkReport {
    /// 生成空报告。
    pub fn empty() -> Self {
        Self {
            generated_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            total_plugins: 0,
            plugin_summaries: Vec::new(),
            capability_matrix: String::new(),
            health_summaries: Vec::new(),
            discovered: String::new(),
            registry_state: String::new(),
            known_limitations: vec![
                "当前仅支持静态注册，动态加载（.so/.dll）已预留但未实现".to_string(),
                "暂不支持热插拔（运行时动态增删插件）".to_string(),
                "暂不支持跨进程市场插件（未来考虑 IPC/gRPC 方案）".to_string(),
            ],
            optimization_suggestions: vec![
                "实现 dlopen 动态加载以减少编译依赖".to_string(),
                "添加插件沙箱（Wasm 运行时隔离）".to_string(),
                "支持市场插件热更新（不重启系统升级插件）".to_string(),
                "添加插件性能基准测试框架".to_string(),
                "实现市场拓扑可视化（市场间套利路径图）".to_string(),
            ],
        }
    }

    /// 完整中文报告。
    pub fn report_zh(&self) -> String {
        let sep = "═".repeat(60);
        let mut lines = vec![
            sep.clone(),
            "      多市场统一框架 — 诊断报告（P3.0）".to_string(),
            sep.clone(),
            format!("生成时间: {}", self.generated_at),
            format!("已安装市场: {} 个", self.total_plugins),
            String::new(),
        ];

        // 插件列表
        lines.push("── 插件列表 ──".to_string());
        if self.plugin_summaries.is_empty() {
            lines.push("  （无已注册插件）".to_string());
        } else {
            for s in &self.plugin_summaries {
                lines.push(format!("  {}", s));
            }
        }
        lines.push(String::new());

        // 能力矩阵
        lines.push("── 能力矩阵 ──".to_string());
        lines.push(self.capability_matrix.clone());
        lines.push(String::new());

        // 健康状态
        lines.push("── 健康状态 ──".to_string());
        if self.health_summaries.is_empty() {
            lines.push("  （无健康检查数据）".to_string());
        } else {
            for s in &self.health_summaries {
                lines.push(format!("  {}", s));
            }
        }
        lines.push(String::new());

        // 发现结果
        lines.push("── 发现结果 ──".to_string());
        lines.push(self.discovered.clone());
        lines.push(String::new());

        // 注册表状态
        lines.push("── 注册表状态 ──".to_string());
        lines.push(self.registry_state.clone());
        lines.push(String::new());

        // 已知限制
        lines.push("── 已知限制 ──".to_string());
        for (i, limit) in self.known_limitations.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, limit));
        }
        lines.push(String::new());

        // 后续优化建议
        lines.push("── 后续优化建议 ──".to_string());
        for (i, suggestion) in self.optimization_suggestions.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, suggestion));
        }

        lines.push(String::new());
        lines.push(sep);
        lines.join("\n")
    }
}

// ============================================================================
// 诊断函数
// ============================================================================

/// 诊断：市场注册表状态。
pub fn diagnose_registry(registry: &MarketRegistry) -> String {
    let mut lines = vec![
        format!("注册表状态: {} 个插件已注册", registry.count()),
        format!("累计注册次数: {}", registry.registration_total()),
        format!(
            "自动发现: {}",
            if registry.is_empty() {
                "未启用"
            } else {
                "已启用"
            }
        ),
    ];

    let ids = registry.list_ids();
    if !ids.is_empty() {
        lines.push(format!("已注册 ID: {}", ids.join(", ")));
    }

    lines.join("\n")
}

/// 诊断：生成能力矩阵（Markdown 表格格式）。
pub fn diagnose_capability_matrix(registry: &MarketRegistry) -> String {
    let summaries = registry.list_all_summaries();
    if summaries.is_empty() {
        return "无已注册插件，无法生成能力矩阵。".to_string();
    }

    let mut lines = vec![
        String::new(),
        format!("| 市场 | 能力数 |"),
        String::new(),
        "|------|--------|".to_string(),
    ];

    for s in &summaries {
        lines.push(format!("| {} | {} |", s.name, s.capability_count));
    }

    lines.push(String::new());
    lines.join("\n")
}

/// 诊断：插件详细信息。
pub fn diagnose_plugin_details(registry: &MarketRegistry) -> String {
    let summaries = registry.list_all_summaries();
    if summaries.is_empty() {
        return "（无已注册插件）".to_string();
    }

    let mut lines = Vec::new();
    for s in &summaries {
        lines.push(s.line_zh());
    }
    lines.join("\n")
}

/// 生成完整的市场框架诊断报告。
///
/// # 参数
///
/// - `registry`: 市场注册中心
/// - `health_reports`: 各市场的健康报告
///
/// # 返回
///
/// 完整的 MarketFrameworkReport
pub fn generate_full_report(
    registry: &MarketRegistry,
    health_reports: &[MarketHealthReport],
) -> MarketFrameworkReport {
    let mut report = MarketFrameworkReport::empty();

    report.total_plugins = registry.count();
    report.plugin_summaries = registry
        .list_all_summaries()
        .iter()
        .map(|s| s.line_zh())
        .collect();
    report.capability_matrix = diagnose_capability_matrix(registry);
    report.health_summaries = health_reports.iter().map(|r| r.status_line_zh()).collect();
    report.discovered = Discovery::discover_and_report(registry);
    report.registry_state = diagnose_registry(registry);

    report
}

/// 生成插件列表文件内容。
pub fn generate_plugin_list(registry: &MarketRegistry) -> String {
    let summaries = registry.list_all_summaries();
    if summaries.is_empty() {
        return "# 已安装市场插件\n\n（空）\n".to_string();
    }

    let mut lines = vec![
        format!("# 已安装市场插件（{} 个）\n", summaries.len()),
        String::new(),
        "| # | 名称 | ID | 类型 | 网关 | 实盘 | 能力数 | 描述 |".to_string(),
        "|---|------|----|------|------|------|--------|------|".to_string(),
    ];

    for (i, s) in summaries.iter().enumerate() {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            i + 1,
            s.name,
            s.id,
            s.market_type,
            s.gateway,
            if s.live_enabled { "✅" } else { "❌" },
            s.capability_count,
            s.description,
        ));
    }

    lines.join("\n")
}

/// 生成能力报告文件内容。
pub fn generate_capability_report(registry: &MarketRegistry) -> String {
    let summaries = registry.list_all_summaries();
    if summaries.is_empty() {
        return "# 能力报告\n\n（无数据）\n".to_string();
    }

    let mut lines = vec![
        "# 能力报告\n".to_string(),
        format!("生成时间: {}\n", Local::now().format("%Y-%m-%d %H:%M:%S")),
        format!("已安装市场: {} 个\n", summaries.len()),
        String::new(),
        "## 能力矩阵\n".to_string(),
    ];

    for s in &summaries {
        lines.push(format!("### {}\n", s.name));
        lines.push(format!("- ID: {}", s.id));
        lines.push(format!("- 类型: {}", s.market_type));
        lines.push(format!("- 能力数: {}", s.capability_count));
        lines.push(format!("- 网关: {}", s.gateway));
        lines.push(format!(
            "- 实盘: {}",
            if s.live_enabled { "是" } else { "否" }
        ));
        lines.push(String::new());
    }

    lines.join("\n")
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::health::MarketHealthReport;
    use crate::metadata::MarketMetadata;
    use crate::plugin::MarketPlugin;
    use async_trait::async_trait;

    struct TestPlugin {
        id: String,
        name: String,
        type_code: String,
        caps: CapabilitySet,
        metadata: MarketMetadata,
    }

    impl TestPlugin {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
                type_code: "prediction".to_string(),
                caps: CapabilitySet::prediction_market_full(),
                metadata: MarketMetadata::prediction_market(name, "TEST"),
            }
        }
    }

    #[async_trait]
    impl MarketPlugin for TestPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn market_type_code(&self) -> &str {
            &self.type_code
        }
        fn supported_features(&self) -> &CapabilitySet {
            &self.caps
        }
        fn gateway_name(&self) -> &str {
            "test-gw"
        }
        fn metadata(&self) -> &MarketMetadata {
            &self.metadata
        }
        async fn health(&self) -> MarketHealthReport {
            MarketHealthReport::healthy(&self.name)
        }
    }

    #[test]
    fn empty_report() {
        let report = MarketFrameworkReport::empty();
        assert_eq!(report.total_plugins, 0);
        assert!(!report.known_limitations.is_empty());
        assert!(!report.optimization_suggestions.is_empty());
        let zh = report.report_zh();
        assert!(zh.contains("诊断报告"));
        assert!(zh.contains("无已注册插件"));
    }

    #[test]
    fn diagnose_empty_registry() {
        let reg = MarketRegistry::new();
        let diag = diagnose_registry(&reg);
        assert!(diag.contains("0 个插件"));
    }

    #[test]
    fn diagnose_registry_with_plugins() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("p1", "市场1")))
            .unwrap();

        let diag = diagnose_registry(&reg);
        assert!(diag.contains("1 个插件"));
        assert!(diag.contains("p1"));
    }

    #[test]
    fn generate_full_report_with_plugins() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("pm", "Polymarket")))
            .unwrap();
        reg.register(Box::new(TestPlugin::new("bn", "Binance")))
            .unwrap();

        let health_reports = vec![
            MarketHealthReport::healthy("Polymarket"),
            MarketHealthReport::healthy("Binance"),
        ];

        let report = generate_full_report(&reg, &health_reports);
        assert_eq!(report.total_plugins, 2);
        assert_eq!(report.health_summaries.len(), 2);
        assert!(!report.plugin_summaries.is_empty());

        let zh = report.report_zh();
        assert!(zh.contains("2 个"));
    }

    #[test]
    fn plugin_list_output() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("pm", "Polymarket")))
            .unwrap();

        let list = super::generate_plugin_list(&reg);
        assert!(list.contains("Polymarket"));
        assert!(list.contains("pm"));
    }

    #[test]
    fn capability_report_output() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("pm", "Polymarket")))
            .unwrap();

        let report = super::generate_capability_report(&reg);
        assert!(report.contains("Polymarket"));
        assert!(report.contains("能力"));
    }

    #[test]
    fn full_report_zh_contains_all_sections() {
        let report = MarketFrameworkReport::empty();
        let zh = report.report_zh();
        assert!(zh.contains("插件列表"));
        assert!(zh.contains("能力矩阵"));
        assert!(zh.contains("健康状态"));
        assert!(zh.contains("发现结果"));
        assert!(zh.contains("注册表状态"));
        assert!(zh.contains("已知限制"));
        assert!(zh.contains("优化建议"));
    }
}
