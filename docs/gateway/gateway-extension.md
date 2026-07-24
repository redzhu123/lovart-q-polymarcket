# Gateway 扩展指南 — 如何新增一个交易所

> P2-03 Exchange Gateway Implementation | 更新: 2026-07-23

本文档说明如何添加一个新的交易所（如 Kalshi、Binance、DEX）。

## 1. 概述

Gateway 设计为完全可扩展。要新增一个交易所，需要：

1. 实现 `ExchangeGateway` trait
2. 注册到工厂函数
3. 添加配置
4. 添加测试
5. 更新文档

## 2. 步骤

### 2.1 创建新模块

```bash
mkdir -p crates/gateway/src/kalshi
touch crates/gateway/src/kalshi/mod.rs
```

### 2.2 实现 ExchangeGateway Trait

参考 PolymarketGateway 实现：

```rust
//! crates/gateway/src/kalshi/mod.rs

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use chrono::{DateTime, Local};

use super::traits::ExchangeGateway;
use super::config::GatewayConfig;
use super::error::GatewayError;
use super::metrics::GatewayMetrics;
use super::ratelimit::RateLimiter;
use super::transport::rest::{HttpRequest, HttpTransport, ReqwestTransport};
use super::transport::websocket::{NoopWsTransport, WsTransport};
use super::types::{
    Balance, BookLevel, GatewayInfo, GatewayResult, Market, OrderBook,
    OrderRequest, Position,
};
use super::middleware::{self, MiddlewareStack};

pub struct KalshiGateway {
    transport: Arc<ReqwestTransport>,
    ws_transport: Arc<dyn WsTransport>,
    middleware: Arc<MiddlewareStack>,
    rate_limiter: Arc<RateLimiter>,
    config: GatewayConfig,
    metrics: Arc<Mutex<GatewayMetrics>>,
    account_id: String,
}

impl KalshiGateway {
    pub fn new(config: GatewayConfig) -> Self {
        // 1. 构造 Kalshi 专用的 ApiTestConfig
        let api_config = pm_api_test::client::config::ApiTestConfig {
            clob_url: config.kalshi_api_url.clone(),
            // ... 其他配置
            ..Default::default()
        };

        // 2. 创建 Transport + 中间件链
        let transport = Arc::new(ReqwestTransport::new(api_config));
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit_per_sec,
            config.rate_limit_per_min,
        ));

        let middleware = Arc::new(
            MiddlewareStack::new()
                .with(Box::new(middleware::logger::RequestLogger::new()))
                .with(Box::new(middleware::ratelimit::RateLimitMiddleware::new(
                    rate_limiter.clone(),
                )))
                .with(Box::new(middleware::metrics::MetricsMiddleware::new(
                    GatewayMetrics::new(),
                ))),
        );

        Self {
            transport,
            ws_transport: Arc::new(NoopWsTransport::new("wss://ws.kalshi.com")),
            middleware,
            rate_limiter,
            config,
            metrics: Arc::new(Mutex::new(GatewayMetrics::new())),
            account_id: "kalshi-account".to_string(),
        }
    }

    // 实现 send_with_middleware（参考 PolymarketGateway）
}

#[async_trait]
impl ExchangeGateway for KalshiGateway {
    fn name(&self) -> &str { "KalshiGateway" }
    fn gateway_type(&self) -> &str { "kalshi" }
    fn live_enabled(&self) -> bool { self.config.enable_live }

    async fn get_markets(&self) -> Result<Vec<Market>, GatewayError> {
        let req = HttpRequest::get("/markets");
        let resp = self.send_with_middleware(req).await?;
        // 解析 Kalshi 响应...
        todo!()
    }

    async fn submit_order(
        &self,
        request: &OrderRequest,
        _now: DateTime<Local>,
    ) -> GatewayResult {
        if !self.config.enable_live {
            return GatewayResult::rejected(
                &request.client_order_id,
                "DryRun 模式",
                0,
            );
        }

        // 构建 Kalshi 订单 JSON
        let body = serde_json::json!({
            "market_id": request.market_id,
            "side": match request.side { Side::Buy => "yes", Side::Sell => "no" },
            "price": request.price,
            "quantity": request.quantity,
            "type": "limit",
        });

        let req = HttpRequest::post("/portfolio/orders", body);
        let resp = self.send_with_middleware(req).await;
        // ...
        todo!()
    }

    // 其他方法...
}
```

### 2.3 注册到工厂函数

编辑 `crates/gateway/src/lib.rs`：

```rust
pub mod kalshi;

pub use kalshi::KalshiGateway;

pub fn create_gateway(cfg: &GatewayConfig) -> Box<dyn ExchangeGateway> {
    match cfg.gateway_type.as_str() {
        "polymarket" => Box::new(PolymarketGateway::new(cfg.clone())),
        "kalshi" => Box::new(KalshiGateway::new(cfg.clone())), // 新增
        "mock" | _ => Box::new(MockGateway::new(cfg.clone())),
    }
}
```

### 2.4 添加配置

编辑 `crates/gateway/src/config.rs`：

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub gateway_type: String,
    // ... 现有字段

    // Kalshi 专用
    #[serde(default = "default_kalshi_api_url")]
    pub kalshi_api_url: String,
}

fn default_kalshi_api_url() -> String { "https://api.kalshi.com".into() }
```

并更新 `pm-models::config::GatewayRawConfig` 类似字段。

### 2.5 添加测试

创建 `crates/gateway/tests/kalshi_test.rs`：

```rust
use pm_gateway::{create_gateway, GatewayConfig};

#[tokio::test]
async fn kalshi_gateway_dry_run_blocks_orders() {
    let cfg = GatewayConfig {
        gateway_type: "kalshi".into(),
        enable_live: false,
        ..Default::default()
    };
    let gw = create_gateway(&cfg);
    assert_eq!(gw.gateway_type(), "kalshi");
    assert!(!gw.live_enabled());
}

#[tokio::test]
async fn kalshi_gateway_subscribe() {
    let cfg = GatewayConfig {
        gateway_type: "kalshi".into(),
        ..Default::default()
    };
    let gw = create_gateway(&cfg);
    gw.subscribe("orderbook:TEST").await.unwrap();
}
```

### 2.6 更新文档

1. 在 `gateway-architecture.md` 添加新交易所到模块结构
2. 在 `gateway-sequence.md` 添加新交易所的时序图
3. 在 `gateway-extension.md`（本文档）添加更多示例

## 3. 完整示例：BinanceGateway

```rust
//! crates/gateway/src/binance/mod.rs

pub struct BinanceGateway {
    // 类似 KalshiGateway
}

#[async_trait]
impl ExchangeGateway for BinanceGateway {
    fn name(&self) -> &str { "BinanceGateway" }
    fn gateway_type(&self) -> &str { "binance" }
    // ... 实现所有方法
}
```

## 4. 测试矩阵

新增交易所必须通过的测试：

| 测试 | 说明 |
|------|------|
| `dry_run_blocks_orders` | DryRun 模式拒绝下单 |
| `factory_creates_*` | 工厂函数正确创建 |
| `connect_disconnect` | 生命周期正常 |
| `subscribe_unsubscribe` | 订阅正常工作 |
| `get_markets` | 获取市场列表 |
| `get_orderbook` | 获取订单簿 |
| `metrics_record` | 指标正确记录 |

## 5. 共享模式

所有交易所 Gateway 应共享：

1. **Transport 复用**：使用 `ReqwestTransport` 包装 `pm-api-test::ApiClient`
2. **中间件链**：Logger / Auth / RateLimit / Metrics / Tracing
3. **错误处理**：使用 `GatewayError`
4. **指标收集**：使用 `GatewayMetrics`
5. **速率限制**：使用 `RateLimiter`

## 6. 实施清单

- [ ] 实现 `ExchangeGateway` trait（最少 13 个方法）
- [ ] 注册到 `create_gateway()`
- [ ] 添加 `GatewayConfig` 配置
- [ ] 添加单元测试（`#[cfg(test)] mod tests`）
- [ ] 添加集成测试（`tests/`）
- [ ] 更新 `lib.rs` 模块声明和导出
- [ ] 更新 `gateway-architecture.md`
- [ ] 更新 `gateway-sequence.md`
- [ ] 添加示例代码（`examples/`）

## 7. 不要做的事

- ❌ 不要直接访问 `reqwest` 或 `tokio-tungstenite`
- ❌ 不要绕过 Middleware 链
- ❌ 不要在 Gateway 中实现策略、风控、业务逻辑
- ❌ 不要修改 Execution crate
- ❌ 不要使用 String 作为错误类型（必须用 GatewayError）
- ❌ 不要使用 println!（必须用 tracing）