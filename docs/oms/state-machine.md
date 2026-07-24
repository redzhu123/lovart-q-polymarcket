# OMS 状态机详解（P2-04 第三节）

> 所有合法状态转移由 [`StateMachine`](../../crates/oms/src/state_machine.rs) 集中管理。

---

## 1. 状态定义

### 1.1 OrderStatus（11 态 + 1 聚合）

| 状态 | 中文 | 性质 | 说明 |
| --- | --- | --- | --- |
| `Created` | 已创建 | 初始 | `Order::new` 后的状态 |
| `Validated` | 已校验 | 中间 | Validator 通过 |
| `PendingSubmit` | 待提交 | 中间 | OMS 决策完成，排队中 |
| `Submitted` | 已提交 | 中间 | 已发往 Gateway |
| `Accepted` | 已接受 | 中间 | Gateway 接受 |
| `PartiallyFilled` | 部分成交 | 中间 | 可自转（多次部分成交） |
| `Filled` | 完全成交 | **终态** | 订单完成 |
| `Cancelled` | 已取消 | **终态** | 用户/系统取消 |
| `Rejected` | 已拒绝 | **终态** | 校验 / Gateway 拒绝 |
| `Expired` | 已过期 | **终态** | 订单超时过期 |
| `Completed` | 已完成 | **聚合态** | 统计展示用 |

### 1.2 终态判定

```rust
pub fn is_terminal(&self) -> bool {
    matches!(self, Filled | Cancelled | Rejected | Expired)
}
```

注意：`Completed` **不是** 终态（它不表示真实订单状态），仅用于聚合统计。

---

## 2. 合法转移图

```
                    ┌─────────────┐
                    │   Created   │ 初始态
                    └──────┬──────┘
                           │
                           ▼
                    ┌─────────────┐
                    │  Validated  │
                    └──────┬──────┘
                           │
                           ▼
                 ┌───────────────────┐
                 │  PendingSubmit    │
                 └──────┬────────────┘
                        │
                        ▼
                 ┌───────────────────┐
                 │    Submitted      │
                 └──────┬────────────┘
                        │
                        ▼
                 ┌───────────────────┐
                 │    Accepted       │
                 └──────┬────────────┘
                        │
       ┌────────────────┼─────────────┐
       │                │             │
       ▼                ▼             ▼
┌──────────┐  ┌──────────────────┐  ┌──────────┐
│  Filled  │  │ PartiallyFilled  │  │Cancelled │
│ (终态)   │  │  (可自转)        │  │ (终态)   │
└──────────┘  └────────┬─────────┘  └──────────┘
                       │
                       ▼
                  ┌──────────┐
                  │  Filled  │
                  │ (终态)   │
                  └──────────┘

任一非终态 ──► Rejected / Expired (终态)
```

---

## 3. 白名单实现

```rust
// crates/oms/src/state_machine.rs
let mut transitions = HashMap::new();

transitions.insert(Created, vec![
    Validated, PendingSubmit, Cancelled, Rejected, Expired
]);
transitions.insert(Validated, vec![
    PendingSubmit, Cancelled, Rejected, Expired
]);
transitions.insert(PendingSubmit, vec![
    Submitted, Cancelled, Rejected, Expired
]);
transitions.insert(Submitted, vec![
    Accepted, PartiallyFilled, Filled, Cancelled, Rejected, Expired
]);
transitions.insert(Accepted, vec![
    PartiallyFilled, Filled, Cancelled, Rejected, Expired
]);
transitions.insert(PartiallyFilled, vec![
    PartiallyFilled, Filled, Cancelled, Rejected, Expired
]);
// 终态：空转移
```

---

## 4. 校验逻辑

### 4.1 校验函数

```rust
pub fn validate_transition(
    &self,
    from: OrderStatus,
    to: OrderStatus,
) -> Result<(), TransitionError> {
    // 1. 终态禁止再转移
    if from.is_terminal() {
        return Err(TransitionError::from_terminal(...));
    }

    // 2. 同状态自转：仅 PartiallyFilled 合法
    if from == to && from != PartiallyFilled {
        return Err(TransitionError::same_state(...));
    }

    // 3. 查白名单
    match self.transitions.get(&from) {
        Some(allowed) if allowed.contains(&to) => Ok(()),
        Some(allowed) => Err(TransitionError::not_allowed(...)),
        None => Err(TransitionError::not_allowed(...)),
    }
}
```

### 4.2 错误类型

```rust
pub enum TransitionError {
    /// 终态禁止再转移
    FromTerminal {
        message: String,
        from: OrderStatus,
        to: OrderStatus,
    },
    /// 不允许自转（除 PartiallyFilled）
    SameState {
        message: String,
        state: OrderStatus,
    },
    /// 不在白名单
    NotAllowed {
        message: String,
        from: OrderStatus,
        to: OrderStatus,
    },
}
```

错误消息均为**中文**：

- `"状态「已取消」为终态，不允许再转移"`
- `"不允许自转（已创建 → 已创建）"`
- `"从「已创建」到「已提交」的转移不在白名单（允许的目标：已校验、待提交、已取消、已拒绝、已过期）"`

---

## 5. 应用转移

OMS 通过 `lifecycle::apply_transition` 集中应用所有转移：

```rust
fn apply_transition(
    order: &mut Order,
    target: OrderStatus,
    reason: &str,
    actor: &str,
    ctx: &LifecycleContext,
    now: DateTime<Local>,
) -> anyhow::Result<()> {
    // 1. 状态机校验
    if let Err(e) = ctx.state_machine.validate_transition(order.status, target) {
        // 2. 发状态机拒绝事件
        ctx.event_bus.publish(StateTransitionRejected { ... });
        // 3. 返回错误（不修改状态）
        return Err(anyhow::anyhow!("OMS 状态机非法转移..."));
    }

    // 4. 修改 Order 状态 + 记录历史
    order.transition(target, reason, actor, now);

    // 5. 持久化状态变化
    ctx.repository.append_status_change(&order.order_id, change)?;
    Ok(())
}
```

---

## 6. 设计原则

### 6.1 白名单 vs 黑名单

选择**白名单**（deny by default）的原因：

- 新增状态时，必须显式声明可达性，避免意外跳级
- 防止后续维护中遗漏边界条件
- 非法转移立即报错，便于发现 bug

### 6.2 不可逆

终态（Filled / Cancelled / Rejected / Expired）不接受任何再转移：

- 即使是 Rejected → Rejected 也不允许（避免循环）
- 终态的 Order 不能再被 submit / cancel / replace

### 6.3 聚合态

`Completed` 是聚合终态，仅在统计 / 展示时使用：

- 实际订单状态机中不会出现（不会 transition 到 Completed）
- `is_terminal()` 返回 false（防止误判）
- 用于把 Filled/Cancelled/Rejected/Expired 统一为"已完成"

---

## 7. 状态机图（CLI 输出）

`cargo run -- oms` 会输出 ASCII 状态图：

```
OMS 订单状态机（11 态 + 1 聚合）：

  已创建 (Created)
     │ validator 通过
     ▼
  已校验 (Validated)
     │ OMS 决策
     ▼
  待提交 (PendingSubmit)
     │ oms 提交
     ▼
  已提交 (Submitted)
     │ gateway 接受
     ▼
  已接受 (Accepted) ─── gateway 取消 ──► 已取消 (Cancelled)（终态）
     │
     ├── 部分成交 ──► 部分成交 (PartiallyFilled) ──► 部分成交 （持续）
     │                    │
     │                    └─► 完全成交 (Filled)（终态）
     └─► 完全成交 (Filled)（终态）

  任一非终态（已创建/已校验/待提交/已提交/已接受/部分成交）
     可进入 已拒绝 (Rejected) / 已过期 (Expired)（终态）

  已完成 (Completed)：聚合终态，用于统计展示。
```

---

## 8. 状态机事件

### 8.1 状态变化事件

每次成功 transition 会发布对应事件：

| 转移 | 事件 |
| --- | --- |
| Created → Validated | `OrderValidated` |
| Validated → PendingSubmit | `OrderPendingSubmit` |
| PendingSubmit → Submitted | `OrderSubmitted` |
| Submitted → Accepted | `OrderAccepted` |
| Submitted → PartiallyFilled | `OrderPartiallyFilled` |
| Submitted → Filled | `OrderFilled` |
| Accepted → PartiallyFilled | `OrderPartiallyFilled` |
| Accepted → Filled | `OrderFilled` |
| * → Cancelled | `OrderCancelled` |
| * → Rejected | `OrderRejected` |
| * → Expired | `OrderExpired` |

### 8.2 拒绝事件

状态机拒绝时发布：

```rust
OrderEvent::StateTransitionRejected {
    order_id: String,
    from: OrderStatus,
    to: OrderStatus,
    reason: String,
    timestamp: DateTime<Local>,
}
```

---

## 9. 测试覆盖

参见 [`tests/state_machine.rs`](../../crates/oms/tests/state_machine.rs)：

- ✅ 完整 happy path（7 个状态）
- ✅ 所有终态不可变
- ✅ 所有活跃态可进入 Rejected / Expired
- ✅ PartiallyFilled 可自转
- ✅ Created / Validated 可直接 Cancelled
- ✅ Created → Submitted 非法跳级
- ✅ Validated → Accepted 非法跳级
- ✅ 中文错误消息

---

## 10. 总结

OMS 状态机具有以下特性：

- ✅ **白名单**：所有转移必须显式声明
- ✅ **不可逆**：终态禁止再转移
- ✅ **聚合态**：Completed 用于统计
- ✅ **中文错误**：所有错误消息为中文
- ✅ **审计完整**：每次转移记录 actor + reason + 时间戳
- ✅ **可视化**：CLI 输出 ASCII 状态图
