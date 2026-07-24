//! In-Memory Wallet Repository（P2-06 第六节）。

use anyhow::Result;
use std::collections::HashMap;
use std::sync::RwLock;

use super::{RepositoryHealth, RepositoryType, WalletRepository};
use crate::domain::{Account, Allowance, Balance, Wallet};

/// 内存钱包仓库。
pub struct InMemoryWalletRepository {
    wallets: RwLock<HashMap<String, Wallet>>,
    accounts: RwLock<HashMap<String, Account>>,
    balances: RwLock<HashMap<(String, String), Balance>>,
    allowances: RwLock<HashMap<String, Allowance>>,
}

impl InMemoryWalletRepository {
    pub fn new() -> Self {
        Self {
            wallets: RwLock::new(HashMap::new()),
            accounts: RwLock::new(HashMap::new()),
            balances: RwLock::new(HashMap::new()),
            allowances: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryWalletRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletRepository for InMemoryWalletRepository {
    fn name(&self) -> &str {
        "InMemoryWalletRepository"
    }

    fn repository_type(&self) -> RepositoryType {
        RepositoryType::Memory
    }

    fn health(&self) -> RepositoryHealth {
        let mut h = RepositoryHealth::ok("InMemoryWalletRepository", RepositoryType::Memory);
        h.wallet_count = self.wallets.read().unwrap().len();
        h.account_count = self.accounts.read().unwrap().len();
        h.balance_count = self.balances.read().unwrap().len();
        h.allowance_count = self.allowances.read().unwrap().len();
        h
    }

    fn save_wallet(&mut self, wallet: &Wallet) -> Result<()> {
        self.wallets
            .write()
            .unwrap()
            .insert(wallet.wallet_id.clone(), wallet.clone());
        Ok(())
    }

    fn get_wallet(&self, wallet_id: &str) -> Result<Option<Wallet>> {
        Ok(self.wallets.read().unwrap().get(wallet_id).cloned())
    }

    fn list_wallets(&self) -> Result<Vec<Wallet>> {
        Ok(self.wallets.read().unwrap().values().cloned().collect())
    }

    fn save_account(&mut self, account: &Account) -> Result<()> {
        self.accounts
            .write()
            .unwrap()
            .insert(account.account_id.clone(), account.clone());
        Ok(())
    }

    fn get_account(&self, account_id: &str) -> Result<Option<Account>> {
        Ok(self.accounts.read().unwrap().get(account_id).cloned())
    }

    fn list_accounts(&self) -> Result<Vec<Account>> {
        Ok(self.accounts.read().unwrap().values().cloned().collect())
    }

    fn find_account_by_address(&self, address: &str, chain_id: u64) -> Result<Option<Account>> {
        Ok(self
            .accounts
            .read()
            .unwrap()
            .values()
            .find(|a| a.address.reveal() == address && a.chain_id == chain_id)
            .cloned())
    }

    fn save_balance(&mut self, balance: &Balance) -> Result<()> {
        let key = (
            balance.account_id.clone(),
            balance.currency.symbol().to_string(),
        );
        self.balances.write().unwrap().insert(key, balance.clone());
        Ok(())
    }

    fn get_balance(&self, account_id: &str, currency: &str) -> Result<Option<Balance>> {
        Ok(self
            .balances
            .read()
            .unwrap()
            .get(&(account_id.to_string(), currency.to_string()))
            .cloned())
    }

    fn list_balances(&self, account_id: &str) -> Result<Vec<Balance>> {
        Ok(self
            .balances
            .read()
            .unwrap()
            .iter()
            .filter(|((aid, _), _)| aid == account_id)
            .map(|(_, b)| b.clone())
            .collect())
    }

    fn save_allowance(&mut self, allowance: &Allowance) -> Result<()> {
        self.allowances
            .write()
            .unwrap()
            .insert(allowance.allowance_id.clone(), allowance.clone());
        Ok(())
    }

    fn get_allowance(&self, allowance_id: &str) -> Result<Option<Allowance>> {
        Ok(self.allowances.read().unwrap().get(allowance_id).cloned())
    }

    fn list_allowances(&self, account_id: &str) -> Result<Vec<Allowance>> {
        Ok(self
            .allowances
            .read()
            .unwrap()
            .values()
            .filter(|a| a.account_id == account_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Address, Currency, Network};
    use chrono::Local;

    #[test]
    fn memory_repo_wallet_crud() {
        let mut repo = InMemoryWalletRepository::new();
        let wallet = Wallet::new("W1".into(), "测试".into(), Network::Polygon);
        repo.save_wallet(&wallet).unwrap();

        let loaded = repo.get_wallet("W1").unwrap().unwrap();
        assert_eq!(loaded.name, "测试");

        let list = repo.list_wallets().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn memory_repo_account_crud() {
        let mut repo = InMemoryWalletRepository::new();
        let account = Account::default_main(Local::now());
        repo.save_account(&account).unwrap();

        let loaded = repo.get_account("ACC-MAIN-001").unwrap().unwrap();
        assert_eq!(loaded.name, "主账户");

        assert_eq!(repo.list_accounts().unwrap().len(), 1);
    }

    #[test]
    fn memory_repo_balance_crud() {
        let mut repo = InMemoryWalletRepository::new();
        let balance = Balance::new("ACC-1", Currency::USDC, 1000.0);
        repo.save_balance(&balance).unwrap();

        let loaded = repo.get_balance("ACC-1", "USDC").unwrap().unwrap();
        assert_eq!(loaded.total, 1000.0);

        assert_eq!(repo.list_balances("ACC-1").unwrap().len(), 1);
    }

    #[test]
    fn memory_repo_allowance_crud() {
        let mut repo = InMemoryWalletRepository::new();
        let allowance = Allowance::new(
            "ALW-1".into(),
            "ACC-1".into(),
            Address::new("0xSpender"),
            500.0,
        );
        repo.save_allowance(&allowance).unwrap();

        let loaded = repo.get_allowance("ALW-1").unwrap().unwrap();
        assert_eq!(loaded.amount, 500.0);

        assert_eq!(repo.list_allowances("ACC-1").unwrap().len(), 1);
    }

    #[test]
    fn memory_repo_health() {
        let repo = InMemoryWalletRepository::new();
        let h = repo.health();
        assert!(h.ok);
        assert_eq!(h.wallet_count, 0);
    }

    #[test]
    fn memory_repo_find_account_by_address() {
        let mut repo = InMemoryWalletRepository::new();
        let mut account = Account::default_main(Local::now());
        account.address = Address::new("0xTargetAddress");
        account.chain_id = 137;
        repo.save_account(&account).unwrap();

        let found = repo
            .find_account_by_address("0xTargetAddress", 137)
            .unwrap();
        assert!(found.is_some());
    }
}
