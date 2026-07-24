# Settlement Workflow（结算工作流）

## 完整流程

```
1. Trade Fill Event 到达
   │
2. Validation（7 条规则）
   ├── 成交价格合法（0 < price ≤ 1.0）
   ├── 成交数量合法（qty > 0）
   ├── 成交方向合法
   ├── 余额充足（买时需要）
   ├── 手续费正确
   ├── 持仓状态合法（卖时需要）
   └── 结算一致性（字段完整性）
   │
   ├── 失败 → 终止结算，返回 SettlementStatus::ValidationFailed
   │
3. Fee Calculation
   └── 按 Maker/Taker 身份计算手续费
   │
4. Position Update
   ├── Buy → 开仓或加仓
   └── Sell → 减仓或平仓
   │
5. Balance Update
   ├── Buy → 冻结 + 扣款
   └── Sell → 入账
   │
6. PnL Update
   └── 记录已实现盈亏
   │
7. Ledger Entry
   └── 生成 1~2 条资金流水
   │
8. Settlement Completed
   ├── 更新 Metrics
   ├── 发布 SettlementEvent
   └── 持久化到 Repository
```

## 事件流

每次结算会发布以下事件（按顺序）：

1. `FillReceived` — 接收成交
2. `ValidationPassed` — 校验通过（或 `ValidationFailed` 终止）
3. `FeeCalculated` — 手续费已计算
4. `PositionUpdated` — 持仓已更新
5. `BalanceUpdated` — 余额已更新
6. `PnLUpdated` — 盈亏已更新
7. `LedgerRecorded` — 流水已记录
8. `SettlementCompleted` — 结算完成
