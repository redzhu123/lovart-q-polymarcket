# Gateway 状态机文档

> P2-03 Exchange Gateway Implementation | 更新: 2026-07-23

## 1. Gateway 生命周期状态

Gateway 自身有一个简单的生命周期状态机：

```
  ┌─────────────┐
  │  Initial    │  ← Gateway::new()
  │  (未连接)    │
  └──────┬──────┘
         │ connect()
         ▼
  ┌─────────────┐
  │  Connected  │  ← transport 已就绪
  │  (已连接)    │
  └──────┬──────┘
         │ disconnect()
         ▼
  ┌─────────────┐
  │ Disconnected│  ← transport 已关闭
  └──────┬──────┘
         │ (可重新 connect)
         ▼
      回到 Initial
```

### 状态描述

| 状态 | 描述 | 允许操作 |
|------|------|----------|
| Initial | Gateway 已创建，但未调用 `connect()` | 只读操作（info、get_markets 等） |
| Connected | `connect()` 调用成功，Transport 已激活 | 所有操作（包括 submit_order） |
| Disconnected | `disconnect()` 已调用 | 只读操作（不再进行 HTTP 请求） |

### 状态转换

| From | To | 触发器 | 副作用 |
|------|-----|--------|--------|
| Initial | Connected | `gateway.connect()` | 初始化 Transport |
| Connected | Disconnected | `gateway.disconnect()` | 清理 Transport |
| Disconnected | Connected | 再次 `connect()` | 重启 Transport |

## 2. 断路器状态机

Gateway 内嵌 CircuitBreaker，用于防止级联失败：

```
        失败 < 阈值
   ┌──────────────┐
   │   Closed     │ ───────────────────────┐
   │   (正常)      │                        │
   └──────┬───────┘                        │
          │ 连续失败 ≥ 阈值                  │
          ▼                                 │
   ┌──────────────┐                        │
   │   Open       │                        │
   │   (熔断)      │ ◄─────────────────────┘
   └──────┬───────┘    测试失败 → 重新打开
          │ 超过恢复超时
          ▼
   ┌──────────────┐
   │   HalfOpen   │  ─── 测试成功 → 回到 Closed
   │   (半开)      │
   └──────────────┘
```

### 状态转换规则

| From | To | 条件 |
|------|-----|------|
| Closed | Open | `failure_count >= threshold` |
| Open | HalfOpen | `elapsed >= recovery_timeout_ms` |
| HalfOpen | Closed | 测试请求成功 |
| HalfOpen | Open | 测试请求失败 |

## 3. P2-02 Workflow 状态机集成

Gateway 内部调用流程遵循 P2-02 API Workflow 状态机（详见 [state-machine.md](../workflow/state-machine.md)）。

```
WorkflowState:
  Idle
    ↓
  LoadingMarket        → GET /markets
    ↓
  LoadingOrderBook     → GET /book?token_id=...
    ↓
  CheckingBalance      → GET /balance
    ↓
  BuildingOrder        → 本地构造
    ↓
  SubmittingOrder      → POST /order (DryRun: 仅构建)
    ↓
  WaitingResult        → 等待
    ↓
  SyncOrder            → GET /orders
    ↓
  SyncTrade            → GET /trades
    ↓
  SyncPosition         → GET /positions
    ↓
  SyncBalance          → GET /balance
    ↓
  Completed
```

## 4. 速率限制状态

`RateLimiter` 使用 Token Bucket 算法：

```
  ┌──────────────┐
  │  充足 (100%)  │  ─── 每次请求消耗 1 token
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │   警告       │  ─── 剩余 < 10% 时记录警告
  │   (< 10%)    │
  └──────┬───────┘
         │
         ▼
  ┌──────────────┐
  │   耗尽 (0%)  │  ─── 新请求需等待
  └──────┬───────┘
         │ 时间流逝
         ▼
  回到 充足
```

## 5. 重试执行器状态

`RetryExecutor` 管理重试次数：

```
  Attempt 0 → 500ms
    ↓
  Attempt 1 → 1000ms
    ↓
  Attempt 2 → 2000ms
    ↓
  Attempt 3 → 4000ms
    ↓
  (max_retries reached)
    ↓
  返回 RetryError::Exhausted
```

每个 Retry 步骤都通过 CircuitBreaker.allow_request() 检查。

## 6. 中间件钩子顺序

每个 HTTP 请求依次触发以下钩子（顺序执行）：

```
on_request (请求前)
  ↓
  1. Logger.on_request     → [请求] GET /time id=...
  2. Auth.on_request       → 检查认证头
  3. RateLimit.on_request  → 检查剩余
  4. Metrics.on_request    → 无操作
  5. Tracing.on_request    → 创建 span
  ↓
Transport.send()
  ↓
on_response (响应后)
  ↓
  1. Logger.on_response    → [响应] ✅ 200 42ms
  2. Metrics.on_response   → 记录 latency + 成功
  3. Tracing.on_response   → 完成 span
  ↓
返回结果

或

on_error (错误时)
  ↓
  1. Logger.on_error       → [错误] 连接失败
  2. Auth.on_error         → 检查认证错误
  3. RateLimit.on_error    → 记录限流
  4. Metrics.on_error      → 记录失败 + 重试
  5. Tracing.on_error      → 错误 span
  ↓
返回错误
```

## 7. 状态查询

```rust
// Gateway 状态
let info = gateway.info();
println!("{}", info.summary_zh());

// 断路器状态
let breaker_stats = breaker.stats_zh();
println!("{}", breaker_stats);

// 速率限制状态
let rl_stats = rate_limiter.stats();
println!("{}", rl_stats.summary_zh());

// 指标
let metrics = metrics.snapshot();
println!("{}", metrics.report_zh());
```