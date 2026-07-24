# Ledger（资金流水）

## 概述

统一资金流水记录。所有资金变化必须生成 Ledger 条目。

## 约束

- **只能追加（Append Only）**
- 禁止修改已记录的条目
- 每条包含完整的余额变化信息

## 数据结构

```rust
pub struct LedgerEntry {
    pub ledger_id: String,       // LEDGER-YYYYMMDD-NNNNNN
    pub trade_id: String,        // 关联成交 ID
    pub order_id: String,        // 关联订单 ID
    pub account_id: String,      // 账户 ID
    pub asset: String,           // 资产（默认 USDC）
    pub amount: f64,             // 变动金额（正=入账，负=出账）
    pub fee: f64,                // 手续费
    pub direction: LedgerDirection, // Debit/Credit
    pub before_balance: f64,     // 变动前余额
    pub after_balance: f64,      // 变动后余额
    pub description: String,     // 中文摘要
    pub timestamp: DateTime<Local>,
}
```

## 汇总指标

- `total_credits()` — 总入账金额
- `total_debits()` — 总出账金额
- `total_fees()` — 总手续费
- `net_flow()` — 净流量 = 入账 - 出账 - 手续费

## CSV 导出

`Ledger::to_csv()` 返回完整的 CSV 字符串，包含 12 列。
