# PMS Repository — 持久化仓库

## PortfolioRepository Trait

```rust
pub trait PortfolioRepository: Send + Sync {
    fn save_portfolio(&self, portfolio: &Portfolio) -> anyhow::Result<()>;
    fn load_portfolio(&self) -> anyhow::Result<Option<Portfolio>>;
    fn save_positions(&self, positions: &[Position]) -> anyhow::Result<()>;
    fn load_positions(&self) -> anyhow::Result<Vec<Position>>;
    fn append_position(&self, position: &Position) -> anyhow::Result<()>;
    fn save_pnl_report(&self, report: &PnLReport) -> anyhow::Result<()>;
    fn load_pnl_reports(&self) -> anyhow::Result<Vec<PnLReport>>;
    fn name(&self) -> &str;
    fn storage_path(&self) -> Option<PathBuf>;
    fn health(&self) -> RepositoryHealth;
    fn clear(&self) -> anyhow::Result<()>;
}
```

## 实现

| 类型 | 说明 |
|------|------|
| InMemoryPortfolioRepository | 内存仓库（默认，测试用） |
| CsvPortfolioRepository | CSV 文件持久化 |
| SQLite（预留） | 未来支持 |

## 工厂函数

```rust
let repo = create_repository(
    RepositoryType::Memory,  // Memory / Csv / Sqlite
    None, None, None,        // CSV paths
)?;
```
