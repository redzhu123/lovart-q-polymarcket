# Event Flow（事件流）

## 系统级事件流

```
Gateway / Exchange
      │
      │ Trade Fill Event
      ▼
┌─────────────────┐
│ Settlement Engine │ ← process_fill()
│                   │
│ 发布事件:          │
│  FillReceived     │
│  ValidationPassed │
│  FeeCalculated    │
│  PositionUpdated  │
│  BalanceUpdated   │
│  PnLUpdated       │
│  LedgerRecorded   │
│  SettlementCompleted│
└────────┬──────────┘
         │
         │ SettlementEvent (via EventBus)
         ▼
┌─────────────────┐
│ Subscribers:      │
│  PMS (Portfolio)  │
│  Metrics          │
│  Audit            │
└─────────────────┘
```

## 事件类型

| 事件 | 中文名 | 说明 |
|------|--------|------|
| FillReceived | 接收成交 | 成交事件已接收 |
| ValidationPassed | 校验通过 | 所有校验规则通过 |
| ValidationFailed | 校验失败 | 校验未通过，终止结算 |
| FeeCalculated | 手续费已计算 | 手续费计算完成 |
| PositionUpdated | 持仓已更新 | 持仓状态已变更 |
| BalanceUpdated | 余额已更新 | 余额已变更 |
| PnLUpdated | 盈亏已更新 | 盈亏数据已更新 |
| LedgerRecorded | 流水已记录 | 资金流水已生成 |
| SettlementCompleted | 结算完成 | 结算成功完成 |
| SettlementFailed | 结算失败 | 结算失败 |

## 订阅者模式

其他模块通过实现 `SettlementSubscriber` trait 订阅结算事件：

```rust
pub trait SettlementSubscriber: Send + Sync {
    fn name(&self) -> &str;
    fn on_event(&self, event: &SettlementEvent) -> anyhow::Result<()>;
}
```

订阅者处理失败不影响 Settlement Engine 主流程（隔离保护）。
