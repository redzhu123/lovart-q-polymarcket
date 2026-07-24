//! 配置热加载支持。
//!
//! 监控配置文件变更，通知订阅者。

use async_trait::async_trait;

/// 热加载配置
#[derive(Debug, Clone)]
pub struct HotReloadConfig {
    /// 是否启用热加载
    pub enabled: bool,
    /// 监控的文件路径列表
    pub watch_paths: Vec<String>,
    /// 检查间隔（秒）
    pub check_interval_secs: u64,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            watch_paths: vec![],
            check_interval_secs: 30,
        }
    }
}

/// 可热加载的组件
#[async_trait]
pub trait HotReloadable: Send + Sync {
    /// 组件名称
    fn name(&self) -> &str;

    /// 配置变更回调
    async fn on_config_changed(
        &self,
        key: &str,
        old_value: &str,
        new_value: &str,
    ) -> anyhow::Result<()>;
}

/// 热加载监控器
pub struct HotReloadWatcher {
    config: HotReloadConfig,
    listeners: Vec<Box<dyn HotReloadable>>,
}

impl HotReloadWatcher {
    /// 创建新的热加载监控器
    pub fn new(config: HotReloadConfig) -> Self {
        Self {
            config,
            listeners: Vec::new(),
        }
    }

    /// 注册热加载监听器
    pub fn subscribe(&mut self, listener: Box<dyn HotReloadable>) {
        tracing::info!("注册热加载监听器: {}", listener.name());
        self.listeners.push(listener);
    }

    /// 启动热加载监控
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("热加载未启用");
            return Ok(());
        }
        tracing::info!(
            "热加载监控启动: 监控 {} 个路径, 间隔 {}秒",
            self.config.watch_paths.len(),
            self.config.check_interval_secs
        );
        // 预留：实际的文件监控循环
        Ok(())
    }

    /// 停止热加载监控
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        tracing::info!("热加载监控停止");
        Ok(())
    }

    /// 监听器数量
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestListener {
        name: String,
    }

    #[async_trait]
    impl HotReloadable for TestListener {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_config_changed(
            &self,
            _key: &str,
            _old_value: &str,
            _new_value: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn hot_reload_watcher_subscribe_and_start() {
        let config = HotReloadConfig {
            enabled: true,
            watch_paths: vec!["config.toml".to_string()],
            check_interval_secs: 30,
        };
        let mut watcher = HotReloadWatcher::new(config);
        let listener = TestListener {
            name: "test-listener".to_string(),
        };
        watcher.subscribe(Box::new(listener));
        assert_eq!(watcher.listener_count(), 1);
        watcher.start().await.unwrap();
        watcher.stop().await.unwrap();
    }
}
