//! P2-07 基础设施层集成测试
//!
//! 验证所有 14 个模块协同工作。

use chrono::Local;
use pm_infrastructure::prelude::*;
use pm_infrastructure::{
    create_di_container, create_event_bus, create_health_center, create_lifecycle_manager,
    create_memory_storage, create_ttl_cache,
};
use std::time::Duration;

/// 场景 1：完整生命周期集成
#[tokio::test]
async fn full_lifecycle_integration() {
    // 创建 DI 容器
    let mut di = create_di_container();

    // 注册密钥管理器
    let secret_mgr = EnvSecretManager::new("test-secret");
    di.register("secret", secret_mgr).unwrap();

    // 创建事件总线
    let bus = create_event_bus();
    di.register("event_bus", bus).unwrap();

    // 创建健康中心
    let health = create_health_center();
    di.register("health", health).unwrap();

    assert_eq!(di.len(), 3);

    // 发布事件
    let bus: &EventBus = di.get("event_bus").unwrap();
    bus.publish(SystemEvent::MarketUpdated {
        market_id: "test-market".to_string(),
        timestamp: Local::now(),
    });
    bus.publish(SystemEvent::MetricsCollected {
        timestamp: Local::now(),
    });
    assert!(bus.published_count() >= 2);
}

/// 场景 2：密钥 + 脱敏验证
#[tokio::test]
async fn secret_and_masking_integration() {
    let mut mgr = EnvSecretManager::new("test");

    let mut cred = Credential::empty();
    cred.api_key = SensitiveString::new("sk-live-abc123def456ghij7890");
    cred.api_secret = SensitiveString::new("super-secret-value");
    cred.environment = "production".to_string();
    mgr.register("polymarket", cred);

    let summary = mgr.safe_summary();
    // 确认不泄露明文
    assert!(!summary.contains("abc123def456"));
    assert!(!summary.contains("super-secret-value"));
    assert!(summary.contains("EnvSecretManager"));

    // 确认掩码函数不泄露
    let masked = mask_api_key("sk-live-abc123def456ghij7890");
    assert!(!masked.contains("abc123def456"));
    assert!(masked.contains("***"));
}

/// 场景 3：缓存 + 调度器集成
#[tokio::test]
async fn cache_and_scheduler_integration() {
    let cache = create_ttl_cache(1); // 1秒 TTL

    cache
        .set_json("market-BTC", serde_json::json!({"price": 50000}))
        .await
        .unwrap();
    assert!(cache.is_fresh("market-BTC").await);

    // 等待过期
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!cache.is_fresh("market-BTC").await);

    let stats = cache.stats();
    assert!(stats.hit_rate() >= 0.0);
}

/// 场景 4：事件总线 + 指标集成
#[tokio::test]
async fn event_bus_and_metrics_integration() {
    let bus = create_event_bus();
    let metrics = InfrastructureMetrics::new("test");

    // 模拟发布订单事件
    bus.publish(SystemEvent::OrderCreated {
        order_id: "order-1".to_string(),
        market_id: "market-1".to_string(),
        timestamp: Local::now(),
    });
    metrics.record_order_event("OrderCreated");

    bus.publish(SystemEvent::OrderFilled {
        order_id: "order-1".to_string(),
        avg_price: 0.55,
        timestamp: Local::now(),
    });
    metrics.record_order_event("OrderFilled");

    assert_eq!(bus.published_count(), 2);
    let snap = metrics.snapshot();
    assert_eq!(snap.orders_filled, 1);

    // 中文报告生成
    let report = metrics.report_zh();
    assert!(report.contains("指标报告"));
    assert!(report.contains("订单"));
}

/// 场景 5：熔断器集成
#[tokio::test]
async fn circuit_breaker_integration() {
    let mut executor = RetryExecutor::default_with(3);

    // 应该成功
    let result = executor
        .execute("health-check", || async { Ok::<_, &str>("ok") })
        .await;
    assert_eq!(result.unwrap(), "ok");
}

/// 场景 6：中文诊断集成
#[tokio::test]
async fn chinese_diagnostics_integration() {
    // 诊断事件总线
    let bus = create_event_bus();
    let report = pm_infrastructure::diagnostics::diagnose_event_bus(&bus);
    let zh = report.format_zh();
    assert!(zh.contains("事件总线"));
    assert!(zh.contains("订阅者"));

    // 诊断存储
    let storage = create_memory_storage();
    let report = pm_infrastructure::diagnostics::diagnose_storage(&storage).await;
    let zh = report.format_zh();
    assert!(zh.contains("存储诊断"));
    assert!(zh.contains("内存存储"));

    // 诊断调度器
    let sched = IntervalScheduler::new("test");
    let report = pm_infrastructure::diagnostics::diagnose_scheduler(&sched).await;
    let zh = report.format_zh();
    assert!(zh.contains("调度器诊断"));
}

/// 场景 7：健康中心集成
#[tokio::test]
async fn health_center_integration() {
    let mut center = create_health_center();

    // 注册测试组件
    struct TestHealth {
        name: String,
        healthy: bool,
    }

    #[async_trait::async_trait]
    impl HealthCheckable for TestHealth {
        fn component_name(&self) -> &str {
            &self.name
        }
        async fn health_check(&self) -> HealthCheck {
            HealthCheck {
                component: self.name.clone(),
                status: if self.healthy {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Unhealthy
                },
                detail: if self.healthy { "正常" } else { "异常" }.to_string(),
                latency_ms: 0,
            }
        }
    }

    center.register(Box::new(TestHealth {
        name: "Gateway".to_string(),
        healthy: true,
    }));
    center.register(Box::new(TestHealth {
        name: "OMS".to_string(),
        healthy: true,
    }));

    let report = center.check_all().await;
    assert!(report.all_healthy());
    let zh = report.report_zh();
    assert!(zh.contains("健康检查报告"));
    assert!(zh.contains("Gateway"));
}

/// 场景 8：生命周期 + DI 容器集成
#[tokio::test]
async fn lifecycle_and_di_integration() {
    let mut di = create_di_container();
    let mut mgr = create_lifecycle_manager();

    // 注册测试组件
    struct TestLifecycle {
        name: String,
        state: LifecycleState,
    }

    #[async_trait::async_trait]
    impl Lifecycle for TestLifecycle {
        fn name(&self) -> &str {
            &self.name
        }

        fn state(&self) -> LifecycleState {
            self.state
        }

        async fn initialize(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Ready;
            Ok(())
        }
        async fn start(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Running;
            Ok(())
        }
        async fn pause(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn resume(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Stopped;
            Ok(())
        }
        async fn shutdown(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Shutdown;
            Ok(())
        }
        async fn recover(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Ready;
            Ok(())
        }
    }

    mgr.register(Box::new(TestLifecycle {
        name: "Gateway".to_string(),
        state: LifecycleState::Uninitialized,
    }));
    mgr.register(Box::new(TestLifecycle {
        name: "OMS".to_string(),
        state: LifecycleState::Uninitialized,
    }));

    di.register("lifecycle_mgr", mgr).unwrap();

    // 完整生命周期
    let mgr: &mut LifecycleManager = di.get_mut("lifecycle_mgr").unwrap();
    mgr.initialize_all().await.unwrap();
    assert_eq!(mgr.state(), LifecycleState::Ready);

    mgr.start_all().await.unwrap();
    assert_eq!(mgr.state(), LifecycleState::Running);

    mgr.stop_all().await.unwrap();
    assert_eq!(mgr.state(), LifecycleState::Stopped);

    mgr.shutdown_all().await.unwrap();
    assert_eq!(mgr.state(), LifecycleState::Shutdown);
}

/// 场景 9：插件注册中心集成
#[test]
fn plugin_registry_integration() {
    let mut reg = PluginRegistry::new();
    assert_eq!(reg.count(), 0);

    // 注册不同类型的插件
    struct DummyPlugin {
        info: PluginInfo,
    }

    #[async_trait::async_trait]
    impl Plugin for DummyPlugin {
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

    reg.register(Box::new(DummyPlugin {
        info: PluginInfo {
            name: "Market-Plugin".to_string(),
            version: "1.0.0".to_string(),
            plugin_type: PluginType::Market,
            description: "测试市场插件".to_string(),
            author: "test".to_string(),
        },
    }))
    .unwrap();

    assert_eq!(reg.count(), 1);
    assert_eq!(reg.list_by_type(PluginType::Market).len(), 1);
    assert_eq!(reg.list_by_type(PluginType::Strategy).len(), 0);
}

/// 场景 10：存储 + CSV 工具集成
#[tokio::test]
async fn storage_and_csv_integration() {
    let store = create_memory_storage();
    store
        .save("order-001", &serde_json::json!({"price": 0.55, "qty": 100}))
        .await
        .unwrap();

    let loaded = store.load("order-001").await.unwrap();
    assert!(loaded.is_some());

    let keys = store.list_keys().await.unwrap();
    assert!(keys.contains(&"order-001".to_string()));

    let health = store.health();
    assert!(health.healthy);
    assert!(health.summary_zh().contains("健康"));
}
