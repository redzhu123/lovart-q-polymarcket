# Wallet Repository (P2-06)

## WalletRepository Trait

```rust
pub trait WalletRepository: Send + Sync {
    fn name(&self) -> &str;
    fn repository_type(&self) -> RepositoryType;
    fn health(&self) -> RepositoryHealth;

    // Wallet CRUD
    fn save_wallet(&mut self, wallet: &Wallet) -> Result<()>;
    fn get_wallet(&self, wallet_id: &str) -> Result<Option<Wallet>>;
    fn list_wallets(&self) -> Result<Vec<Wallet>>;

    // Account CRUD
    fn save_account(&mut self, account: &Account) -> Result<()>;
    fn get_account(&self, account_id: &str) -> Result<Option<Account>>;
    fn find_account_by_address(&self, address: &str, chain_id: u64) -> Result<Option<Account>>;

    // Balance & Allowance CRUD
    // ...
}
```

## 实现

### InMemoryWalletRepository

- HashMap 存储
- 零配置，开箱即用
- 进程重启后数据丢失

### CsvWalletRepository

- CSV 文件持久化
- 分文件存储：wallets.csv / accounts.csv / balances.csv / allowances.csv
- 适合少量数据场景

### SqliteWalletRepository

- 接口预留
- 返回 "未实现" 错误
- 未来支持

## 工厂函数

```rust
pub fn create_repository(
    repo_type: RepositoryType,
    csv_path: Option<PathBuf>,
) -> Result<Box<dyn WalletRepository>>
```

## RepositoryHealth

```rust
pub struct RepositoryHealth {
    pub name: String,
    pub repository_type: RepositoryType,
    pub wallet_count: usize,
    pub account_count: usize,
    pub balance_count: usize,
    pub allowance_count: usize,
    pub ok: bool,
}
```
