//! pm-market-framework：多市场统一框架（P3.0）。
//!
//! 系统不再围绕单一市场，而是升级为通用交易平台。
//! 任何市场必须注册为 MarketPlugin，不得直接修改核心系统。
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              Market Framework               │
//! │  ┌─────────┐ ┌──────────┐ ┌─────────────┐  │
//! │  │ Plugin  │ │ Registry │ │  Discovery   │  │
//! │  └────┬────┘ └────┬─────┘ └──────┬──────┘  │
//! │       │           │              │          │
//! │  ┌────┴────┐ ┌────┴─────┐ ┌──────┴──────┐  │
//! │  │Provider │ │Capability│ │   Health    │  │
//! │  └────┬────┘ └────┬─────┘ └──────┬──────┘  │
//! │       │           │              │          │
//! │  ┌────┴────┐ ┌────┴─────┐ ┌──────┴──────┐  │
//! │  │ Adapter │ │ Metadata │ │ Diagnostics │  │
//! │  └─────────┘ └──────────┘ └─────────────┘  │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # 模块
//!
//! - [`plugin`]：MarketPlugin trait — 统一市场接口
//! - [`capability`]：能力声明系统 — 每个市场声明自己的能力
//! - [`registry`]：MarketRegistry — 注册/注销/发现/查询
//! - [`discovery`]：Market Discovery — 自动发现已注册插件
//! - [`metadata`]：市场元数据 — 统一市场信息
//! - [`health`]：市场健康检查 — REST/WS/Gateway/Auth/Latency/Streaming
//! - [`provider`]：MarketDataProvider trait — 数据供应商
//! - [`adapter`]：MarketAdapter trait — 数据格式适配
//! - [`events`]：市场事件 — MarketRegistered/Removed/Connected/Disconnected
//! - [`diagnostics`]：诊断工具 — 报告生成
//! - [`error`]：错误类型 — 统一错误处理
//!
//! # 日志规范
//!
//! 所有日志使用中文，统一通过 tracing 输出。
//!
//! # 使用示例
//!
//! ```ignore
//! use pm_market_framework::prelude::*;
//! use pm_market_framework::MarketFramework;
//!
//! let mut framework = MarketFramework::new();
//! framework.register_plugin(Box::new(PolymarketPlugin::new()))?;
//! framework.register_plugin(Box::new(BinancePlugin::new()))?;
//!
//! // 列出所有市场
//! println!("{}", framework.registry().render_table_zh());
//!
//! // 健康检查
//! let report = framework.check_all_health().await;
//! println!("{}", report.report_zh());
//!
//! // 生成诊断报告
//! let diag = framework.generate_report();
//! println!("{}", diag.report_zh());
//! ```

pub mod adapter;
pub mod amm;
pub mod capability;
pub mod diagnostics;
pub mod discovery;
pub mod error;
pub mod events;
pub mod health;
pub mod metadata;
pub mod plugin;
pub mod provider;
pub mod quote;
pub mod registry;

// ============================================================================
// 预导入模块
// ============================================================================

/// 预导入模块：集中导出最常用类型。
pub mod prelude {
    pub use crate::MarketFramework;
    pub use crate::adapter::{MarketAdapter, NoopAdapter, UnifiedMarketSummary, UnifiedOrderBook};
    pub use crate::amm::{AmmPoolState, AmmState, DexPoolQuote};
    pub use crate::capability::{CapabilitySet, MarketCapability};
    pub use crate::diagnostics::{
        MarketFrameworkReport, diagnose_capability_matrix, diagnose_plugin_details,
        diagnose_registry, generate_capability_report, generate_full_report, generate_plugin_list,
    };
    pub use crate::discovery::{Discovery, DiscoveryResult};
    pub use crate::error::{MarketFrameworkError, MarketFrameworkResult};
    pub use crate::events::MarketEvent;
    pub use crate::health::{
        DimensionCheck, HealthDimension, MarketHealthReport, MarketHealthStatus,
    };
    pub use crate::metadata::{FeeModel, MarketId, MarketMetadata, MarketType};
    pub use crate::plugin::MarketPlugin;
    pub use crate::provider::{
        CexMarketDataProvider, DexMarketDataProvider, MarketDataProvider, MockMarketDataProvider,
    };
    pub use crate::quote::{CanonicalInstrument, ProductKind, VenueKind, VenueQuote};
    pub use crate::registry::{MarketRegistry, PluginSummary};
}

pub use amm::{AmmPoolState, AmmState, DexPoolQuote};
pub use quote::{CanonicalInstrument, ProductKind, VenueKind, VenueQuote};

// ============================================================================
// MarketFramework（顶层门面）
// ============================================================================

/// 多市场框架顶层门面（P3.0）。
///
/// 封装注册中心、发现、健康检查、诊断等所有功能。
/// 应用入口点应该创建一个 `MarketFramework` 实例，在其上注册所有市场插件。
///
/// # 使用示例
///
/// ```ignore
/// let fw = MarketFramework::new();
/// fw.register_plugin(Box::new(PolymarketPlugin::new()))?;
/// fw.discover();
/// println!("{}", fw.render_all());
/// ```
pub struct MarketFramework {
    /// 市场注册中心。
    registry: registry::MarketRegistry,
}

impl MarketFramework {
    /// 创建新的市场框架实例。
    pub fn new() -> Self {
        tracing::info!("多市场统一框架（P3.0）已初始化");
        Self {
            registry: registry::MarketRegistry::new(),
        }
    }

    /// 创建带自动发现的框架实例。
    pub fn with_auto_discover(enabled: bool) -> Self {
        tracing::info!(
            "多市场统一框架（P3.0）已初始化，自动发现: {}",
            if enabled { "启用" } else { "禁用" }
        );
        Self {
            registry: registry::MarketRegistry::new().with_auto_discover(enabled),
        }
    }

    /// 获取注册中心的引用。
    pub fn registry(&self) -> &registry::MarketRegistry {
        &self.registry
    }

    /// 注册一个市场插件。
    pub fn register_plugin(
        &self,
        plugin: Box<dyn plugin::MarketPlugin>,
    ) -> Result<(), error::MarketFrameworkError> {
        self.registry.register(plugin)
    }

    /// 注销一个市场插件。
    pub fn unregister_plugin(&self, id: &str) -> Result<String, error::MarketFrameworkError> {
        self.registry.unregister(id)
    }

    /// 执行市场发现。
    pub fn discover(&self) -> discovery::DiscoveryResult {
        discovery::Discovery::discover_all(&self.registry)
    }

    /// 渲染所有市场的中文信息。
    pub fn render_all(&self) -> String {
        let output = vec![
            "══════ 多市场统一框架（P3.0）══════".to_string(),
            String::new(),
            self.registry.render_table_zh(),
            String::new(),
            discovery::Discovery::discover_and_report(&self.registry),
        ];
        output.join("\n")
    }

    /// 生成完整的诊断报告。
    pub fn generate_report(&self) -> diagnostics::MarketFrameworkReport {
        let health_reports: Vec<health::MarketHealthReport> = Vec::new();
        diagnostics::generate_full_report(&self.registry, &health_reports)
    }
}

impl Default for MarketFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::MarketFramework;
    use super::prelude::*;
    use async_trait::async_trait;

    /// 完整框架的集成测试用插件。
    struct IntegrationTestPlugin {
        caps: CapabilitySet,
        metadata: MarketMetadata,
    }

    impl IntegrationTestPlugin {
        fn new(name: &str, _type_code: &str) -> Self {
            let caps = CapabilitySet::prediction_market_full();
            let metadata = MarketMetadata::prediction_market(name, "TEST");
            Self { caps, metadata }
        }
    }

    #[async_trait]
    impl MarketPlugin for IntegrationTestPlugin {
        fn id(&self) -> &str {
            "integration-test"
        }
        fn name(&self) -> &str {
            "集成测试插件"
        }
        fn market_type_code(&self) -> &str {
            "test"
        }
        fn supported_features(&self) -> &CapabilitySet {
            &self.caps
        }
        fn gateway_name(&self) -> &str {
            "test-gateway"
        }
        fn metadata(&self) -> &MarketMetadata {
            &self.metadata
        }
        async fn health(&self) -> MarketHealthReport {
            MarketHealthReport::healthy("集成测试插件")
        }
    }

    #[test]
    fn framework_new_is_empty() {
        let fw = MarketFramework::new();
        assert_eq!(fw.registry().count(), 0);
    }

    #[test]
    fn framework_register_and_list() {
        let fw = MarketFramework::new();
        let plugin = IntegrationTestPlugin::new("测试", "test");
        fw.register_plugin(Box::new(plugin)).unwrap();
        assert_eq!(fw.registry().count(), 1);
    }

    #[test]
    fn framework_discover() {
        let fw = MarketFramework::new();
        let plugin = IntegrationTestPlugin::new("测试市场", "test");
        fw.register_plugin(Box::new(plugin)).unwrap();

        let result = fw.discover();
        assert!(result.has_plugins());
        assert_eq!(result.count(), 1);
    }

    #[test]
    fn framework_render_all() {
        let fw = MarketFramework::new();
        let plugin = IntegrationTestPlugin::new("渲染测试", "test");
        fw.register_plugin(Box::new(plugin)).unwrap();

        let output = fw.render_all();
        assert!(output.contains("P3.0"));
        assert!(output.contains("集成测试插件"));
    }

    #[test]
    fn framework_generate_report() {
        let fw = MarketFramework::new();
        let plugin = IntegrationTestPlugin::new("报告测试", "test");
        fw.register_plugin(Box::new(plugin)).unwrap();

        let report = fw.generate_report();
        assert_eq!(report.total_plugins, 1);
    }

    #[test]
    fn framework_unregister() {
        let fw = MarketFramework::new();
        fw.register_plugin(Box::new(IntegrationTestPlugin::new("待删除", "test")))
            .unwrap();
        assert_eq!(fw.registry().count(), 1);

        let name = fw.unregister_plugin("integration-test").unwrap();
        assert!(name.contains("集成测试插件"));
        assert_eq!(fw.registry().count(), 0);
    }

    #[test]
    fn prelude_exports_compile() {
        // 验证 prelude 所有导出类型可编译。
        let _set = CapabilitySet::new();
        let _cap = MarketCapability::Spot;
        let _reg = MarketRegistry::new();
        let _evt = MarketEvent::market_registered("id", "name");
        let _meta = MarketMetadata::default();
        let _status = MarketHealthStatus::Healthy;
        let _fw = MarketFramework::new();
        let _err = MarketFrameworkError::Generic {
            detail: "test".into(),
        };
    }

    #[test]
    fn framework_with_auto_discover() {
        let fw = MarketFramework::with_auto_discover(true);
        assert!(fw.registry().is_empty());
    }

    #[test]
    fn full_lifecycle() {
        // 完整生命周期测试：创建 → 注册 → 列表 → 发现 → 报告 → 注销 → 空
        let fw = MarketFramework::new();

        // 注册
        fw.register_plugin(Box::new(IntegrationTestPlugin::new(
            "Polymarket",
            "prediction",
        )))
        .unwrap();
        assert_eq!(fw.registry().count(), 1);

        // 列表
        let summaries = fw.registry().list_all_summaries();
        assert_eq!(summaries.len(), 1);

        // 发现
        let result = fw.discover();
        assert!(result.has_plugins());

        // 报告
        let report = fw.generate_report();
        assert_eq!(report.total_plugins, 1);

        // 渲染
        let output = fw.render_all();
        assert!(output.contains("集成测试插件"));

        // 注销
        fw.unregister_plugin("integration-test").unwrap();
        assert!(fw.registry().is_empty());
    }
}
