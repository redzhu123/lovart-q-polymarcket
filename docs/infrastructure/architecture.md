# pm-infrastructure 架构文档

## 概述

`pm-infrastructure`（P2-07）是统一的交易基础设施层，提取并统一分散在各业务 crate 中的基础设施能力。

## 架构原则

- **提取而非重复**：所有能力从现有 crate 提取，不重复实现
- **加性变更**：不修改现有 crate，业务模块可逐步迁移
- **最小依赖**：仅依赖 `pm-core` + 外部 crate，不依赖任何业务 crate
- **安全第一**：Secret 自动脱敏，DryRun 默认启用

## 模块架构

```
pm-infrastructure
├── configuration/    配置中心（TOML/Env/CLI/Defaults，热加载）
├── authentication/   认证框架（Session/API Key，JWT/Wallet/OAuth 预留）
├── secret/          密钥管理（SensitiveString 自动脱敏，Env/DotEnv）
├── cache/           缓存框架（Memory/TTL/LRU，Redis 预留）
├── storage/         存储框架（Memory/CSV/SQLite，PostgreSQL/ClickHouse 预留）
├── scheduler/       任务调度（TokenBucket/Backoff/CircuitBreaker/RetryExecutor）
├── plugin/          插件框架（PluginRegistry，动态发现预留）
├── event_bus/       事件总线（SystemEvent 11种事件，Subscriber，失败隔离）
├── health/          健康中心（HealthCheckable，中文报告）
├── lifecycle/       生命周期（10 状态，LifecycleManager）
├── metrics/         指标收集（Counter/Gauge/Histogram，Prometheus 输出）
├── trace/           追踪（CorrelationId/RequestId，结构化日志预留）
├── diagnostics/     诊断工具（Diagnosable，便捷 diagnose_* 函数）
└── dependency/      依赖注入（DiContainer，&str 键，初始化/关闭排序）
```

## 提取来源

| 模块 | 提取来源 |
|------|---------|
| `secret/` | `pm-auth::credential` + `pm-trading::credential` + `pm-trading::mask` |
| `metrics/` | `pm-gateway::metrics::prometheus` |
| `trace/` | `pm-gateway::middleware::tracing_mw` |
| `scheduler/` | `pm-execution::scheduler` + `pm-gateway::retry` |
| `cache/` | `pm-scanner::datasource::cache` |
| `storage/` | `pm-oms::repository` + `pm-storage` |
| `event_bus/` | `pm-oms::events` |
| `authentication/` | `pm-auth` (auth_provider/session/signer/middleware) |
| `health/` | `pm-gateway::health` + `pm-scanner::health` |
| `lifecycle/` | `pm-oms::lifecycle` + `pm-trading::state` |
| `diagnostics/` | 各 crate 的 diagnose_* 函数 |

## 依赖规则

```
pm-core  ←  pm-infrastructure  ←  业务 crate (pm-oms, pm-pms, ...)
```

- pm-infrastructure 只依赖 pm-core 和外部 crate
- 业务 crate 禁止自行实现配置、缓存、日志、生命周期、健康检查、认证、事件总线
- 业务 crate 逐步迁移以依赖 pm-infrastructure
