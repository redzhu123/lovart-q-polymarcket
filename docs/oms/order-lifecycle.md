# OMS 订单生命周期（P2-04）

> 单个订单从创建到终止的完整生命周期。

---

## 1. 11 态 + 1 聚合

```
Created（已创建）
  │ validator 通过
  ▼
Validated（已校验）
  │ OMS 决策
  ▼
PendingSubmit（待提交）
  │ oms 提交
  ▼
Submitted（已提交）
  │ gateway 接受
  ▼
Accepted（已接受）
  │
  ├── 部分成交 ──► PartiallyFilled ──► PartiallyFilled（自转，持续）
  │                  │
  │                  └─► Filled
  │
  └─── gateway 取消 ──► Cancelled

Created ──► Cancelled（本地取消）
Validated ──► Cancelled（本地取消）
PendingSubmit ──► Cancelled / Rejected / Expired
Submitted ──► Cancelled / Rejected / Expired
Accepted ──► PartiallyFilled / Filled / Cancelled / Rejected / Expired

Filled（终态） / Cancelled（终态） / Rejected（终态） / Expired（终态）
  └── 不可再转移

Completed（聚合态）：Filled / Cancelled / Rejected / Expired 之一，用于统计展示
```

---

## 2. 状态机白名单

```rust
// state_machine.rs
Created        → [Validated, PendingSubmit, Cancelled, Rejected, Expired]
Validated      → [PendingSubmit, Cancelled, Rejected, Expired]
PendingSubmit  → [Submitted, Cancelled, Rejected, Expired]
Submitted      → [Accepted, PartiallyFilled, Filled, Cancelled, Rejected, Expired]
Accepted       → [PartiallyFilled, Filled, Cancelled, Rejected, Expired]
PartiallyFilled → [PartiallyFilled, Filled, Cancelled, Rejected, Expired]

Filled         → ∅ (终态)
Cancelled      → ∅ (终态)
Rejected       → ∅ (终态)
Expired        → ∅ (终态)
Completed      → ∅ (聚合态)
```

---

## 3. 创建 → 提交完整时序

```
T0: Execution 创建 CreateOrderInput（限价 BUY 100 @ 0.45）
T1: OMS.create_order(input)
    → Order::new 生成 order_id="OMS-20260723-000001"
    → status = Created, version = 1, filled = 0, remaining = 100
    → repository.save
    → event_bus.publish(OrderCreated)
    → metrics.total_created++

T2: OMS.validate_order(order, ctx)
    → validator.check_all (9 rules)
    → 若全部通过：
       → status = Validated, version = 2
       → repository.save
       → event_bus.publish(OrderValidated)
       → metrics.total_validated++
    → 若失败：
       → status = Rejected, version = 2
       → event_bus.publish(ValidationFailed + OrderRejected)
       → metrics.total_validation_failed++ + total_rejected++

T3: OMS.submit_order(order)
    → status = PendingSubmit (if from Created/Validated)
    → event_bus.publish(OrderPendingSubmit)
    → status = Submitted, version = 3
    → repository.save
    → event_bus.publish(OrderSubmitted)
    → metrics.total_submitted++
    → gateway.submit_order(OrderRequest)
    → 接收 GatewayResult

T4: apply_gateway_result(order, result)
    → 若 Rejected/Expired:
       → status = Rejected/Expired
       → event_bus.publish(GatewayError + OrderRejected)
    → 若 Accepted:
       → status = Accepted
       → set_exchange_order_id(<GW-ID>)
       → event_bus.publish(OrderAccepted)
       → metrics.total_accepted++
    → 若 PartiallyFilled:
       → update_fill(<filled>, <avg_price>, 0.0)
       → status = PartiallyFilled
       → event_bus.publish(OrderPartiallyFilled)
       → metrics.total_partially_filled++
    → 若 Filled:
       → update_fill(<filled>, <avg_price>, <slippage>)
       → status = Filled（终态）
       → event_bus.publish(OrderFilled)
       → metrics.total_filled++
    → 若 Cancelled:
       → status = Cancelled（终态）
       → event_bus.publish(OrderCancelled)
       → metrics.total_cancelled++
```

---

## 4. 取消流程

```
Execution → OMS.cancel_order(order, "用户取消")
  → 若 status.is_terminal():
       → 返回 GatewayResult(success=true, message="订单非活跃")
       → 不变更状态
  → 否则：
       → gateway.cancel_order(exchange_order_id)  // 若已有
       → status = Cancelled, version += 1
       → repository.save
       → event_bus.publish(OrderCancelled)
       → metrics.total_cancelled++
```

---

## 5. 替换流程

```
Execution → OMS.replace_order(old_order, new_input)
  → cancel_order(old, "替换订单")
  → create_order(new_input)
  → submit_order(new)
  → 返回 new_order
```

注意：

- `cancel_order` 在 `old` 已为终态时是 no-op，不会失败。
- `create_order` 对相同 `client_order_id` 返回已存在订单（幂等）。
- `submit_order` 推进新订单状态。

---

## 6. 状态变化历史

每次状态变化写入 `order.status_history`：

```rust
pub struct StatusChange {
    pub from: OrderStatus,
    pub to: OrderStatus,
    pub at: DateTime<Local>,
    pub reason: String,
    pub actor: String,  // "oms" | "validator" | "gateway" | "recovery"
}
```

同时通过 `Order::print_timeline()` 输出中文时间线：

```
【订单 OMS-20260723-000001】
  客户端订单 ID : CLI-DEMO-001
  状态          : 完全成交（100/100，成交率 100.0%）
  加权均价 / 滑点: 0.4520 / 0.44%
  ...
  状态变化历史：
    [16:32:41] 已创建 → 已创建（oms）：订单创建
    [16:32:42] 已创建 → 已校验（validator）：校验通过
    [16:32:42] 已校验 → 待提交（oms）：OMS 决策完成，等待提交
    [16:32:42] 待提交 → 已提交（oms）：已提交到 Gateway
    [16:32:42] 已提交 → 已接受（gateway）：Gateway 接受（已接受）
    [16:32:43] 已接受 → 完全成交（gateway）：Gateway 返回完全成交
```

---

## 7. 异常分支

### 7.1 校验失败

```
validator.validate → false
  → transition Created → Rejected
  → 不调用 Gateway
  → publish ValidationFailed + OrderRejected
  → repository.save
```

### 7.2 Gateway 拒绝

```
gateway.submit_order → success=false, status=Rejected
  → transition Submitted → Rejected
  → publish GatewayError + OrderRejected
  → repository.save
```

### 7.3 状态机拒绝

```
transition(Created, Filled)  // 非法跳级
  → StateTransitionRejected 错误
  → publish StateTransitionRejected
  → 状态不变，返回错误
```

### 7.4 重复 client_order_id

```
create_order(input) where client_order_id 已存在
  → 返回已存在 Order（不创建新订单）
  → 记 warn 日志
```

---

## 8. 时间戳与版本

- `created_at`：订单创建时间，永不修改。
- `updated_at`：每次状态变化时刷新。
- `version`：乐观锁字段，每次状态变化 +1。
- `retry_count`：重试次数（cancel / submit 失败重试时累加）。

---

## 9. 内存 vs CSV vs SQLite

| 实现 | 适用场景 | 特点 |
| --- | --- | --- |
| InMemoryRepository | 测试 / 实时运行 | 读写最快，重启丢失 |
| CsvRepository | 生产 / 回放 | 持久化到 CSV，append-only |
| SqliteRepository | P2-05+ | 接口预留，待实现 |

---

## 10. 总结

OMS 生命周期具有以下特性：

- ✅ **白名单状态机**：所有转移必须通过校验
- ✅ **完整审计**：所有状态变化记录 actor + reason + 时间戳
- ✅ **聚合终态**：Completed 用于统计展示
- ✅ **异常隔离**：状态机拒绝不破坏订单对象
- ✅ **幂等性**：相同 client_order_id 返回已存在订单
- ✅ **可恢复**：重启时通过 Recovery 对齐 Gateway 状态
