//! pm-infrastructure：统一交易基础设施层（P2-07）
//!
//! 本 crate 提取并统一分散在各业务 crate 中的基础设施能力，
//! 包括：配置中心、认证框架、密钥管理、缓存框架、存储框架、
//! 任务调度、插件框架、事件总线、健康中心、生命周期管理、
//! 指标收集、分布式追踪、诊断工具、依赖注入。
//!
//! # 架构约束
//!
//! - pm-infrastructure 仅依赖 pm-core 和外部 crate（tokio, serde 等）
//! - pm-infrastructure 不得依赖任何业务 crate（pm-gateway, pm-oms 等）
//! - 所有业务模块应统一依赖 pm-infrastructure，不得自行实现基础设施能力
//!
//! # 日志规范
//!
//! 所有日志使用中文，统一通过 tracing 输出，支持结构化日志。
//! 日志初始化通过 [`init_logging`] 函数完成，默认环境变量过滤器为 `PM_INFRA_LOG`。

pub mod authentication;
pub mod cache;
pub mod configuration;
pub mod dependency;
pub mod diagnostics;
pub mod event_bus;
pub mod health;
pub mod lifecycle;
pub mod metrics;
pub mod plugin;
pub mod scheduler;
pub mod secret;
pub mod storage;
pub mod trace;

/// 预导入模块：集中导出最常用的类型
pub mod prelude {
    pub use crate::authentication::session::{Session, SessionManager};
    pub use crate::authentication::signer::{NoopSigner, SignRequest, SignResponse, Signer};
    pub use crate::authentication::{
        AuthHealth, AuthMiddleware, AuthenticationProvider, MockAuthProvider,
    };
    pub use crate::cache::{Cache, CacheStats, LruCache, MemoryCache, TtlCache};
    pub use crate::configuration::{
        ChainLoader, ConfigLoader, ConfigSource, ConfigSourceType, HotReloadConfig,
        HotReloadWatcher, HotReloadable,
    };
    pub use crate::dependency::DiContainer;
    pub use crate::diagnostics::{
        Diagnosable, DiagnosticItem, DiagnosticReport, DiagnosticsCenter,
    };
    pub use crate::event_bus::{EVENT_CSV_HEADER, EventBus, Subscriber, SystemEvent};
    pub use crate::health::{
        HealthCenter, HealthCheck, HealthCheckable, HealthReport, HealthStatus,
    };
    pub use crate::lifecycle::{Lifecycle, LifecycleManager, LifecycleState};
    pub use crate::metrics::{
        Counter, Gauge, Histogram, InfrastructureMetrics, MetricsCollector, MetricsSnapshot,
    };
    pub use crate::plugin::{Plugin, PluginInfo, PluginRegistry, PluginType};
    pub use crate::scheduler::{
        Backoff, CircuitBreaker, CircuitState, IntervalScheduler, RetryError, RetryExecutor,
        ScheduleKind, ScheduledTask, Scheduler, SchedulerStats, Task, TaskResult, TokenBucket,
    };
    pub use crate::secret::mask::{
        mask_address, mask_api_key, mask_passphrase, mask_private_key, mask_secret,
    };
    pub use crate::secret::sensitive::SensitiveString;
    pub use crate::secret::{Credential, CredentialSource, EnvSecretManager, SecretManager};
    pub use crate::storage::{
        CsvStorage, MemoryStorage, SqliteStorage, Storage, StorageHealth, StorageKind,
        create_storage,
    };
    pub use crate::trace::{
        CorrelationId, RequestId, TracingConfig, init_default_tracing, init_tracing,
    };
}

/// 初始化基础设施层日志
///
/// 使用环境变量 `PM_INFRA_LOG` 控制日志级别，默认为 `info`。
/// 第三方库日志默认静默。
pub fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;

    let env_filter = EnvFilter::try_from_env("PM_INFRA_LOG")
        .unwrap_or_else(|_| EnvFilter::new(level))
        .add_directive("hyper=warn".parse().unwrap())
        .add_directive("hyper_util=warn".parse().unwrap())
        .add_directive("reqwest=warn".parse().unwrap())
        .add_directive("tower=warn".parse().unwrap())
        .add_directive("rustls=warn".parse().unwrap());

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();

    tracing::info!("基础设施层日志已初始化，级别: {}", level);
}

/// 创建默认的内存缓存
pub fn create_memory_cache() -> cache::MemoryCache {
    cache::MemoryCache::new("default-memory-cache")
}

/// 创建带 TTL 的缓存
pub fn create_ttl_cache(ttl_secs: u64) -> cache::TtlCache {
    cache::TtlCache::new(
        "default-ttl-cache",
        std::time::Duration::from_secs(ttl_secs),
    )
}

/// 创建 LRU 缓存
pub fn create_lru_cache(max_size: usize) -> cache::LruCache {
    cache::LruCache::new("default-lru-cache", max_size)
}

/// 创建默认内存存储
pub fn create_memory_storage() -> storage::MemoryStorage {
    storage::MemoryStorage::new("default-memory-storage")
}

/// 创建默认事件总线
pub fn create_event_bus() -> event_bus::EventBus {
    event_bus::EventBus::new()
}

/// 创建默认健康中心
pub fn create_health_center() -> health::HealthCenter {
    health::HealthCenter::new()
}

/// 创建默认生命周期管理器
pub fn create_lifecycle_manager() -> lifecycle::LifecycleManager {
    lifecycle::LifecycleManager::new()
}

/// 创建默认 DI 容器
pub fn create_di_container() -> dependency::DiContainer {
    dependency::DiContainer::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use crate::lifecycle::LifecycleState;
    use crate::storage::Storage;

    #[test]
    fn prelude_exports_compile() {
        // 验证 prelude 所有导出类型可编译
        let _bus = create_event_bus();
        let _health = create_health_center();
        let _lifecycle = create_lifecycle_manager();
        let _di = create_di_container();
    }

    #[test]
    fn factory_functions_return_valid_objects() {
        let cache = create_memory_cache();
        assert!(cache.name().contains("memory"));

        let ttl = create_ttl_cache(60);
        assert!(ttl.name().contains("ttl"));

        let lru = create_lru_cache(100);
        assert!(lru.name().contains("lru"));

        let storage = create_memory_storage();
        assert!(storage.name().contains("memory"));
    }

    #[test]
    fn init_logging_does_not_panic() {
        // 初始化日志不应 panic（即使在测试中可能重复初始化）
        init_logging("debug");
    }

    #[test]
    fn health_center_starts_empty() {
        let hc = create_health_center();
        assert_eq!(hc.component_count(), 0);
    }

    #[test]
    fn lifecycle_manager_starts_uninitialized() {
        let lm = create_lifecycle_manager();
        assert_eq!(lm.state(), LifecycleState::Uninitialized);
    }

    #[test]
    fn di_container_starts_empty() {
        let di = create_di_container();
        assert!(!di.has("nonexistent"));
    }
}
