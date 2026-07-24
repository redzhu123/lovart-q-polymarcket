//! 插件框架：统一的插件注册和发现机制。
//!
//! # 核心能力
//!
//! - [`Plugin`] trait：统一的插件接口
//! - [`PluginRegistry`]：插件注册中心
//!
//! # 支持的插件类型
//!
//! - Market（市场数据源）
//! - Strategy（交易策略）
//! - Gateway（交易所网关）
//! - Risk（风控规则）
//! - Analytics（分析工具）

use crate::health::HealthStatus;
use async_trait::async_trait;
use std::collections::HashMap;

/// 插件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    /// 市场数据源
    Market,
    /// 交易策略
    Strategy,
    /// 交易所网关
    Gateway,
    /// 风控规则
    Risk,
    /// 分析工具
    Analytics,
    /// 自定义
    Custom,
}

impl PluginType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            PluginType::Market => "市场数据源",
            PluginType::Strategy => "交易策略",
            PluginType::Gateway => "交易所网关",
            PluginType::Risk => "风控规则",
            PluginType::Analytics => "分析工具",
            PluginType::Custom => "自定义",
        }
    }
}

/// 插件信息
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// 插件名称
    pub name: String,
    /// 版本号
    pub version: String,
    /// 插件类型
    pub plugin_type: PluginType,
    /// 描述
    pub description: String,
    /// 作者
    pub author: String,
}

/// 插件 trait
///
/// 所有插件必须实现此接口。
/// 新增 Market、Strategy、Gateway、Risk、Analytics 插件无需修改核心系统。
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 获取插件信息
    fn info(&self) -> PluginInfo;

    /// 初始化插件
    async fn initialize(&mut self) -> anyhow::Result<()>;

    /// 启动插件
    async fn start(&mut self) -> anyhow::Result<()>;

    /// 停止插件
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// 健康检查
    async fn health_check(&self) -> HealthStatus;
}

/// 插件注册中心
///
/// 管理所有已注册的插件，支持按类型查询。
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// 创建新的插件注册中心
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// 注册插件（同名插件会覆盖）
    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> anyhow::Result<()> {
        let info = plugin.info();
        let name = info.name.clone();
        if self.plugins.contains_key(&name) {
            tracing::warn!("插件已存在，将被覆盖: {}", name);
        }
        tracing::info!(
            "注册插件: {} v{} ({})",
            name,
            info.version,
            info.plugin_type.as_zh()
        );
        self.plugins.insert(name, plugin);
        Ok(())
    }

    /// 注销插件
    pub fn unregister(&mut self, name: &str) -> anyhow::Result<()> {
        if self.plugins.remove(name).is_some() {
            tracing::info!("注销插件: {}", name);
        }
        Ok(())
    }

    /// 获取插件
    pub fn get(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins.get(name).map(|p| p.as_ref())
    }

    /// 按类型列出插件
    pub fn list_by_type(&self, plugin_type: PluginType) -> Vec<&dyn Plugin> {
        self.plugins
            .values()
            .filter(|p| p.info().plugin_type == plugin_type)
            .map(|p| p.as_ref())
            .collect()
    }

    /// 列出所有插件信息
    pub fn list_all(&self) -> Vec<PluginInfo> {
        self.plugins.values().map(|p| p.info()).collect()
    }

    /// 插件数量
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin {
        info: PluginInfo,
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn info(&self) -> PluginInfo {
            self.info.clone()
        }

        async fn initialize(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn start(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    fn make_plugin(name: &str, ptype: PluginType) -> TestPlugin {
        TestPlugin {
            info: PluginInfo {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                plugin_type: ptype,
                description: format!("Test plugin {}", name),
                author: "test".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn plugin_registry_register_and_list() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(make_plugin("market-a", PluginType::Market)))
            .unwrap();
        reg.register(Box::new(make_plugin("strategy-b", PluginType::Strategy)))
            .unwrap();

        assert_eq!(reg.count(), 2);
        let all = reg.list_all();
        assert_eq!(all.len(), 2);

        let markets = reg.list_by_type(PluginType::Market);
        assert_eq!(markets.len(), 1);
    }

    #[tokio::test]
    async fn plugin_registry_unregister() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(make_plugin("test", PluginType::Custom)))
            .unwrap();
        assert_eq!(reg.count(), 1);

        reg.unregister("test").unwrap();
        assert_eq!(reg.count(), 0);
    }

    #[tokio::test]
    async fn plugin_registry_duplicate_override() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(make_plugin("dup", PluginType::Market)))
            .unwrap();
        reg.register(Box::new(make_plugin("dup", PluginType::Risk)))
            .unwrap();
        // 不应 panic，应覆盖
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn plugin_type_zh() {
        assert_eq!(PluginType::Market.as_zh(), "市场数据源");
        assert_eq!(PluginType::Strategy.as_zh(), "交易策略");
    }
}
