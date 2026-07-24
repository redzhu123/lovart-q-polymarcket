# Wallet Architecture (P2-06)

## 架构

```
Authentication
      │
      ▼
┌──────────────────────────┐
│  Wallet Domain           │
│  ├─ AccountManager       │  ← 多账户管理
│  ├─ BalanceManager       │  ← 余额追踪
│  ├─ AllowanceManager     │  ← 授权管理
│  └─ WalletSigner         │  ← 交易签名
└─────────┬────────────────┘
          │
┌─────────▼────────────────┐
│  WalletRepository        │  ← Memory / CSV / SQLite(预留)
└──────────────────────────┘
```

## 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| domain | `src/domain/mod.rs` | 统一领域模型 |
| account | `src/account/mod.rs` | 多账户管理 |
| balance | `src/balance/mod.rs` | 余额追踪与操作 |
| allowance | `src/allowance/mod.rs` | 授权额度管理 |
| signer | `src/signer/mod.rs` | 签名接口 |
| repository | `src/repository/mod.rs` | 持久化接口 |
| diagnostics | `src/diagnostics/mod.rs` | 健康诊断 |

## AccountManager

```rust
pub struct AccountManager {
    accounts: Vec<Account>,
    next_id_counter: u64,
}
```

- `create_account(name, address, wallet_id, network, currency)`
- `find_by_id(account_id)` / `find_by_address(address, network)`
- `deactivate(account_id)` / `activate(account_id)`

## BalanceManager

```rust
pub struct BalanceManager {
    balances: HashMap<(String, Currency), Balance>,
}
```

- `set_balance(balance)` / `get(account_id, currency)`
- `freeze()` / `unfreeze()` / `debit()` / `credit()`
- `total_by_currency(currency)` / `total_available_by_currency(currency)`

## AllowanceManager

```rust
pub struct AllowanceManager {
    allowances: Vec<Allowance>,
    next_id_counter: u64,
}
```

- `approve(account_id, spender, amount)`
- `consume(allowance_id, amount)`
- `revoke(allowance_id)`
- `purge_expired()`

## WalletRepository

```rust
pub trait WalletRepository: Send + Sync {
    fn save_wallet(&mut self, wallet: &Wallet) -> Result<()>;
    fn get_wallet(&self, wallet_id: &str) -> Result<Option<Wallet>>;
    fn list_wallets(&self) -> Result<Vec<Wallet>>;
    fn save_account(&mut self, account: &Account) -> Result<()>;
    // ... balances, allowances
}
```

实现：
- **InMemoryWalletRepository**：内存存储
- **CsvWalletRepository**：CSV 文件持久化
- **SqliteWalletRepository**：SQLite（接口预留）
