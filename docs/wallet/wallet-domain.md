# Wallet Domain Model (P2-06)

## 概述

统一钱包领域模型，所有市场统一使用此 Domain。

## 核心类型

### Wallet

```rust
pub struct Wallet {
    pub wallet_id: String,
    pub name: String,
    pub accounts: Vec<Account>,
    pub network: Network,
    pub created_at: DateTime<Local>,
}
```

### Account

```rust
pub struct Account {
    pub account_id: String,
    pub name: String,
    pub address: Address,        // 自动脱敏
    pub wallet_id: String,
    pub chain_id: u64,
    pub network: Network,
    pub currency: Currency,
    pub active: bool,
    pub created_at: DateTime<Local>,
}
```

### Address

区块链地址，Display/Debug 自动脱敏：
- `reveal()` 显式获取
- `masked()` 保留前6后4
- 序列化保持原始值

### Network

支持的区块链网络：
- Ethereum (chain_id=1)
- Polygon (chain_id=137)
- PolygonMumbai (chain_id=80001)
- Arbitrum (chain_id=42161)
- Solana
- Custom(chain_id)

### Currency

支持的货币：
- USDC, USDT, ETH, MATIC, SOL
- Custom(symbol)

### Balance

```rust
pub struct Balance {
    pub account_id: String,
    pub available: f64,     // 可用
    pub locked: f64,        // 锁定（冻结中）
    pub total: f64,         // 总计
    pub currency: Currency,
    pub unrealized_pnl: f64,// 未实现盈亏
    pub realized_pnl: f64,  // 已实现盈亏
    pub updated_at: Option<DateTime<Local>>,
}
```

操作：
- `freeze(amount)` / `unfreeze(amount)` — 冻结/解冻
- `debit(amount)` — 从锁定中扣款
- `credit(amount)` — 入金

### Allowance

```rust
pub struct Allowance {
    pub allowance_id: String,
    pub account_id: String,
    pub spender: Address,
    pub amount: f64,         // 总额度
    pub used: f64,           // 已使用
    pub nonce: Nonce,
    pub expires_at: Option<DateTime<Local>>,
    pub revoked: bool,
}
```

操作：
- `consume(amount)` — 消耗额度
- `revoke()` — 撤销授权
- `is_usable()` — 是否可用
- `remaining()` — 剩余额度

### Nonce

交易序号：`Nonce(u64)`，支持 `increment()`。
