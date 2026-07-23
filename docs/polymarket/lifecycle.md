# Polymarket Order 生命周期

> 最后更新: 2026-07-23 | 适用版本: CLOB V2 (2026-04-28+)

---

## 1. 概述

Polymarket 订单的生命周期跨越**链下匹配**和**链上结算**两个阶段。完整的生命周期从策略层生成交易信号开始，到链上最终确认结束。

---

## 2. 生命周期状态机

### 2.1 完整状态图

```
                    ┌──────────┐
                    │  CREATED │  策略层生成订单
                    └────┬─────┘
                         │
                    ┌────▼─────┐
                    │ VALIDATED│  本地验证通过
                    └────┬─────┘
                         │
                    ┌────▼─────┐
                    │  QUEUED  │  进入执行队列
                    └────┬─────┘
                         │
                    ┌────▼─────┐
                    │SUBMITTED │  POST /order → CLOB
                    └────┬─────┘
                         │
              ┌──────────┼──────────┐
              │          │          │
         ┌────▼───┐ ┌───▼────┐ ┌──▼──────┐
         │ACCEPTED│ │REJECTED│ │ EXPIRED │
         └────┬───┘ └────────┘ └─────────┘
              │
    ┌─────────┼─────────┐
    │         │         │
┌───▼────┐ ┌─▼──────┐ ┌▼────────┐
│PARTIAL │ │ FILLED │ │CANCELLED│
└───┬────┘ └────────┘ └─────────┘
    │
    │ (继续匹配)
    │
┌───▼────┐
│ FILLED │
└────────┘
```

### 2.2 状态详细说明

| 状态 | 发生位置 | 触发条件 | 下一状态 |
|------|----------|----------|----------|
| **CREATED** | 本地 `OrderBuilder` | 策略层调用 `pipeline.submit(request)` | VALIDATED |
| **VALIDATED** | 本地 `ExecutionValidator` | 8 条验证规则全部通过 | QUEUED |
| **QUEUED** | 本地 `ExecutionQueue` | 通过验证 + 入队成功 | SUBMITTED |
| **SUBMITTED** | `POST /order` → CLOB | 调度器放行 + L2 签名请求发送 | ACCEPTED / REJECTED |
| **ACCEPTED** | CLOB | 服务器验证通过（签名、余额、tick size 等） | PARTIAL / FILLED / CANCELLED |
| **PARTIAL** | CLOB | 部分匹配成交，剩余仍在 book 中 | FILLED / CANCELLED |
| **FILLED** | CLOB → Polygon | 全部成交 | (终态) |
| **REJECTED** | CLOB / 本地 | 验证失败（签名错误、余额不足等） | (终态) |
| **CANCELLED** | CLOB | 用户主动取消或系统自动取消 | (终态) |
| **EXPIRED** | CLOB | GTD 订单到达过期时间 | (终态) |
| **FAILED** | 本地重试耗尽 | 网络错误、超时等，重试次数用尽 | (终态) |

---

## 3. 链上结算流程

CLOB 的订单匹配发生在链下，但结算在 Polygon 链上。成交后的链上流程：

### 3.1 成交生命周期 (Trade Lifecycle)

```
MATCHED (链下匹配)
    │
    ▼
MINED (已提交到 Polygon 内存池)
    │
    ▼
CONFIRMED (Polygon 区块确认)

异常路径:
MATCHED → RETRYING → MINED → CONFIRMED  (重试成功)
MATCHED → RETRYING → FAILED              (重试失败)
```

### 3.2 可观测性

| 阶段 | 数据源 | 延迟 |
|------|--------|------|
| 订单状态变更 | CLOB REST `GET /order/{id}` | ~100ms |
| 订单事件 | User WebSocket `order` event | 实时 |
| 成交事件 | User WebSocket `trade` event | 实时 |
| 链上确认 | Polygon RPC (`OrderFilled` event) | 区块确认后 (~3s) |
| 历史记录 | Data API `/trades` | 数秒 |

---

## 4. 与当前项目 Execution 状态机的映射

### 4.1 当前 `OrderStatus` 枚举

```rust
// crates/execution/src/order.rs
pub enum OrderStatus {
    Created,
    Validated,
    Queued,
    Submitted,
    Accepted,
    PartiallyFilled,
    Filled,        // 终态
    Cancelled,     // 终态
    Expired,       // 终态
    Rejected,      // 终态
    Failed,        // 终态
}
```

### 4.2 状态映射

| 本地状态 | 外部对应 | 说明 |
|----------|----------|------|
| `Created` | - | 纯本地，无外部对应 |
| `Validated` | - | 纯本地，无外部对应 |
| `Queued` | - | 纯本地，无外部对应 |
| `Submitted` | POST /order 已发送 | 等待服务器响应 |
| `Accepted` | CLOB 返回 success | `status: "accepted"` |
| `PartiallyFilled` | CLOB order UPDATE | `size_matched > 0 && size_matched < original_size` |
| `Filled` | 全部成交 | `size_matched == original_size` 且链上 CONFIRMED |
| `Cancelled` | DELETE /order 成功 | 用户主动取消 |
| `Expired` | GTD 自动过期 | CLOB 自动取消（服务器端） |
| `Rejected` | CLOB 返回 400 | 服务器拒绝（详见 [clob.md](clob.md) 错误码） |
| `Failed` | 网络错误/超时 | 重试耗尽后的终态 |

### 4.3 需要调整的地方

1. **增加区分**: `Filled` 应区分 "CLOB 已匹配" vs "链上已确认"
2. **新增状态**: 可能需要 `Settled`（链上最终确认）作为 Filled 后的终态
3. **PARTIAL 过渡**: `PartiallyFilled` → `Filled` 的转换应通过 WebSocket `order` (UPDATE) 事件驱动
4. **重试逻辑**: `Failed` 前的重试应与 CLOB 的 `order timed out` 错误对齐（该错误表示可安全重试）

---

## 5. 事件驱动 vs 轮询

### 5.1 推荐方式：WebSocket 事件驱动

```
User WS → order event (type: PLACEMENT)    → 状态 → Accepted
User WS → order event (type: UPDATE)       → 状态 → PartiallyFilled
User WS → trade event (status: CONFIRMED)  → 状态 → Filled
User WS → order event (type: CANCELLATION) → 状态 → Cancelled
```

### 5.2 降级方式：REST 轮询

当 WebSocket 不可用时：
```
定时 (每 N 秒) 调用 GET /orders 检查活跃订单
定时 (每 N 秒) 调用 GET /order/{id} 检查特定订单
```

---

## 6. 订单生效与取消的时序

### 6.1 提交 → 生效

```
T0: POST /order
T0+100ms: CLOB 返回 accepted
T0+200ms: WebSocket 推送 order PLACEMENT
T0+300ms: 订单出现在 GET /orders 列表中
T0+3s: (如立即成交) trade 事件 MATCHED → MINED → CONFIRMED
```

### 6.2 取消时序

```
T0: DELETE /order
T0+100ms: CLOB 返回 success
T0+200ms: WebSocket 推送 order CANCELLATION
T0+300ms: 订单从 GET /orders 列表中消失

注意: 已经 MATCHED 的订单可能无法取消。取消前先检查订单状态。
```

---

## 7. 异常处理

### 7.1 网络超时

```
POST /order → 超时 (5s)
  ├── 不要立即重试！
  ├── 先查 GET /order/{id} 或 GET /orders 确认订单是否已创建
  ├── 如果已创建: 继续跟踪状态
  └── 如果未创建: 可以重试提交
```

客户端必须在提交前生成并存储 `client_order_id`，超时后通过 `client_order_id` 去重确认。

### 7.2 服务器返回 "order timed out"

```
错误: "order timed out"
含义: 服务器端处理超时（burst 并发导致）
处理: 订单未被创建，可以安全重试
```

### 7.3 链上交易卡住

```
trade status: MATCHED → RETRYING → RETRYING → ...
处理:
  1. 等待（通常最终会 MINED 或 FAILED）
  2. 如果 FAILED: 资金应是安全的（订单未成交）
  3. 联系支持: 如果长时间卡在 RETRYING
```

---

## 8. 当前接入位置与改进建议

### 8.1 当前实现

```rust
// crates/execution/src/pipeline.rs — ExecutionPipeline
pub fn handle_gateway_result(&mut self, result: GatewayResult) {
    match result.status {
        "accepted" => order.transition(OrderStatus::Accepted),
        "filled"   => order.transition(OrderStatus::Filled),
        "rejected" => order.transition(OrderStatus::Rejected),
        // ...
    }
}
```

### 8.2 真实 Gateway 需要的改进

| 改进项 | 说明 |
|--------|------|
| **异步状态更新** | 不再由 `handle_gateway_result` 一次性决定终态。提交后转 `Accepted`，后续由 WS 或轮询更新 |
| **状态订阅** | `ExecutionPipeline` 应订阅 User WS 的 order/trade 事件，持续更新 Order 状态 |
| **链上确认** | `Filled` 后应追踪链上 `CONFIRMED` 状态才标记为完全完成 |
| **超时处理** | 增加提交超时→查询确认→决策重试的三步逻辑 |
| **幂等提交** | 基于 `client_order_id` 的幂等性保证，防止网络重试导致重复订单 |

---

## 9. 未来扩展建议

1. **状态回放**: 基于 `/activity` 重建历史订单的完整状态变化时间线
2. **自动重试策略**: 区分可重试错误（timeout, Too Early, 500）和不可重试错误（invalid signature, balance）
3. **订单审计日志**: 记录每一个状态转换的 timestamp + reason，用于事后复盘
4. **链上最终性追踪**: 后台任务持续追踪已成交订单的链上确认状态
5. **异常订单告警**: 超过 N 秒未从 ACCEPTED 变为 FILLED/CANCELLED 的订单自动告警
