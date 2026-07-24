//! 配置中心：统一管理应用、交易所、策略、风控、OMS、Gateway、结算、PMS 配置。
//!
//! 支持 YAML、TOML、环境变量、命令行参数，支持热加载。
//! 所有模块统一通过配置中心读取配置，禁止直接读取配置文件。

pub mod hot_reload;

pub use hot_reload::{HotReloadConfig, HotReloadWatcher, HotReloadable};

use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashMap;

/// 配置来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigSourceType {
    /// YAML 文件
    Yaml,
    /// TOML 文件
    Toml,
    /// 环境变量
    Env,
    /// 命令行参数
    Cli,
    /// 默认值
    Default,
}

impl ConfigSourceType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            ConfigSourceType::Yaml => "YAML",
            ConfigSourceType::Toml => "TOML",
            ConfigSourceType::Env => "环境变量",
            ConfigSourceType::Cli => "命令行",
            ConfigSourceType::Default => "默认值",
        }
    }
}

/// 配置来源描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSource {
    /// 来源类型
    pub source_type: ConfigSourceType,
    /// 文件路径（仅文件类型）
    pub path: Option<String>,
    /// 优先级（数值越小优先级越高）
    pub priority: u32,
}

/// 统一的配置加载 trait
#[async_trait]
pub trait ConfigLoader: Send + Sync {
    /// 加载器名称
    fn name(&self) -> &str;

    /// 加载字符串配置
    async fn load_str(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// 加载整数配置
    async fn load_int(&self, key: &str) -> anyhow::Result<Option<i64>>;

    /// 加载浮点数配置
    async fn load_float(&self, key: &str) -> anyhow::Result<Option<f64>>;

    /// 加载布尔配置
    async fn load_bool(&self, key: &str) -> anyhow::Result<Option<bool>>;

    /// 加载子配置节（反序列化为结构体）
    async fn load_section<T: DeserializeOwned>(&self, section: &str) -> anyhow::Result<Option<T>>;

    /// 加载全部配置
    async fn load_all<T: DeserializeOwned>(&self) -> anyhow::Result<T>;

    /// 重新加载配置（热加载）
    async fn reload(&mut self) -> anyhow::Result<()>;

    /// 所有配置来源
    fn sources(&self) -> &[ConfigSource];
}

/// 优先级链式加载器
///
/// 按优先级合并多个配置来源，优先级数值越小的来源优先。
pub struct ChainLoader {
    name: String,
    sources: Vec<ConfigSource>,
    data: HashMap<String, toml::Value>,
}

impl ChainLoader {
    /// 创建新的链式加载器
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sources: Vec::new(),
            data: HashMap::new(),
        }
    }

    /// 添加 TOML 文件来源
    pub fn add_toml(&mut self, path: &str, priority: u32) -> anyhow::Result<&mut Self> {
        let content = std::fs::read_to_string(path)?;
        let table: toml::Table = toml::from_str(&content)?;
        for (key, value) in table {
            self.data.insert(key, value);
        }
        self.sources.push(ConfigSource {
            source_type: ConfigSourceType::Toml,
            path: Some(path.to_string()),
            priority,
        });
        tracing::info!("添加 TOML 配置来源: {} (优先级 {})", path, priority);
        Ok(self)
    }

    /// 添加环境变量来源
    pub fn add_env(&mut self, prefix: &str, priority: u32) -> anyhow::Result<&mut Self> {
        for (key, value) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix(prefix) {
                let config_key = stripped.to_lowercase().replace('_', ".");
                self.data.insert(config_key, toml::Value::String(value));
            }
        }
        self.sources.push(ConfigSource {
            source_type: ConfigSourceType::Env,
            path: None,
            priority,
        });
        tracing::info!(
            "添加环境变量配置来源: prefix={} (优先级 {})",
            prefix,
            priority
        );
        Ok(self)
    }

    /// 添加默认值
    pub fn add_defaults<T: Serialize>(
        &mut self,
        defaults: &T,
        priority: u32,
    ) -> anyhow::Result<&mut Self> {
        let json = serde_json::to_value(defaults)?;
        if let Some(obj) = json.as_object() {
            for (key, value) in obj {
                let toml_val: toml::Value = serde_json::from_value(value.clone())?;
                self.data.entry(key.clone()).or_insert(toml_val);
            }
        }
        self.sources.push(ConfigSource {
            source_type: ConfigSourceType::Default,
            path: None,
            priority,
        });
        Ok(self)
    }

    /// 获取原始值
    fn get_raw(&self, key: &str) -> Option<&toml::Value> {
        self.data.get(key)
    }
}

#[async_trait]
impl ConfigLoader for ChainLoader {
    fn name(&self) -> &str {
        &self.name
    }

    async fn load_str(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .get_raw(key)
            .and_then(|v| v.as_str().map(|s| s.to_string())))
    }

    async fn load_int(&self, key: &str) -> anyhow::Result<Option<i64>> {
        Ok(self.get_raw(key).and_then(|v| v.as_integer()))
    }

    async fn load_float(&self, key: &str) -> anyhow::Result<Option<f64>> {
        Ok(self.get_raw(key).and_then(|v| v.as_float()))
    }

    async fn load_bool(&self, key: &str) -> anyhow::Result<Option<bool>> {
        Ok(self.get_raw(key).and_then(|v| v.as_bool()))
    }

    async fn load_section<T: DeserializeOwned>(&self, section: &str) -> anyhow::Result<Option<T>> {
        if let Some(raw) = self.get_raw(section) {
            let value: T = toml::Value::try_from(raw.clone())?.try_into()?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    async fn load_all<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let table: toml::Table = self.data.clone().into_iter().collect();
        let value = toml::Value::Table(table);
        Ok(value.try_into()?)
    }

    async fn reload(&mut self) -> anyhow::Result<()> {
        tracing::info!("重新加载配置...");
        for source in &self.sources {
            if let Some(path) = &source.path {
                if let ConfigSourceType::Toml = source.source_type {
                    let content = std::fs::read_to_string(path)?;
                    let table: toml::Table = toml::from_str(&content)?;
                    for (key, value) in table {
                        self.data.insert(key, value);
                    }
                }
            }
        }
        Ok(())
    }

    fn sources(&self) -> &[ConfigSource] {
        &self.sources
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chain_loader_from_defaults() {
        let mut loader = ChainLoader::new("test");
        let defaults = serde_json::json!({
            "app_name": "test-app",
            "max_retries": 3
        });
        loader.add_defaults(&defaults, 100).unwrap();

        let name = loader.load_str("app_name").await.unwrap();
        assert_eq!(name, Some("test-app".to_string()));

        let retries = loader.load_int("max_retries").await.unwrap();
        assert_eq!(retries, Some(3));
    }

    #[tokio::test]
    async fn chain_loader_sources() {
        let mut loader = ChainLoader::new("test");
        loader
            .add_defaults(&serde_json::json!({"key": "val"}), 200)
            .unwrap();
        assert_eq!(loader.sources().len(), 1);
    }

    #[test]
    fn config_source_type_zh() {
        assert_eq!(ConfigSourceType::Toml.as_zh(), "TOML");
        assert_eq!(ConfigSourceType::Env.as_zh(), "环境变量");
        assert_eq!(ConfigSourceType::Cli.as_zh(), "命令行");
    }
}
