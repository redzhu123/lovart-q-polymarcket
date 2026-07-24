//! 依赖注入容器：简单的服务注册和查找。
//!
//! 不依赖 proc macro 或反射，使用 &'static str 作为服务键。

use crate::lifecycle::Lifecycle;
use std::any::Any;
use std::collections::HashMap;

/// 服务键类型
type ServiceKey = &'static str;

/// 简单的依赖注入容器
///
/// 用字符串键注册和检索服务，支持初始化/关闭排序。
///
/// # 示例
///
/// ```ignore
/// let mut di = DiContainer::new();
/// di.register("event_bus", EventBus::new())?;
/// let bus: &EventBus = di.get("event_bus").unwrap();
/// ```
pub struct DiContainer {
    services: HashMap<ServiceKey, Box<dyn Any + Send + Sync>>,
    init_order: Vec<ServiceKey>,
    shutdown_order: Vec<ServiceKey>,
}

impl DiContainer {
    /// 创建空的 DI 容器
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            init_order: Vec::new(),
            shutdown_order: Vec::new(),
        }
    }

    /// 注册服务
    ///
    /// # Panics
    ///
    /// 如果服务键已存在则 panic。
    pub fn register<T: Send + Sync + 'static>(
        &mut self,
        key: ServiceKey,
        service: T,
    ) -> anyhow::Result<()> {
        if self.services.contains_key(key) {
            anyhow::bail!("服务键已存在: {}", key);
        }
        tracing::info!("DI 容器注册服务: {}", key);
        self.services.insert(key, Box::new(service));
        Ok(())
    }

    /// 注册服务并指定初始化顺序
    pub fn register_with_init_order<T: Send + Sync + 'static>(
        &mut self,
        key: ServiceKey,
        service: T,
    ) -> anyhow::Result<()> {
        self.register(key, service)?;
        self.init_order.push(key);
        Ok(())
    }

    /// 设置初始化顺序
    pub fn with_init_order(&mut self, order: &[ServiceKey]) -> &mut Self {
        self.init_order = order.to_vec();
        self
    }

    /// 设置关闭顺序
    pub fn with_shutdown_order(&mut self, order: &[ServiceKey]) -> &mut Self {
        self.shutdown_order = order.to_vec();
        self
    }

    /// 获取服务引用
    pub fn get<T: Send + Sync + 'static>(&self, key: ServiceKey) -> Option<&T> {
        self.services.get(key).and_then(|s| s.downcast_ref::<T>())
    }

    /// 获取服务可变引用
    pub fn get_mut<T: Send + Sync + 'static>(&mut self, key: ServiceKey) -> Option<&mut T> {
        self.services
            .get_mut(key)
            .and_then(|s| s.downcast_mut::<T>())
    }

    /// 检查服务是否存在
    pub fn has(&self, key: ServiceKey) -> bool {
        self.services.contains_key(key)
    }

    /// 移除服务
    pub fn remove(&mut self, key: ServiceKey) -> Option<Box<dyn Any + Send + Sync>> {
        self.services.remove(key)
    }

    /// 服务数量
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// 按初始化顺序初始化所有 Lifecycle 服务
    ///
    /// 如果注册了初始化顺序，按顺序初始化；否则按注册顺序。
    pub async fn initialize_all(&mut self) -> anyhow::Result<()> {
        let order: Vec<ServiceKey> = if self.init_order.is_empty() {
            self.services.keys().copied().collect()
        } else {
            self.init_order.clone()
        };

        for key in order {
            if let Some(service) = self.services.get_mut(key) {
                if let Some(lifecycle) = service.downcast_mut::<Box<dyn Lifecycle>>() {
                    tracing::info!("DI 容器初始化: {}", key);
                    lifecycle.initialize().await?;
                }
            }
        }
        Ok(())
    }

    /// 按关闭顺序关闭所有 Lifecycle 服务（逆序）
    pub async fn shutdown_all(&mut self) -> anyhow::Result<()> {
        let order: Vec<ServiceKey> = if self.shutdown_order.is_empty() {
            let mut keys: Vec<ServiceKey> = self.services.keys().copied().collect();
            keys.reverse();
            keys
        } else {
            let mut reversed = self.shutdown_order.clone();
            reversed.reverse();
            reversed
        };

        for key in order {
            if let Some(service) = self.services.get_mut(key) {
                if let Some(lifecycle) = service.downcast_mut::<Box<dyn Lifecycle>>() {
                    tracing::info!("DI 容器关闭: {}", key);
                    if let Err(e) = lifecycle.shutdown().await {
                        tracing::warn!("DI 容器关闭 {} 失败: {}", key, e);
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for DiContainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn di_container_register_and_get() {
        let mut di = DiContainer::new();
        di.register("counter", 42u64).unwrap();
        di.register("name", "hello".to_string()).unwrap();

        assert!(di.has("counter"));
        assert!(!di.has("nonexistent"));

        let val: &u64 = di.get("counter").unwrap();
        assert_eq!(*val, 42);

        let name: &String = di.get("name").unwrap();
        assert_eq!(name, "hello");
    }

    #[test]
    fn di_container_duplicate_panics() {
        let mut di = DiContainer::new();
        di.register("key", 1u64).unwrap();
        let result = di.register("key", 2u64);
        assert!(result.is_err());
    }

    #[test]
    fn di_container_remove() {
        let mut di = DiContainer::new();
        di.register("key", 100u64).unwrap();
        assert_eq!(di.len(), 1);

        let removed = di.remove("key");
        assert!(removed.is_some());
        assert_eq!(di.len(), 0);
        assert!(!di.has("key"));
    }

    #[test]
    fn di_container_get_missing_returns_none() {
        let di = DiContainer::new();
        let val: Option<&u64> = di.get("missing");
        assert!(val.is_none());
    }

    #[test]
    fn di_container_len() {
        let mut di = DiContainer::new();
        assert!(di.is_empty());
        di.register("a", 1u64).unwrap();
        di.register("b", 2u64).unwrap();
        assert_eq!(di.len(), 2);
    }
}
