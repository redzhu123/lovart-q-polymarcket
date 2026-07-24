# Gateway 架构文档

> P2-03 Exchange Gateway Implementation | 更新: 2026-07-23

## 1. 目标

实现企业级 Exchange Gateway，统一所有交易所（Polymarket、Kalshi、Binance、DEX 等）的通信接口。Execution 只能通过此 crate 调用交易所，禁止直接访问 HTTP 或 WebSocket。

## 2. 职责边界

| 维度 | 说明 |
|------|------|
| 唯一入口 | 所有外部通信经由本 crate |
| 业务隔离 | Strategy / Risk / Execution 禁止直接 HTTP，必须通过 `ExchangeGateway` trait |
| 通信转换 | JSON ↔ Rust 类型 统一转换 |
| 状态同步 | Order / Balance / Position 同步管理 |
| 可扩展 | 通过 Middleware 扩展新功能 |

## 3. 架构层级

```
┌─────────────────────────────────────────────────┐
│  CLI / Execution (业务层)                         │
│  使用: Box<dyn ExchangeGateway>                  │
└──────────────────┬──────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────┐
│  ExchangeGateway Trait (抽象层)                  │
│  submit_order / cancel / get_markets / ...       │
└──────────────────┬──────────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
┌───────────────┐       ┌────────────────┐
│ MockGateway   │       │ PolymarketGateway│
│ (Paper/测试)  │       │ (真实 API)       │
└───────┬───────┘       └────────┬───────┘
        │                        │
        │                        ▼
        │              ┌────────────────────┐
        │              │  Middleware Stack  │
        │              │  ┌──────────────┐  │
        │              │  │ Logger       │  │
        │              │  ├──────────────┤  │
        │              │  │ Auth         │  │
        │              │  ├──────────────┤  │
        │              │  │ RateLimit    │  │
        │              │  ├──────────────┤  │
        │              │  │ Metrics      │  │
        │              │  ├──────────────┤  │
        │              │  │ Tracing      │  │
        │              │  └──────────────┘  │
        │              └────────┬───────────┘
        │                       │
        │                       ▼
        │              ┌────────────────────┐
        │              │  Transport Layer   │
        │              │  ┌──────────────┐  │
        │              │  │ REST (Reqwest)│  │
        │              │  ├──────────────┤  │
        │              │  │ WS (Tungsten)│  │
        │              │  └──────────────┘  │
        │              └────────┬───────────┘
        │                       │
        └───────────────────────┴───► API / Mock
```

## 4. 模块结构

```
crates/gateway/
├── Cargo.toml
├── src/
│   ├── lib.rs               # 模块导出 + 工厂函数
│   ├── traits.rs            # ExchangeGateway trait
│   ├── types.rs             # 共享类型 (GatewayResult, OrderRequest, Market, OrderBook)
│   ├── error.rs             # 统一错误类型 (GatewayError)
│   ├── config.rs            # GatewayConfig + to_api_test_config 桥接
│   ├── adapter.rs           # JSON ↔ Rust 转换
│   ├── retry.rs             # Backoff / CircuitBreaker / RetryExecutor
│   ├── metrics.rs           # 基础指标 (GatewayMetrics)
│   │   └── prometheus.rs    # Prometheus 风格指标
│   ├── health.rs            # HealthChecker / HealthReport
│   ├── sync.rs              # SyncManager (Order/Balance/Position)
│   ├── diagnostics.rs       # 诊断输出
│   ├── mock.rs              # MockGateway
│   ├── polymarket/
│   │   ├── mod.rs           # PolymarketGateway
│   │   ├── rest.rs          # 旧 REST 实现（已废弃）
│   │   └── types.rs         # Polymarket API JSON 类型
│   ├── transport/
│   │   ├── mod.rs           # Transport 模块入口
│   │   ├── rest.rs          # HttpTransport trait + ReqwestTransport 实现
│   │   └── websocket.rs     # WsTransport trait + NoopWsTransport 占位
│   ├── middleware/
│   │   ├── mod.rs           # Middleware trait + MiddlewareStack
│   │   ├── logger.rs        # 请求/响应日志
│   │   ├── auth.rs          # 认证头注入
│   │   ├── retry.rs         # 重试（包装 RetryExecutor）
│   │   ├── ratelimit.rs     # 速率限制
│   │   ├── metrics.rs       # 指标收集
│   │   └── tracing_mw.rs    # Tracing span 创建（避免与 tracing crate 同名）
│   ├── auth/
│   │   └── mod.rs           # AuthProvider trait + PolymarketAuth + NoopAuth
│   └── ratelimit/
│       └── mod.rs           # TokenBucket + RateLimiter
├── tests/                   # 集成测试
│   ├── gateway_integration.rs   # 完整生命周期
│   ├── transport_test.rs        # Transport 抽象
│   ├── error_test.rs            # 错误类型
│   └── ratelimit_test.rs        # 速率限制
└── examples/                # 示例代码（待添加）
```

## 5. P2-01 / P2-02 集成

| P2 阶段 | 集成方式 |
|---------|----------|
| P2-01 API 认证 | `GatewayConfig::to_api_test_config()` 桥接为 `pm_api_test::ApiTestConfig`，`ReqwestTransport` 复用 `ApiClient` |
| P2-02 API Workflow | PolymarketGateway 内部调用流程遵循 `pm_api_workflow::StateMachine::lifecycle_order()` 状态机 |

## 6. 安全模型

| 层级 | 默认行为 | 真实交易 |
|------|----------|----------|
| Gateway | DryRun | 需 `enable_live=true` |
| PolymarketGateway | `submit_order` 拒绝 | 需 `enable_live=true` + API 密钥 |
| 速率限制 | 自动 Token Bucket | 自动 Token Bucket |
| 断路器 | 失败 5 次触发 | 失败 5 次触发 |
| 中间件 | 默认开启 5 层 | 默认开启 5 层 |

## 7. 数据模型

### Gateway 类型

- `GatewayResult` — 单次操作结果（订单 ID、状态、成交数量、价格、消息、耗时）
- `OrderRequest` — 下单请求（client_order_id、市场、方向、买卖、价格、数量）
- `Balance` — 账户余额（可用、总额、占用、未实现盈亏、已实现盈亏）
- `Position` — 持仓（市场、问题、方向、数量、入场价、标记价、盈亏）
- `Market` — 市场（ID、问题、关闭状态、YES/NO 价格、成交量、流动性）
- `OrderBook` — 订单簿（市场、买盘、卖盘、最小报价单位）
- `GatewayInfo` — Gateway 摘要（名称、类型、模式、健康、延迟、WS 状态、Rate Limit、订单统计）
- `GatewayError` — 错误类型（网络/认证/限流/校验/交易所/超时/序列化）

### Prometheus 指标

| 指标 | 类型 | 说明 |
|------|------|------|
| `gateway_api_requests_total` | Counter | API 请求总数 |
| `gateway_api_requests_success` | Counter | 成功次数（2xx） |
| `gateway_api_requests_failure` | Counter | 失败次数（非 2xx） |
| `gateway_api_latency_ms` | Histogram | 延迟分布（11 个桶） |
| `gateway_ws_reconnects_total` | Counter | WS 重连次数 |
| `gateway_ws_disconnects_total` | Counter | WS 断连次数 |
| `gateway_rate_limit_hits_total` | Counter | 限流触发次数 |
| `gateway_circuit_trips_total` | Counter | 断路器跳闸次数 |
| `gateway_active_orders` | Gauge | 活跃订单数 |
| `gateway_ws_connected` | Gauge | WS 连接状态（0/1） |

## 8. 测试覆盖

| 测试类型 | 数量 | 位置 |
|----------|------|------|
| 单元测试 | 144+ | `src/**/*.rs` 内 `#[cfg(test)] mod tests` |
| 集成测试 | 44+ | `tests/*.rs` |
| **总计** | **188+** | |

测试覆盖：
- MockGateway 完整生命周期
- PolymarketGateway DryRun 拒绝
- Transport GET / POST / DELETE
- 中间件链顺序执行
- 速率限制 Token Bucket
- 错误类型创建和显示
- Prometheus 文本格式输出
- Rate Limit 并发访问

## 9. 已知限制

| 项 | 说明 | 后续改进 |
|----|------|----------|
| WebSocket | 当前为 NoopWsTransport 占位实现 | 后续接入 tokio-tungstenite |
| Prometheus 暴露 | 当前仅文本输出，无 HTTP 端点 | 可对接 prometheus exporter |
| HMAC 签名 | Polymarket L2 签名待实现 | 接入 EIP-712 钱包签名 |
| 真实交易测试 | 默认 DryRun，真实交易需手动开启 | 添加集成测试（`#[ignore]`） |
| 凭据轮换 | 当前单凭据 | 支持多 API 密钥轮换 |
| 重试策略 | 固定指数退避 | 支持动态调整 |

## 10. 扩展指南

要新增一个交易所（如 Kalshi、Binance、DEX）：

1. 创建 `crates/gateway/src/{exchange}/mod.rs`
2. 实现 `ExchangeGateway` trait（最少 11 个方法）
3. 在 `lib.rs::create_gateway()` 注册新类型
4. 在 `config.rs::GatewayConfig` 添加配置字段
5. 添加集成测试
6. 更新文档

详见 `gateway-extension.md`。