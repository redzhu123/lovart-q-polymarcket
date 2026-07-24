# PMS 事件模型

## 架构

PMS 通过实现 `pm_oms::events::Subscriber` trait 监听 OMS 事件。

```
OMS EventBus
     │
     ▼
PmsEventSubscriber
     │
     ├── OrderFilled → 开仓/加仓 + 扣款
     ├── OrderCancelled → 释放冻结资金
     └── OrderRejected → 释放冻结资金
```

## 实现

```rust
pub struct PmsEventSubscriber {
    name: String,
    handler: PmsEventHandler,
}

impl Subscriber for PmsEventSubscriber {
    fn name(&self) -> &str { &self.name }
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        // 分发到 PMS handler
    }
}
```

## 设计原则

- PMS 不主动调用 OMS
- 事件处理失败隔离（不影响 OMS 主流程）
- 闭包模式：handler 由 `Pms` 提供，避免循环依赖
