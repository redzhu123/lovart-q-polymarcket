//! MarketPlugin Trait（P3.0 第二节）。
//!
//! 统一定义所有市场的接口。
//! 未来 Polymarket / Kalshi / Binance / OKX / Bybit / Hyperliquid / Uniswap / Raydium
//! 全部实现该 Trait。
//!
//! # 设计约束
//!
//! - 新增市场不得修改核心系统（Strategy / Risk / OMS / Settlement / PMS / Infrastructure）
//! - 仅需新增 provider + adapter + gateway，然后实现 [`MarketPlugin`]
//! - 实现者必须通过 [`MarketRegistry`](crate::registry::MarketRegistry) 注册

use async_trait::async_trait;

use crate::adapter::MarketAdapter;
use crate::capability::CapabilitySet;
use crate::error::MarketFrameworkResult;
use crate::health::MarketHealthReport;
use crate::metadata::MarketMetadata;
use crate::provider::MarketDataProvider;

// ============================================================================
// MarketPlugin Trait
// ============================================================================

/// 市场插件 Trait（P3.0 第二节）。
///
/// 统一定义所有市场的接口。每个市场插件封装了以下组件：
///
/// - **标识**：`id()` / `name()` / `market_type()`
/// - **能力**：`supported_features()`
/// - **供应商**：`provider()`
/// - **网关**：`gateway_name()`
/// - **适配器**：`adapter()`
/// - **元数据**：`metadata()`
/// - **健康检查**：`health()`
///
/// # 实现指南
///
/// ```ignore
/// struct PolymarketPlugin { ... }
///
/// #[async_trait]
/// impl MarketPlugin for PolymarketPlugin {
///     fn id(&self) -> &str { "polymarket-v1" }
///     fn name(&self) -> &str { "Polymarket" }
///     fn market_type(&self) -> MarketType { MarketType::Prediction }
///     // ...
/// }
/// ```
///
/// # 安全性
///
/// - `health()` 不得产生副作用
/// - `provider()` 和 `adapter()` 返回共享引用
/// - 所有方法必须线程安全（`Send + Sync`）
#[async_trait]
pub trait MarketPlugin: Send + Sync {
    // ===== 标识 =====

    /// 插件唯一 ID（如 "polymarket-v1"）。
    ///
    /// 用于注册、注销、查找。不得重复。
    fn id(&self) -> &str;

    /// 插件中文名称（如 "Polymarket 预测市场"）。
    fn name(&self) -> &str;

    /// 版本号。
    fn version(&self) -> &str {
        "1.0.0"
    }

    /// 市场类型代码（如 "polymarket", "binance", "okx"）。
    fn market_type_code(&self) -> &str;

    /// 插件描述。
    fn description(&self) -> &str {
        "市场插件"
    }

    // ===== 能力 =====

    /// 声明该市场支持的所有能力。
    ///
    /// 系统根据此能力集自动启用功能。
    /// **禁止写死布尔值** — 必须通过此方法返回能力集。
    fn supported_features(&self) -> &CapabilitySet;

    /// 检查是否支持某个特定能力。
    fn has_capability(&self, cap: &crate::capability::MarketCapability) -> bool {
        self.supported_features().has(cap)
    }

    // ===== 供应商（数据层）=====

    /// 获取市场数据供应商。
    ///
    /// 用于读取行情、订单簿、成交记录等。
    /// None 表示该市场不需要数据供应商。
    fn provider(&self) -> Option<&dyn MarketDataProvider> {
        None
    }

    // ===== 网关（执行层）=====

    /// 网关名称（如 "polymarket", "binance", "mock"）。
    ///
    /// 供 Execution 创建对应的 ExchangeGateway。
    fn gateway_name(&self) -> &str;

    /// 是否启用真实交易。
    fn live_enabled(&self) -> bool {
        false
    }

    // ===== 适配器 =====

    /// 获取市场适配器。
    ///
    /// 用于市场特定数据格式转换。
    /// None 表示该市场不需要特殊适配。
    fn adapter(&self) -> Option<&dyn MarketAdapter> {
        None
    }

    // ===== 元数据 =====

    /// 获取市场元数据。
    fn metadata(&self) -> &MarketMetadata;

    // ===== 生命周期 =====

    /// 初始化插件。
    ///
    /// 在注册后、首次使用前调用。
    /// 实现者应在此处完成：
    /// - 连接建立
    /// - 认证初始化
    /// - 资源分配
    async fn initialize(&mut self) -> MarketFrameworkResult<()> {
        let start = std::time::Instant::now();
        tracing::info!("[{}] 插件初始化开始", self.name());
        // 默认实现：无操作
        let elapsed_ms = start.elapsed().as_millis();
        tracing::info!("[{}] 插件初始化完成（{}ms）", self.name(), elapsed_ms);
        Ok(())
    }

    /// 启动插件。
    ///
    /// 使插件进入可工作状态。
    async fn start(&mut self) -> MarketFrameworkResult<()> {
        tracing::info!("[{}] 插件启动", self.name());
        Ok(())
    }

    /// 停止插件。
    ///
    /// 优雅关闭，释放资源。
    async fn stop(&mut self) -> MarketFrameworkResult<()> {
        tracing::info!("[{}] 插件停止", self.name());
        Ok(())
    }

    /// 关闭插件。
    ///
    /// 强制关闭，不保证优雅。
    async fn shutdown(&mut self) -> MarketFrameworkResult<()> {
        tracing::info!("[{}] 插件关闭", self.name());
        Ok(())
    }

    // ===== 健康检查 =====

    /// 执行完整健康检查。
    ///
    /// 检查内容：
    /// - REST 连接
    /// - WebSocket 连接
    /// - 网关状态
    /// - 认证状态
    /// - 延迟
    /// - 流数据
    ///
    /// 返回中文健康报告。
    async fn health(&self) -> MarketHealthReport;

    /// 快速 Ping 检查。
    async fn ping(&self) -> bool {
        self.health().await.overall_healthy()
    }

    // ===== 统计 =====

    /// 插件信息摘要（中文）。
    fn info_summary_zh(&self) -> String {
        let lines = vec![
            format!("【{}】v{}", self.name(), self.version()),
            format!("  ID: {}", self.id()),
            format!("  类型代码: {}", self.market_type_code()),
            format!("  描述: {}", self.description()),
            format!(
                "  可交易: {}",
                if self.live_enabled() { "✅" } else { "❌" }
            ),
            format!("  网关: {}", self.gateway_name()),
            String::new(),
            self.supported_features().render_table("支持的能力"),
        ];
        lines.join("\n")
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilitySet, MarketCapability};
    use crate::metadata::MarketMetadata;

    /// 测试用 MarketPlugin 实现。
    struct TestPlugin {
        metadata: MarketMetadata,
        capabilities: CapabilitySet,
    }

    impl TestPlugin {
        fn new() -> Self {
            Self {
                metadata: MarketMetadata::prediction_market("TestExchange", "TEST"),
                capabilities: CapabilitySet::prediction_market_full(),
            }
        }
    }

    #[async_trait]
    impl MarketPlugin for TestPlugin {
        fn id(&self) -> &str {
            "test-plugin-v1"
        }

        fn name(&self) -> &str {
            "测试插件"
        }

        fn market_type_code(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "测试用市场插件"
        }

        fn supported_features(&self) -> &CapabilitySet {
            &self.capabilities
        }

        fn gateway_name(&self) -> &str {
            "test-gateway"
        }

        fn metadata(&self) -> &MarketMetadata {
            &self.metadata
        }

        async fn health(&self) -> MarketHealthReport {
            crate::health::MarketHealthReport::healthy("测试插件")
        }
    }

    #[tokio::test]
    async fn plugin_info_summary_zh() {
        let plugin = TestPlugin::new();
        let summary = plugin.info_summary_zh();
        assert!(summary.contains("测试插件"));
        assert!(summary.contains("test-plugin-v1"));
        assert!(summary.contains("test-gateway"));
        assert!(summary.contains("预测市场"));
    }

    #[tokio::test]
    async fn plugin_health_returns_healthy() {
        let plugin = TestPlugin::new();
        let report = plugin.health().await;
        assert!(report.overall_healthy());
    }

    #[tokio::test]
    async fn plugin_ping() {
        let plugin = TestPlugin::new();
        assert!(plugin.ping().await);
    }

    #[tokio::test]
    async fn plugin_default_live_disabled() {
        let plugin = TestPlugin::new();
        assert!(!plugin.live_enabled());
    }

    #[tokio::test]
    async fn plugin_has_capability() {
        let plugin = TestPlugin::new();
        assert!(plugin.has_capability(&MarketCapability::Prediction));
        assert!(plugin.has_capability(&MarketCapability::LiveTrading));
        assert!(!plugin.has_capability(&MarketCapability::Spot));
    }

    #[tokio::test]
    async fn plugin_initialize_succeeds() {
        let mut plugin = TestPlugin::new();
        let result = plugin.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn plugin_lifecycle_start_stop() {
        let mut plugin = TestPlugin::new();
        assert!(plugin.start().await.is_ok());
        assert!(plugin.stop().await.is_ok());
    }
}
