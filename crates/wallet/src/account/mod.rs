//! Account Manager（P2-06 第四节）。
//!
//! 多账户管理：
//! - 创建/查找/列出账户
//! - 按地址/网络搜索
//! - 激活/停用账户

use crate::domain::{Account, Address, Currency, Network};
use anyhow::Result;

// ============================================================================
// AccountManager
// ============================================================================

/// 账户管理器（P2-06 第四节）。
///
/// 管理多个钱包账户的生命周期。
pub struct AccountManager {
    accounts: Vec<Account>,
    next_id_counter: u64,
}

impl AccountManager {
    /// 创建账户管理器。
    pub fn new(initial_accounts: Vec<Account>) -> Self {
        let max_id = initial_accounts
            .iter()
            .filter_map(|a| {
                a.account_id
                    .strip_prefix("ACC-")
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0);

        Self {
            accounts: initial_accounts,
            next_id_counter: max_id + 1,
        }
    }

    /// 创建空账户管理器。
    pub fn empty() -> Self {
        Self {
            accounts: Vec::new(),
            next_id_counter: 1,
        }
    }

    /// 创建新账户。
    pub fn create_account(
        &mut self,
        name: &str,
        address: Address,
        wallet_id: &str,
        network: Network,
        currency: Currency,
    ) -> Result<&Account> {
        let account_id = format!("ACC-{:04}", self.next_id_counter);
        self.next_id_counter += 1;

        let account = Account::new(
            account_id,
            name.to_string(),
            address,
            wallet_id.to_string(),
            network,
            currency,
        );

        tracing::info!(
            account_id = %account.account_id,
            name = %account.name,
            wallet_id = %account.wallet_id,
            network = %account.network.short_name(),
            "创建账户"
        );

        self.accounts.push(account);
        Ok(self.accounts.last().unwrap())
    }

    /// 查找账户（按 ID）。
    pub fn find_by_id(&self, account_id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.account_id == account_id)
    }

    /// 查找账户（按地址 + 网络）。
    pub fn find_by_address(&self, address: &Address, network: Network) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| &a.address == address && a.network == network)
    }

    /// 获取所有账户。
    pub fn all(&self) -> &[Account] {
        &self.accounts
    }

    /// 获取活跃账户。
    pub fn active(&self) -> Vec<&Account> {
        self.accounts.iter().filter(|a| a.active).collect()
    }

    /// 账户数量。
    pub fn count(&self) -> usize {
        self.accounts.len()
    }

    /// 活跃账户数量。
    pub fn active_count(&self) -> usize {
        self.accounts.iter().filter(|a| a.active).count()
    }

    /// 停用账户。
    pub fn deactivate(&mut self, account_id: &str) -> Result<()> {
        if let Some(account) = self
            .accounts
            .iter_mut()
            .find(|a| a.account_id == account_id)
        {
            account.active = false;
            tracing::info!(account_id = %account_id, "账户已停用");
            Ok(())
        } else {
            anyhow::bail!("账户不存在: {}", account_id)
        }
    }

    /// 激活账户。
    pub fn activate(&mut self, account_id: &str) -> Result<()> {
        if let Some(account) = self
            .accounts
            .iter_mut()
            .find(|a| a.account_id == account_id)
        {
            account.active = true;
            tracing::info!(account_id = %account_id, "账户已激活");
            Ok(())
        } else {
            anyhow::bail!("账户不存在: {}", account_id)
        }
    }

    /// 健康检查。
    pub fn health(&self) -> AccountManagerHealth {
        AccountManagerHealth {
            total: self.count(),
            active: self.active_count(),
            networks: self
                .accounts
                .iter()
                .map(|a| a.network)
                .collect::<std::collections::HashSet<_>>()
                .len(),
        }
    }

    /// 打印所有账户（中文）。
    pub fn print_zh(&self) {
        println!();
        println!("══════════════════════════════════════");
        println!("  账户列表");
        println!("══════════════════════════════════════");
        println!();
        println!("  总数: {} | 活跃: {}", self.count(), self.active_count());
        println!();

        if self.accounts.is_empty() {
            println!("  （无账户）");
        } else {
            for a in &self.accounts {
                println!(
                    "  {} | {} | {} | {} | {}",
                    a.account_id,
                    a.name,
                    a.address,
                    a.network.short_name(),
                    if a.active { "活跃" } else { "已停用" },
                );
            }
        }
        println!();
    }
}

// ============================================================================
// AccountManagerHealth
// ============================================================================

/// 账户管理器健康状态。
#[derive(Debug, Clone)]
pub struct AccountManagerHealth {
    pub total: usize,
    pub active: usize,
    pub networks: usize,
}

impl AccountManagerHealth {
    pub fn summary_zh(&self) -> String {
        format!(
            "总账户: {} | 活跃: {} | 网络覆盖: {} 个",
            self.total, self.active, self.networks,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_manager_empty() {
        let mgr = AccountManager::empty();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.all().is_empty());
    }

    #[test]
    fn account_manager_create() {
        let mut mgr = AccountManager::empty();
        let result = mgr.create_account(
            "测试账户",
            Address::new("0x1234"),
            "W1",
            Network::Polygon,
            Currency::USDC,
        );
        assert!(result.is_ok());
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn account_manager_find() {
        let mut mgr = AccountManager::empty();
        mgr.create_account(
            "A1",
            Address::new("0xAAAA"),
            "W1",
            Network::Polygon,
            Currency::USDC,
        )
        .unwrap();
        mgr.create_account(
            "A2",
            Address::new("0xBBBB"),
            "W1",
            Network::Ethereum,
            Currency::ETH,
        )
        .unwrap();

        assert!(mgr.find_by_id("ACC-0001").is_some());
        assert!(mgr.find_by_id("NONEXISTENT").is_none());
    }

    #[test]
    fn account_manager_deactivate_activate() {
        let mut mgr = AccountManager::empty();
        mgr.create_account(
            "A1",
            Address::new("0xAAAA"),
            "W1",
            Network::Polygon,
            Currency::USDC,
        )
        .unwrap();

        mgr.deactivate("ACC-0001").unwrap();
        assert_eq!(mgr.active_count(), 0);

        mgr.activate("ACC-0001").unwrap();
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn account_manager_health() {
        let mut mgr = AccountManager::empty();
        mgr.create_account(
            "A1",
            Address::new("0xA"),
            "W1",
            Network::Polygon,
            Currency::USDC,
        )
        .unwrap();
        mgr.create_account(
            "A2",
            Address::new("0xB"),
            "W1",
            Network::Ethereum,
            Currency::ETH,
        )
        .unwrap();

        let h = mgr.health();
        assert_eq!(h.total, 2);
        assert_eq!(h.active, 2);
        assert_eq!(h.networks, 2);
    }
}
