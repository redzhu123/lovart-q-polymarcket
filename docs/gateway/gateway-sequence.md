# Gateway 时序图（P2-03）

> Exchange Gateway Implementation | 更新: 2026-07-23

本文档展示关键操作的时序图。所有中文。

## 1. 下单流程（submit_order）

```
Strategy → Execution → Gateway.submit_order()
                                 │
                                 ▼
                       PolymarketGateway.submit_order()
                                 │
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
  Safety Check            Rate Limit Check         Middleware: before
  (DryRun?)               (Token Bucket)           (Logger/Auth/Tracing)
        │                        │                        │
        └────────────────────────┼────────────────────────┘
                                 ▼
                       MiddlewareStack.run_before()
                                 │
                                 ▼
                       Transport.send() → pm_api_test::ApiClient
                                 │
                                 ▼
                       HTTP /order (POST)
                                 │
                                 ▼
                       HTTP Response
                                 │
                                 ▼
                       MiddlewareStack.run_after()
                                 │
                                 ▼
                       Metrics Middleware 更新
                                 │
                                 ▼
                       GatewayResult (accepted/filled/rejected)
                                 │
                                 ▼
                       Execution 处理 Order 状态转换
```

### DryRun 拒绝分支

```
submit_order()
    │
    ▼
if !enable_live { return rejected("DryRun 模式") }
    │
    ▼
继续执行真实 API 调用
```

## 2. 取消订单流程（cancel_order）

```
Strategy → Execution → Gateway.cancel_order(order_id)
                                 │
                                 ▼
                       cancel_order(order_id)
                                 │
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
  Safety Check            中间件链                  DELETE /order/{id}
  (DryRun?)               (Middleware)                    │
        │                        │                        │
        ▼                        ▼                        ▼
  返回 cancelled            HTTP 请求                HTTP Response
  (DryRun 模式)              Transport                  │
        │                        │                        │
        └────────────────────────┼────────────────────────┘
                                 ▼
                       GatewayResult.cancelled()
```

## 3. 获取市场列表（get_markets）

```
Strategy → Execution → Gateway.get_markets()
                                 │
                                 ▼
                       PolymarketGateway.get_markets()
                                 │
                                 ▼
                       Middleware: before
                                 │
                                 ▼
                       GET /markets (REST)
                                 │
                                 ▼
                       解析 JSON → Vec<Market>
                                 │
                                 ▼
                       Middleware: after
                                 │
                                 ▼
                       返回 Vec<Market>
```

## 4. 获取订单簿（get_orderbook）

```
Strategy → Execution → Gateway.get_orderbook(market_id)
                                 │
                                 ▼
                       PolymarketGateway.get_orderbook()
                                 │
                                 ▼
                       GET /book?token_id={market_id}
                                 │
                                 ▼
                       解析 bids/asks → OrderBook
                                 │
                                 ▼
                       返回 OrderBook { bids, asks, tick_size }
```

## 5. WebSocket 订阅（subscribe）

```
Strategy → Execution → Gateway.subscribe(channel)
                                 │
                                 ▼
                       PolymarketGateway.subscribe()
                                 │
                                 ▼
                       ws_transport.subscribe(channel)
                                 │
                                 ▼
                       NoopWsTransport.subscribe()
                       （占位实现 — 实际待集成）
                                 │
                                 ▼
                       返回 Ok(()) 或 WebSocket 消息
```

## 6. 错误处理流程

```
任意 HTTP 调用
    │
    ▼
Transport.send() 返回 Err(GatewayError)
    │
    ▼
MiddlewareStack.run_on_error()
    │
    ├─ Logger → 记录 [错误]
    ├─ Auth → 检查认证
    ├─ RateLimit → 记录限流
    ├─ Metrics → 增加失败计数
    └─ Tracing → 记录 span 错误
    │
    ▼
返回错误给调用方
```

## 7. 健康检查流程

```
CLI: cargo run -- gateway
    │
    ▼
diagnose_gateway(gateway)
    │
    ├─ gateway.health() → GatewayInfo
    │    │
    │    ├─ gateway.ping() → GET /time
    │    ├─ metrics 快照
    │    ├─ ws_transport.is_connected()
    │    └─ rate_limiter.remaining()
    │
    └─ 中文输出（综合状态、REST、WS、Rate Limit、订单统计）
```

## 8. 重试流程

```
submit_order() 失败
    │
    ▼
Middleware: Retry 检查
    │
    ├─ if error.is_retryable():
    │   │
    │   ├─ CircuitBreaker.allow_request()?
    │   │    │
    │   │    ├─ Closed → 继续
    │   │    └─ Open → 拒绝（断路器已打开）
    │   │
    │   ├─ Backoff.next_delay()
    │   │    │
    │   │    ├─ attempt 0: 500ms
    │   │    ├─ attempt 1: 1000ms
    │   │    ├─ attempt 2: 2000ms
    │   │    └─ max 15000ms
    │   │
    │   └─ 重新发起 HTTP 请求
    │
    └─ if !error.is_retryable():
        │
        └─ 直接返回错误
```

## 9. P2-02 Workflow 状态机集成

```
WorkflowState 序列：
  LoadingMarket → LoadingOrderBook → CheckingBalance
        → BuildingOrder → SubmittingOrder → WaitingResult
        → SyncOrder → SyncTrade → SyncPosition → SyncBalance
        → Completed

Gateway 内部调用：
  LoadingMarket → GET /markets
  LoadingOrderBook → GET /book?token_id=...
  CheckingBalance → GET /balance
  BuildingOrder → 本地构造 OrderRequest
  SubmittingOrder → POST /order
  SyncOrder → GET /orders
  SyncTrade → GET /trades
  SyncPosition → GET /positions
  SyncBalance → GET /balance

注：Gateway 内部方法顺序遵循 Workflow 状态机。
```