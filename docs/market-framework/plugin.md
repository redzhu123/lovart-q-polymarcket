# MarketPlugin Trait（P3.0）

## 概述

`MarketPlugin` 是多市场统一框架的核心 Trait。所有市场（Polymarket、Kalshi、Binance、OKX、Bybit、Hyperliquid、Uniswap、Raydium）必须实现此接口。

## 接口定义

```rust
#[async_trait]
pub trait MarketPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &str { "1.0.0" }
    fn market_type_code(&self) -> &str;
    fn description(&self) -> &str { "市场插件" }
    fn supported_features(&self) -> &CapabilitySet;
    fn has_capability(&self, cap: &MarketCapability) -> bool;
    fn provider(&self) -> Option<&dyn MarketDataProvider> { None }
    fn gateway_name(&self) -> &str;
    fn live_enabled(&self) -> bool { false }
    fn adapter(&self) -> Option<&dyn MarketAdapter> { None }
    fn metadata(&self) -> &MarketMetadata;
    async fn initialize(&mut self) -> MarketFrameworkResult<()>;
    async fn start(&mut self) -> MarketFrameworkResult<()>;
    async fn stop(&mut self) -> MarketFrameworkResult<()>;
    async fn shutdown(&mut self) -> MarketFrameworkResult<()>;
    async fn health(&self) -> MarketHealthReport;
    async fn ping(&self) -> bool;
    fn info_summary_zh(&self) -> String;
}
```

## 方法说明

| 方法 | 类型 | 说明 |
|------|------|------|
| `id()` | 必须 | 插件唯一 ID（如 "polymarket-v1"） |
| `name()` | 必须 | 插件中文名称（如 "Polymarket 预测市场"） |
| `version()` | 默认 | 版本号，默认 "1.0.0" |
| `market_type_code()` | 必须 | 市场类型代码（如 "polymarket", "binance"） |
| `description()` | 默认 | 插件描述 |
| `supported_features()` | 必须 | 返回该市场的能力集合 |
| `has_capability()` | 默认 | 检查是否支持某能力 |
| `provider()` | 默认 | 获取数据供应商（None 表示不需要） |
| `gateway_name()` | 必须 | 网关名称（如 "polymarket", "binance", "mock"） |
| `live_enabled()` | 默认 | 是否启用真实交易（默认 false） |
| `adapter()` | 默认 | 获取数据适配器（None 表示不需要） |
| `metadata()` | 必须 | 获取市场元数据 |
| `initialize()` | 默认 | 初始化插件 |
| `start()` | 默认 | 启动插件 |
| `stop()` | 默认 | 停止插件 |
| `shutdown()` | 默认 | 关闭插件 |
| `health()` | 必须 | 执行健康检查 |
| `ping()` | 默认 | 快速 Ping 检查 |
| `info_summary_zh()` | 默认 | 插件信息中文摘要 |

## 实现示例

```rust
struct PolymarketPlugin {
    metadata: MarketMetadata,
    capabilities: CapabilitySet,
}

#[async_trait]
impl MarketPlugin for PolymarketPlugin {
    fn id(&self) -> &str { "polymarket-v1" }
    fn name(&self) -> &str { "Polymarket 预测市场" }
    fn market_type_code(&self) -> &str { "polymarket" }
    fn supported_features(&self) -> &CapabilitySet { &self.capabilities }
    fn gateway_name(&self) -> &str { "polymarket" }
    fn metadata(&self) -> &MarketMetadata { &self.metadata }

    async fn health(&self) -> MarketHealthReport {
        MarketHealthReport::healthy("Polymarket")
    }
}
```

## 新增市场流程

新增一个市场仅需：

1. 新增 `Provider`（数据供应商）
2. 新增 `Adapter`（数据格式适配）
3. 新增 `Gateway`（执行网关）
4. 实现 `MarketPlugin` Trait

**不得修改**：Strategy / Risk / OMS / Settlement / PMS / Infrastructure

## 安全约束

- `health()` 不得产生副作用
- `provider()` 和 `adapter()` 返回共享引用
- 所有方法必须线程安全（`Send + Sync`）
- 默认 `live_enabled() = false`（Dry Run）
