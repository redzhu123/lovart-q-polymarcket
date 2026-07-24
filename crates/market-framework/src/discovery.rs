//! 市场发现（P3.0 第八节）。
//!
//! 启动时自动发现所有已注册的 MarketPlugin。
//! 系统无需手动修改代码即可新增市场。
//!
//! # 当前实现
//!
//! - 静态注册模式：插件通过代码显式注册
//! - 动态加载预留：接口已设计，未来支持从 .so/.dll 动态加载
//!
//! # 发现流程
//!
//! 1. 调用 `discover_all()` 收集所有已注册插件
//! 2. 输出发现报告
//! 3. 返回插件列表

use crate::registry::MarketRegistry;
use chrono::Local;

// ============================================================================
// DiscoveryResult
// ============================================================================

/// 发现结果。
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// 发现的插件 ID 列表。
    pub plugin_ids: Vec<String>,
    /// 发现时间戳。
    pub discovered_at: String,
    /// 发现耗时（毫秒）。
    pub elapsed_ms: u64,
    /// 是否有发现错误。
    pub errors: Vec<String>,
}

impl DiscoveryResult {
    /// 创建空的发现结果。
    pub fn empty() -> Self {
        Self {
            plugin_ids: Vec::new(),
            discovered_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            elapsed_ms: 0,
            errors: Vec::new(),
        }
    }

    /// 是否发现了插件。
    pub fn has_plugins(&self) -> bool {
        !self.plugin_ids.is_empty()
    }

    /// 发现的插件数量。
    pub fn count(&self) -> usize {
        self.plugin_ids.len()
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        if self.plugin_ids.is_empty() {
            return format!(
                "【市场发现】{}\n  结果: ⚠️ 未发现任何市场插件\n  耗时: {}ms",
                self.discovered_at, self.elapsed_ms
            );
        }

        let mut lines = vec![
            format!(
                "【市场发现】{}\n  结果: ✅ 发现 {} 个市场插件\n  耗时: {}ms",
                self.discovered_at,
                self.count(),
                self.elapsed_ms
            ),
            String::new(),
            "  已发现插件:".to_string(),
        ];

        for (i, id) in self.plugin_ids.iter().enumerate() {
            lines.push(format!("    {}. {}", i + 1, id));
        }

        if !self.errors.is_empty() {
            lines.push(String::new());
            lines.push("  错误:".to_string());
            for err in &self.errors {
                lines.push(format!("    ❌ {}", err));
            }
        }

        lines.join("\n")
    }
}

// ============================================================================
// Discovery
// ============================================================================

/// 市场发现器。
///
/// 负责发现、收集和报告已注册的市场插件。
/// 当前使用静态注册，未来支持动态加载。
pub struct Discovery;

impl Discovery {
    /// 从头开始发现所有市场（静态注册模式）。
    ///
    /// 此函数不实际扫描文件系统 — 它列出注册中心中已注册的所有插件。
    /// 生产环境中，插件应在 `main()` 中注册后再调用此函数。
    ///
    /// # 参数
    ///
    /// - `registry`: 市场注册中心
    ///
    /// # 返回
    ///
    /// 发现结果，包含所有已注册插件的 ID。
    pub fn discover_all(registry: &MarketRegistry) -> DiscoveryResult {
        let start = std::time::Instant::now();
        tracing::info!("开始市场发现...");

        let plugin_ids = registry.list_ids();
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let result = DiscoveryResult {
            plugin_ids,
            discovered_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            elapsed_ms,
            errors: Vec::new(),
        };

        tracing::info!(
            "市场发现完成: 发现 {} 个插件（{}ms）",
            result.count(),
            elapsed_ms
        );

        result
    }

    /// 发现并渲染为中文报告。
    pub fn discover_and_report(registry: &MarketRegistry) -> String {
        let result = Self::discover_all(registry);
        result.summary_zh()
    }

    /// 按能力过滤发现结果。
    pub fn discover_by_capability(
        registry: &MarketRegistry,
        capability: &crate::capability::MarketCapability,
    ) -> DiscoveryResult {
        let start = std::time::Instant::now();
        let plugin_ids = registry.find_by_capability(capability);
        let elapsed_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            "按能力 '{}' 发现: {} 个插件",
            capability.as_zh(),
            plugin_ids.len()
        );

        DiscoveryResult {
            plugin_ids,
            discovered_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            elapsed_ms,
            errors: Vec::new(),
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilitySet, MarketCapability};
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
        fn new(id: &str, name: &str, type_code: &str) -> Self {
            let caps = CapabilitySet::prediction_market_full();
            let metadata = MarketMetadata::prediction_market(name, "TEST");
            Self {
                id: id.to_string(),
                name: name.to_string(),
                type_code: type_code.to_string(),
                caps,
                metadata,
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
    fn discover_empty_registry() {
        let reg = MarketRegistry::new();
        let result = Discovery::discover_all(&reg);
        assert!(!result.has_plugins());
        assert_eq!(result.count(), 0);
    }

    #[test]
    fn discover_with_plugins() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("p1", "插件1", "prediction")))
            .unwrap();
        reg.register(Box::new(TestPlugin::new("p2", "插件2", "prediction")))
            .unwrap();

        let result = Discovery::discover_all(&reg);
        assert!(result.has_plugins());
        assert_eq!(result.count(), 2);
        assert!(result.plugin_ids.contains(&"p1".to_string()));
        assert!(result.plugin_ids.contains(&"p2".to_string()));
    }

    #[test]
    fn discover_by_capability() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("pm", "Polymarket", "prediction")))
            .unwrap();

        let result = Discovery::discover_by_capability(&reg, &MarketCapability::Prediction);
        assert!(result.has_plugins());
        assert_eq!(result.count(), 1);
    }

    #[test]
    fn discovery_summary_zh() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestPlugin::new("test", "测试", "prediction")))
            .unwrap();

        let report = Discovery::discover_and_report(&reg);
        assert!(report.contains("市场发现"));
        assert!(report.contains("test"));
        assert!(report.contains("✅"));
    }

    #[test]
    fn discovery_empty_summary() {
        let reg = MarketRegistry::new();
        let report = Discovery::discover_and_report(&reg);
        assert!(report.contains("未发现"));
    }
}
