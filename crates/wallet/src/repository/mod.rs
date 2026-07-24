//! Wallet Repository Trait（P2-06 第六节）。
//!
//! 统一持久化接口：
//! - MemoryRepository：内存存储
//! - CsvRepository：CSV 文件持久化
//! - SqliteRepository：SQLite（接口预留）

use anyhow::Result;

pub mod csv;
pub mod memory;

use crate::domain::{Account, Allowance, Balance, Wallet};

// ============================================================================
// RepositoryType
// ============================================================================

/// 仓库类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryType {
    Memory,
    Csv,
    Sqlite,
}

impl RepositoryType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RepositoryType::Memory => "内存",
            RepositoryType::Csv => "CSV 文件",
            RepositoryType::Sqlite => "SQLite（接口预留）",
        }
    }
}

// ============================================================================
// RepositoryHealth
// ============================================================================

/// 仓库健康状态。
#[derive(Debug, Clone)]
pub struct RepositoryHealth {
    pub name: String,
    pub repository_type: RepositoryType,
    pub wallet_count: usize,
    pub account_count: usize,
    pub balance_count: usize,
    pub allowance_count: usize,
    pub ok: bool,
}

impl RepositoryHealth {
    pub fn ok(name: &str, repo_type: RepositoryType) -> Self {
        Self {
            name: name.to_string(),
            repository_type: repo_type,
            wallet_count: 0,
            account_count: 0,
            balance_count: 0,
            allowance_count: 0,
            ok: true,
        }
    }

    pub fn unhealthy(name: &str, repo_type: RepositoryType) -> Self {
        Self {
            name: name.to_string(),
            repository_type: repo_type,
            wallet_count: 0,
            account_count: 0,
            balance_count: 0,
            allowance_count: 0,
            ok: false,
        }
    }

    pub fn summary_zh(&self) -> String {
        format!(
            "仓库: {} | 类型: {} | 钱包: {} | 账户: {} | 余额: {} | 授权: {} | 健康: {}",
            self.name,
            self.repository_type.as_zh(),
            self.wallet_count,
            self.account_count,
            self.balance_count,
            self.allowance_count,
            if self.ok { "✅" } else { "❌" },
        )
    }
}

// ============================================================================
// WalletRepository Trait
// ============================================================================

/// 钱包持久化接口（P2-06 第六节）。
pub trait WalletRepository: Send + Sync {
    /// 仓库名称。
    fn name(&self) -> &str;

    /// 仓库类型。
    fn repository_type(&self) -> RepositoryType;

    /// 健康检查。
    fn health(&self) -> RepositoryHealth;

    // ---- Wallet ----

    /// 保存钱包。
    fn save_wallet(&mut self, wallet: &Wallet) -> Result<()>;

    /// 获取钱包。
    fn get_wallet(&self, wallet_id: &str) -> Result<Option<Wallet>>;

    /// 列出所有钱包。
    fn list_wallets(&self) -> Result<Vec<Wallet>>;

    // ---- Account ----

    /// 保存账户。
    fn save_account(&mut self, account: &Account) -> Result<()>;

    /// 获取账户。
    fn get_account(&self, account_id: &str) -> Result<Option<Account>>;

    /// 列出所有账户。
    fn list_accounts(&self) -> Result<Vec<Account>>;

    /// 按地址查找账户。
    fn find_account_by_address(&self, address: &str, chain_id: u64) -> Result<Option<Account>>;

    // ---- Balance ----

    /// 保存余额。
    fn save_balance(&mut self, balance: &Balance) -> Result<()>;

    /// 获取余额。
    fn get_balance(&self, account_id: &str, currency: &str) -> Result<Option<Balance>>;

    /// 列出账户的所有余额。
    fn list_balances(&self, account_id: &str) -> Result<Vec<Balance>>;

    // ---- Allowance ----

    /// 保存授权。
    fn save_allowance(&mut self, allowance: &Allowance) -> Result<()>;

    /// 获取授权。
    fn get_allowance(&self, allowance_id: &str) -> Result<Option<Allowance>>;

    /// 列出账户的所有授权。
    fn list_allowances(&self, account_id: &str) -> Result<Vec<Allowance>>;
}

// ============================================================================
// 工厂函数
// ============================================================================

use crate::repository::csv::CsvWalletRepository;
use crate::repository::memory::InMemoryWalletRepository;

/// 创建仓库。
pub fn create_repository(
    repo_type: RepositoryType,
    csv_path: Option<std::path::PathBuf>,
) -> Result<Box<dyn WalletRepository>> {
    match repo_type {
        RepositoryType::Memory => Ok(Box::new(InMemoryWalletRepository::new())),
        RepositoryType::Csv => {
            let path = csv_path.unwrap_or_else(|| std::path::PathBuf::from("data/wallet"));
            Ok(Box::new(CsvWalletRepository::new(path)?))
        }
        RepositoryType::Sqlite => {
            anyhow::bail!("SQLite 仓库接口预留，未实现")
        }
    }
}
