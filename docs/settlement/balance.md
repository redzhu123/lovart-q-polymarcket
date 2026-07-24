# Balance Settlement（余额结算）

## 概述

所有成交驱动的余额变化均由 Balance Manager 统一处理。

## 余额模型

```rust
pub struct BalanceState {
    pub account_id: String,
    pub asset: String,            // USDC
    pub available: f64,           // 可用余额
    pub frozen: f64,              // 冻结余额
    pub reserved: f64,            // 预留余额（接口预留）
    pub equity: f64,              // 账户权益 = available + frozen + reserved
    pub wallet_balance: f64,      // 交易所钱包余额
    pub nav: f64,                 // 净资产价值
}
```

## 操作

| 操作 | 说明 |
|------|------|
| `freeze(amount)` | 冻结资金（可用→冻结） |
| `unfreeze(amount)` | 释放冻结（冻结→可用） |
| `debit(amount)` | 成交扣款（优先从冻结扣） |
| `credit(amount)` | 平仓入账 |
| `charge_fee(fee)` | 扣除手续费 |

## 资金流水

每次余额变化都会生成 Ledger 条目，记录变动前后的余额。
