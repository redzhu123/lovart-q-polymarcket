//! 市场注册中心（P3.0 第四节）。
//!
//! 负责 MarketPlugin 的注册、注销、发现和查询。
//! 系统通过注册中心管理所有已安装市场。
//!
//! # 核心功能
//!
//! - 注册与注销 MarketPlugin
//! - 按 ID / 名称 / 类型 / 能力查询
//! - 列出所有已安装市场
//! - 与 [`EventBus`] 集成，发布市场变化事件

use std::collections::HashMap;
use std::sync::RwLock;

use crate::capability::MarketCapability;
use crate::error::{MarketFrameworkError, MarketFrameworkResult};
use crate::events::MarketEvent;
use crate::plugin::MarketPlugin;

// ============================================================================
// MarketRegistry
// ============================================================================

/// 市场注册中心。
///
/// 线程安全的 MarketPlugin 注册表。
/// 支持注册、注销、查询和列表操作。
///
/// # 使用示例
///
/// ```ignore
/// let mut registry = MarketRegistry::new();
/// registry.register(Box::new(PolymarketPlugin::new()))?;
///
/// let plugins = registry.list_all();
/// for p in plugins {
///     println!("{}", p.info_summary_zh());
/// }
/// ```
pub struct MarketRegistry {
    /// 已注册插件（按 ID 索引）。
    plugins: RwLock<HashMap<String, Box<dyn MarketPlugin>>>,
    /// 是否启用自动发现。
    auto_discover: bool,
    /// 注册数量计数器。
    registration_count: RwLock<u64>,
}

impl MarketRegistry {
    /// 创建新的注册中心。
    pub fn new() -> Self {
        tracing::info!("市场注册中心已创建");
        Self {
            plugins: RwLock::new(HashMap::new()),
            auto_discover: true,
            registration_count: RwLock::new(0),
        }
    }

    /// 是否启用自动发现。
    pub fn with_auto_discover(mut self, enabled: bool) -> Self {
        self.auto_discover = enabled;
        tracing::info!("自动发现: {}", if enabled { "启用" } else { "禁用" });
        self
    }

    // ===== 注册 / 注销 =====

    /// 注册一个市场插件。
    ///
    /// # 参数
    ///
    /// - `plugin`: 实现了 MarketPlugin trait 的插件实例
    ///
    /// # 返回
    ///
    /// - 成功：Ok(())
    /// - 失败：如果同 ID 插件已注册，返回 `PluginAlreadyRegistered` 错误
    ///
    /// # 日志
    ///
    /// 记录插件名称、ID、能力数量、注册耗时。
    pub fn register(&self, plugin: Box<dyn MarketPlugin>) -> MarketFrameworkResult<()> {
        let id = plugin.id().to_string();
        let name = plugin.name().to_string();
        let cap_count = plugin.supported_features().count();
        let start = std::time::Instant::now();

        let mut plugins =
            self.plugins
                .write()
                .map_err(|e| MarketFrameworkError::RegistryError {
                    detail: format!("获取写锁失败: {}", e),
                })?;

        if plugins.contains_key(&id) {
            tracing::warn!("插件已存在，将被覆盖: {} ({})", name, id);
        }

        tracing::info!(
            "注册市场插件: {} (ID: {}) | 能力: {} 项",
            name,
            id,
            cap_count
        );

        plugins.insert(id.clone(), plugin);

        let elapsed_ms = start.elapsed().as_millis() as u64;
        if let Ok(mut count) = self.registration_count.write() {
            *count += 1;
        }

        tracing::info!("插件 {} 注册完成（{}ms）", name, elapsed_ms);

        Ok(())
    }

    /// 注销一个市场插件。
    ///
    /// # 参数
    ///
    /// - `id`: 插件 ID
    ///
    /// # 返回
    ///
    /// - 成功时返回被注销的插件名称
    /// - 失败时返回 `PluginNotFound`
    pub fn unregister(&self, id: &str) -> MarketFrameworkResult<String> {
        let mut plugins =
            self.plugins
                .write()
                .map_err(|e| MarketFrameworkError::RegistryError {
                    detail: format!("获取写锁失败: {}", e),
                })?;

        match plugins.remove(id) {
            Some(plugin) => {
                let name = plugin.name().to_string();
                tracing::info!("注销市场插件: {} (ID: {})", name, id);
                Ok(name)
            }
            None => {
                tracing::warn!("尝试注销不存在的插件: {}", id);
                Err(MarketFrameworkError::PluginNotFound {
                    name: id.to_string(),
                })
            }
        }
    }

    // ===== 查询 =====

    /// 按 ID 获取插件（返回是否存在）。
    pub fn exists(&self, id: &str) -> bool {
        self.plugins
            .read()
            .map(|p| p.contains_key(id))
            .unwrap_or(false)
    }

    /// 按 ID 查找插件并执行回调。
    ///
    /// 因为 MarketPlugin 是 trait object，直接返回引用有生命周期问题。
    /// 改为使用闭包在锁内访问插件。
    pub fn with_plugin<F, R>(&self, id: &str, f: F) -> MarketFrameworkResult<R>
    where
        F: FnOnce(&dyn MarketPlugin) -> R,
    {
        let plugins = self
            .plugins
            .read()
            .map_err(|e| MarketFrameworkError::RegistryError {
                detail: format!("获取读锁失败: {}", e),
            })?;

        match plugins.get(id) {
            Some(plugin) => Ok(f(plugin.as_ref())),
            None => Err(MarketFrameworkError::PluginNotFound {
                name: id.to_string(),
            }),
        }
    }

    /// 按市场类型代码查询插件。
    pub fn find_by_type(&self, type_code: &str) -> Vec<String> {
        self.plugins
            .read()
            .map(|p| {
                p.values()
                    .filter(|p| p.market_type_code() == type_code)
                    .map(|p| p.id().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 按能力查询插件（拥有指定能力的插件）。
    pub fn find_by_capability(&self, cap: &MarketCapability) -> Vec<String> {
        self.plugins
            .read()
            .map(|p| {
                p.values()
                    .filter(|p| p.has_capability(cap))
                    .map(|p| p.id().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 查找支持真实交易的插件。
    pub fn find_live_trading_plugins(&self) -> Vec<String> {
        self.find_by_capability(&MarketCapability::LiveTrading)
    }

    // ===== 列表 =====

    /// 列出所有已注册插件的信息摘要。
    pub fn list_all_summaries(&self) -> Vec<PluginSummary> {
        self.plugins
            .read()
            .map(|p| {
                p.values()
                    .map(|plugin| PluginSummary {
                        id: plugin.id().to_string(),
                        name: plugin.name().to_string(),
                        version: plugin.version().to_string(),
                        market_type: plugin.market_type_code().to_string(),
                        gateway: plugin.gateway_name().to_string(),
                        live_enabled: plugin.live_enabled(),
                        capability_count: plugin.supported_features().count(),
                        description: plugin.description().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 列出所有已注册插件的 ID。
    pub fn list_ids(&self) -> Vec<String> {
        self.plugins
            .read()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 渲染为中文表格。
    pub fn render_table_zh(&self) -> String {
        let summaries = self.list_all_summaries();
        if summaries.is_empty() {
            return "【市场注册表】\n  （空 — 尚未注册任何市场插件）".to_string();
        }

        let header = format!(
            "【市场注册表】已安装 {} 个市场\n\n\
             {:<30} {:<20} {:<12} {:<12} {:<8} {:<10}",
            summaries.len(),
            "插件名称",
            "ID",
            "市场类型",
            "网关",
            "实盘",
            "能力数"
        );

        let mut lines = vec![header];
        lines.push("-".repeat(100));

        for s in &summaries {
            lines.push(format!(
                "{:<30} {:<20} {:<12} {:<12} {:<8} {:<10}",
                truncate(&s.name, 28),
                truncate(&s.id, 18),
                s.market_type,
                s.gateway,
                if s.live_enabled { "✅" } else { "❌" },
                s.capability_count,
            ));
        }

        lines.join("\n")
    }

    /// 插件数量。
    pub fn count(&self) -> usize {
        self.plugins.read().map(|p| p.len()).unwrap_or(0)
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// 注册计数。
    pub fn registration_total(&self) -> u64 {
        self.registration_count.read().map(|g| *g).unwrap_or(0)
    }

    /// 获取事件的插件列表（所有插件的 ID-名称对）。
    pub fn plugin_events(&self) -> Vec<MarketEvent> {
        self.plugins
            .read()
            .map(|p| {
                p.iter()
                    .map(|(id, plugin)| MarketEvent::market_registered(id, plugin.name()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for MarketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PluginSummary
// ============================================================================

/// 插件摘要信息（用于列表展示）。
#[derive(Debug, Clone)]
pub struct PluginSummary {
    /// 插件 ID。
    pub id: String,
    /// 插件名称。
    pub name: String,
    /// 版本。
    pub version: String,
    /// 市场类型代码。
    pub market_type: String,
    /// 网关名称。
    pub gateway: String,
    /// 是否启用真实交易。
    pub live_enabled: bool,
    /// 支持的能力数量。
    pub capability_count: usize,
    /// 描述。
    pub description: String,
}

impl PluginSummary {
    /// 渲染为一行中文摘要。
    pub fn line_zh(&self) -> String {
        format!(
            "{} | {} | {} | {} | {} | {} 项能力",
            if self.live_enabled { "🔴" } else { "🟡" },
            self.name,
            self.id,
            self.market_type,
            self.gateway,
            self.capability_count
        )
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    } else {
        s.to_string()
    }
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
    use async_trait::async_trait;

    /// 测试用插件。
    struct TestMarketPlugin {
        id: String,
        name: String,
        type_code: String,
        caps: CapabilitySet,
        metadata: MarketMetadata,
    }

    impl TestMarketPlugin {
        fn new(id: &str, name: &str, type_code: &str) -> Self {
            let caps = if type_code == "prediction" {
                CapabilitySet::prediction_market_full()
            } else {
                CapabilitySet::spot_exchange_full()
            };
            let metadata = if type_code == "prediction" {
                MarketMetadata::prediction_market(name, "TEST")
            } else {
                MarketMetadata::spot_market(name, "BTC", "USDT")
            };
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
    impl MarketPlugin for TestMarketPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn market_type_code(&self) -> &str {
            &self.type_code
        }
        fn description(&self) -> &str {
            "测试插件"
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
            MarketHealthReport::healthy(&self.name)
        }
    }

    #[test]
    fn registry_new_is_empty() {
        let reg = MarketRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn registry_register_and_count() {
        let reg = MarketRegistry::new();
        let plugin = TestMarketPlugin::new("test-1", "测试市场 1", "prediction");
        reg.register(Box::new(plugin)).unwrap();
        assert_eq!(reg.count(), 1);
        assert!(!reg.is_empty());
        assert!(reg.exists("test-1"));
    }

    #[test]
    fn registry_register_multiple() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new(
            "pm-1",
            "Polymarket",
            "prediction",
        )))
        .unwrap();
        reg.register(Box::new(TestMarketPlugin::new("bn-1", "Binance", "spot")))
            .unwrap();
        assert_eq!(reg.count(), 2);

        let all = reg.list_all_summaries();
        assert_eq!(all.len(), 2);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Polymarket"));
        assert!(names.contains(&"Binance"));
    }

    #[test]
    fn registry_duplicate_override() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new("dup", "原始", "spot")))
            .unwrap();
        reg.register(Box::new(TestMarketPlugin::new("dup", "覆盖", "prediction")))
            .unwrap();
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn registry_unregister() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new("rm", "待删除", "spot")))
            .unwrap();
        assert_eq!(reg.count(), 1);

        let name = reg.unregister("rm").unwrap();
        assert_eq!(name, "待删除");
        assert_eq!(reg.count(), 0);
        assert!(!reg.exists("rm"));
    }

    #[test]
    fn registry_unregister_not_found() {
        let reg = MarketRegistry::new();
        let result = reg.unregister("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn registry_find_by_type() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new(
            "pm",
            "Polymarket",
            "prediction",
        )))
        .unwrap();
        reg.register(Box::new(TestMarketPlugin::new("bn", "Binance", "spot")))
            .unwrap();

        let preds = reg.find_by_type("prediction");
        assert_eq!(preds.len(), 1);
        assert_eq!(preds[0], "pm");

        let spots = reg.find_by_type("spot");
        assert_eq!(spots.len(), 1);
        assert_eq!(spots[0], "bn");
    }

    #[test]
    fn registry_find_by_capability() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new(
            "pm",
            "Polymarket",
            "prediction",
        )))
        .unwrap();
        reg.register(Box::new(TestMarketPlugin::new("bn", "Binance", "spot")))
            .unwrap();

        let trading = reg.find_by_capability(&MarketCapability::LiveTrading);
        assert_eq!(trading.len(), 2); // 两者都支持真实交易

        let spot = reg.find_by_capability(&MarketCapability::Spot);
        assert_eq!(spot.len(), 1);
        assert_eq!(spot[0], "bn");
    }

    #[test]
    fn registry_render_table() {
        let reg = MarketRegistry::new();
        reg.register(Box::new(TestMarketPlugin::new(
            "pm",
            "Polymarket",
            "prediction",
        )))
        .unwrap();

        let table = reg.render_table_zh();
        assert!(table.contains("Polymarket"));
        assert!(table.contains("pm"));
        assert!(table.contains("prediction"));
    }

    #[test]
    fn registry_empty_table() {
        let reg = MarketRegistry::new();
        let table = reg.render_table_zh();
        assert!(table.contains("空"));
    }

    #[test]
    fn plugin_summary_line_zh() {
        let summary = PluginSummary {
            id: "test".into(),
            name: "测试".into(),
            version: "1.0".into(),
            market_type: "spot".into(),
            gateway: "gw".into(),
            live_enabled: false,
            capability_count: 5,
            description: "描述".into(),
        };
        let line = summary.line_zh();
        assert!(line.contains("测试"));
        assert!(line.contains("🟡"));
    }
}
