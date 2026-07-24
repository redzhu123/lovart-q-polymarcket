# Market Registry（P3.0 第四节）

## 概述

`MarketRegistry` 是市场插件的注册中心，负责：

- 注册与注销 MarketPlugin
- 按 ID / 名称 / 类型 / 能力查询
- 列出所有已安装市场
- 发布市场变化事件

## 核心 API

### 注册 / 注销

```rust
let registry = MarketRegistry::new();

// 注册插件
registry.register(Box::new(PolymarketPlugin::new()))?;

// 注销插件
let name = registry.unregister("polymarket-v1")?;
```

### 查询

```rust
// 按 ID 查询是否存在
registry.exists("polymarket-v1");

// 使用闭包访问插件
registry.with_plugin("polymarket-v1", |plugin| {
    println!("{}", plugin.name());
});

// 按类型查询
let spot_plugins = registry.find_by_type("spot");

// 按能力查询
let trading_plugins = registry.find_by_capability(&MarketCapability::LiveTrading);
```

### 列表

```rust
// 获取所有插件摘要
let summaries = registry.list_all_summaries();

// 渲染为中文表格
println!("{}", registry.render_table_zh());

// 获取所有 ID
let ids = registry.list_ids();
```

## PluginSummary

```rust
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub market_type: String,
    pub gateway: String,
    pub live_enabled: bool,
    pub capability_count: usize,
    pub description: String,
}
```

## 注册表表格示例

```
【市场注册表】已安装 3 个市场

 插件名称                      ID                   市场类型     网关         实盘     能力数
 ────────────────────────────────────────────────────────────────────────────────────
 Polymarket                    polymarket-v1        prediction   polymarket   ❌      15
 Binance                       binance-v1           spot         binance      ❌      22
 OKX                           okx-v1               spot         okx          ❌      20
```

## 线程安全

`MarketRegistry` 使用 `RwLock` 保证线程安全，支持多读单写。

## 与 EventBus 集成

注册和注销插件时，`MarketRegistry` 可通过 `plugin_events()` 生成对应的 `MarketEvent` 列表，供外部 EventBus 发布。
