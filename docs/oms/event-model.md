# OMS 事件模型（P2-04 第六节）

> 所有 OMS 订单状态变化通过 EventBus 发布事件。Portfolio / Metrics / Audit 通过订阅事件工作。

---

## 1. 设计原则

### 1.1 单向数据流

```
OMS 业务动作 → state machine → repository
                │
                ▼
            EventBus → publish(OrderEvent)
                                │
                ┌───────────────┼───────────────┐
                ▼               ▼               ▼
          Portfolio        Metrics          Audit
          (Subscriber)     (Subscriber)     (Subscriber)
```

OMS **不直接调用** Portfolio / Metrics / Audit 模块，仅发布事件。

### 1.2 同步分发

EventBus 是同步的：

- `publish()` 立即调用所有 Subscriber
- Subscriber 失败不影响其他 Subscriber
- 失败仅记 `warn` 日志

### 1.3 失败隔离

```rust
pub fn publish(&self, event: OrderEvent) {
    // 1. 计数
    // 2. 记 info 日志
    // 3. 遍历 subscribers
    for sub in subs.iter() {
        if let Err(e) = sub.on_event(&event) {
            // 隔离：仅记 warn，不影响其他 subscriber
            tracing::warn!(subscriber = %sub.name(), error = %e, "...");
        }
    }
}
```

---

## 2. OrderEvent 枚举

### 2.1 全部事件类型（15 种）

| 事件 | 中文 | 触发时机 |
| --- | --- | --- |
| `OrderCreated` | 订单创建 | `lifecycle::create_order` |
| `OrderValidated` | 校验通过 | Validator 通过 |
| `OrderPendingSubmit` | 待提交 | 进入 PendingSubmit |
| `OrderSubmitted` | 已提交 | 调用 `gateway.submit_order` 前 |
| `OrderAccepted` | 已接受 | Gateway 返回 Accepted |
| `OrderPartiallyFilled` | 部分成交 | Gateway 返回 PartiallyFilled |
| `OrderFilled` | 完全成交 | Gateway 返回 Filled |
| `OrderCancelled` | 已取消 | 用户/Gateway 取消 |
| `OrderRejected` | 已拒绝 | Validator 或 Gateway 拒绝 |
| `OrderExpired` | 已过期 | 订单超时过期 |
| `ValidationFailed` | 校验失败 | Validator 拒绝 |
| `GatewayError` | 网关错误 | Gateway 返回错误 |
| `StateTransitionRejected` | 状态机拒绝 | 非法状态转移 |
| `RecoveryCompleted` | 恢复完成 | 程序启动恢复结束 |

### 2.2 事件结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum OrderEvent {
    OrderCreated {
        order_id: String,
        client_order_id: String,
        market_id: String,
        timestamp: DateTime<Local>,
    },
    OrderValidated {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    OrderPendingSubmit {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    OrderSubmitted {
        order_id: String,
        gateway: String,
        timestamp: DateTime<Local>,
    },
    OrderAccepted {
        order_id: String,
        gateway: String,
        exchange_order_id: String,
        timestamp: DateTime<Local>,
    },
    OrderPartiallyFilled {
        order_id: String,
        filled: f64,
        remaining: f64,
        avg_price: f64,
        timestamp: DateTime<Local>,
    },
    OrderFilled {
        order_id: String,
        avg_price: f64,
        slippage: f64,
        timestamp: DateTime<Local>,
    },
    OrderCancelled {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    OrderRejected {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    OrderExpired {
        order_id: String,
        timestamp: DateTime<Local>,
    },
    ValidationFailed {
        order_id: String,
        reason: String,
        timestamp: DateTime<Local>,
    },
    GatewayError {
        order_id: String,
        gateway: String,
        error: String,
        timestamp: DateTime<Local>,
    },
    StateTransitionRejected {
        order_id: String,
        from: OrderStatus,
        to: OrderStatus,
        reason: String,
        timestamp: DateTime<Local>,
    },
    RecoveryCompleted {
        recovered_count: usize,
        timestamp: DateTime<Local>,
    },
}
```

### 2.3 事件方法

```rust
impl OrderEvent {
    /// 英文标识（CSV 字段）
    pub fn event_name(&self) -> &'static str;

    /// 中文名（CLI 显示）
    pub fn event_name_zh(&self) -> &'static str;

    /// 关联订单 ID
    pub fn order_id(&self) -> &str;

    /// 时间戳
    pub fn timestamp(&self) -> DateTime<Local>;
}
```

---

## 3. Subscriber Trait

```rust
pub trait Subscriber: Send + Sync {
    fn name(&self) -> &str;
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()>;
}
```

实现示例（OmsMetrics）：

```rust
pub struct OmsMetricsSubscriber {
    sink: Arc<Mutex<OmsMetrics>>,
}

impl Subscriber for OmsMetricsSubscriber {
    fn name(&self) -> &str { "OmsMetricsSubscriber" }

    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        let mut m = self.sink.lock().unwrap();
        m.record(event);
        Ok(())
    }
}
```

---

## 4. EventBus

### 4.1 公共 API

```rust
impl EventBus {
    pub fn new() -> Self;
    pub fn subscribe(&mut self, sub: Box<dyn Subscriber>);
    pub fn subscriber_count(&self) -> usize;
    pub fn published_count(&self) -> u64;
    pub fn publish(&self, event: OrderEvent);
    pub fn record_only(&self, event: OrderEvent);
}
```

### 4.2 默认 Subscriber

`Oms::new()` 自动注册 `OmsMetricsSubscriber`（可关闭）：

```rust
let cfg = OmsConfig {
    subscribe_metrics: true,  // 默认
    ..
};
Oms::new(cfg, gateway)?;
// event_bus.subscriber_count() == 1
```

### 4.3 自定义 Subscriber

```rust
struct AuditSubscriber { db: Arc<Database> }
impl Subscriber for AuditSubscriber {
    fn name(&self) -> &str { "Audit" }
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        self.db.insert_event(event)?;
        Ok(())
    }
}

let mut oms = Oms::new(cfg, gateway)?;
oms.subscribe(Box::new(AuditSubscriber { db }));
```

---

## 5. CSV 持久化

### 5.1 events.csv 表头

```csv
timestamp,event_type,event_name_zh,order_id,extra_json
```

### 5.2 CSV 行示例

```csv
2026-07-23 16:32:41.110,OrderCreated,订单创建,OMS-20260723-000001,{"event_type":"OrderCreated","order_id":"OMS-20260723-000001","client_order_id":"CLI-DEMO-001","market_id":"mkt-btc-2024","timestamp":"2026-07-23T08:32:41.110Z"}
```

### 5.3 CLI 读取

`cargo run -- oms-events` 显示最近 50 条事件。

---

## 6. 完整事件流示例

```text
T0: oms.create_order
    → publish OrderCreated

T1: oms.validate_order (success)
    → publish OrderValidated

T2: oms.submit_order
    → publish OrderPendingSubmit
    → publish OrderSubmitted

T3: gateway.submit_order → success, status=Accepted
    → publish OrderAccepted

T4: gateway.submit_order → success, status=Filled
    → publish OrderFilled
```

---

## 7. 与状态机的协同

### 7.1 状态变化 → 事件映射

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
    ctx.state_machine.validate_transition(order.status, target)?;
    // 2. 修改状态
    order.transition(target, reason, actor, now);
    // 3. 持久化 StatusChange
    ctx.repository.append_status_change(...)?;
    // 4. 发布对应事件（在 caller 中）
    Ok(())
}
```

调用方根据 target 选择对应事件发布：

```rust
match target {
    OrderStatus::Validated => publish(OrderValidated { .. }),
    OrderStatus::Submitted => publish(OrderSubmitted { .. }),
    OrderStatus::Accepted => publish(OrderAccepted { .. }),
    OrderStatus::Filled => publish(OrderFilled { .. }),
    OrderStatus::Cancelled => publish(OrderCancelled { .. }),
    OrderStatus::Rejected => publish(OrderRejected { .. }),
    OrderStatus::Expired => publish(OrderExpired { .. }),
    OrderStatus::PartiallyFilled => publish(OrderPartiallyFilled { .. }),
    _ => {}
}
```

### 7.2 Gateway 拒绝

```rust
if !result.success {
    // 发布 GatewayError
    publish(GatewayError { order_id, gateway, error });
    // 状态机转移到 Rejected
    transition(Rejected, ...);
    // 发布 OrderRejected
    publish(OrderRejected { order_id, reason });
}
```

---

## 8. 性能考量

- **同步分发**：每次 publish 立即触发所有 Subscriber。
  - 适合低频订单（每秒 < 100 单）。
  - 高频场景可改为异步通道（`tokio::mpsc`）。
- **不存全量事件**：EventBus 仅维护订阅者列表 + 计数。
  - 事件持久化由 Repository（CSV / SQLite）负责。
- **失败隔离**：单个 Subscriber 抛错不影响其他。

---

## 9. 测试覆盖

参见 [`tests/events.rs`](../../crates/oms/tests/events.rs)：

- ✅ OrderCreated / Validated / Submitted / Cancelled 事件发布
- ✅ 自定义 Subscriber 接收事件
- ✅ Subscriber 失败不破坏主流程
- ✅ 完整事件链（创建 → 校验 → 提交 → 接受/拒绝）
- ✅ CSV 行序列化格式

---

## 10. 总结

OMS 事件模型具有以下特性：

- ✅ **单向数据流**：OMS → EventBus → Subscriber
- ✅ **失败隔离**：Subscriber 失败不影响主流程
- ✅ **完整覆盖**：15 种事件覆盖所有订单生命周期
- ✅ **可扩展**：业务模块实现 Subscriber trait 即可订阅
- ✅ **可持久化**：CSV / SQLite 记录所有事件
- ✅ **中文友好**：所有事件有中英文名
