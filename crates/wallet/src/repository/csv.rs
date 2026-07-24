//! CSV Wallet Repository（P2-06 第六节）。

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use super::{RepositoryHealth, RepositoryType, WalletRepository};
use crate::domain::{Account, Allowance, Balance, Wallet};

/// CSV 钱包仓库。
pub struct CsvWalletRepository {
    #[allow(dead_code)]
    base_path: PathBuf,
    wallets_path: PathBuf,
    accounts_path: PathBuf,
    balances_path: PathBuf,
    allowances_path: PathBuf,
}

impl CsvWalletRepository {
    pub fn new(base_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_path)?;
        let repo = Self {
            wallets_path: base_path.join("wallets.csv"),
            accounts_path: base_path.join("accounts.csv"),
            balances_path: base_path.join("balances.csv"),
            allowances_path: base_path.join("allowances.csv"),
            base_path,
        };
        repo.ensure_files()?;
        Ok(repo)
    }

    fn ensure_files(&self) -> Result<()> {
        if !self.wallets_path.exists() {
            fs::write(&self.wallets_path, "wallet_id,name,network,created_at\n")?;
        }
        if !self.accounts_path.exists() {
            fs::write(
                &self.accounts_path,
                "account_id,name,address,wallet_id,chain_id,network,currency,active\n",
            )?;
        }
        if !self.balances_path.exists() {
            fs::write(
                &self.balances_path,
                "account_id,available,locked,total,currency\n",
            )?;
        }
        if !self.allowances_path.exists() {
            fs::write(
                &self.allowances_path,
                "allowance_id,account_id,spender,amount,used,revoked\n",
            )?;
        }
        Ok(())
    }

    fn read_wallets_from_csv(&self) -> Result<Vec<Wallet>> {
        if !self.wallets_path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&self.wallets_path)?;
        let mut wallets = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if record.len() < 4 {
                continue;
            }
            let wallet = Wallet::new(
                record[0].to_string(),
                record[1].to_string(),
                crate::domain::Network::Polygon,
            );
            wallets.push(wallet);
        }
        Ok(wallets)
    }

    fn read_accounts_from_csv(&self) -> Result<Vec<Account>> {
        if !self.accounts_path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&self.accounts_path)?;
        let mut accounts = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if record.len() < 8 {
                continue;
            }
            let chain_id: u64 = record[4].parse().unwrap_or(137);
            let account = Account::new(
                record[0].to_string(),
                record[1].to_string(),
                crate::domain::Address::new(record[2].to_string()),
                record[3].to_string(),
                crate::domain::Network::Custom(chain_id),
                crate::domain::Currency::USDC,
            );
            accounts.push(account);
        }
        Ok(accounts)
    }
}

impl WalletRepository for CsvWalletRepository {
    fn name(&self) -> &str {
        "CsvWalletRepository"
    }

    fn repository_type(&self) -> RepositoryType {
        RepositoryType::Csv
    }

    fn health(&self) -> RepositoryHealth {
        let mut h = RepositoryHealth::ok("CsvWalletRepository", RepositoryType::Csv);
        h.wallet_count = self.list_wallets().map(|w| w.len()).unwrap_or(0);
        h.account_count = self.list_accounts().map(|a| a.len()).unwrap_or(0);
        h
    }

    fn save_wallet(&mut self, wallet: &Wallet) -> Result<()> {
        let txt = format!(
            "wallet_id,name,network,created_at\n{},{},{},{}\n",
            wallet.wallet_id,
            wallet.name,
            wallet.network.short_name(),
            wallet.created_at.to_rfc3339(),
        );
        fs::write(&self.wallets_path, txt)?;
        Ok(())
    }

    fn get_wallet(&self, wallet_id: &str) -> Result<Option<Wallet>> {
        let wallets = self.read_wallets_from_csv()?;
        Ok(wallets.into_iter().find(|w| w.wallet_id == wallet_id))
    }

    fn list_wallets(&self) -> Result<Vec<Wallet>> {
        self.read_wallets_from_csv()
    }

    fn save_account(&mut self, account: &Account) -> Result<()> {
        let txt = format!(
            "account_id,name,address,wallet_id,chain_id,network,currency,active\n{},{},{},{},{},{},{},{}\n",
            account.account_id,
            account.name,
            account.address.reveal(),
            account.wallet_id,
            account.chain_id,
            account.network.short_name(),
            account.currency.symbol(),
            account.active,
        );
        fs::write(&self.accounts_path, txt)?;
        Ok(())
    }

    fn get_account(&self, account_id: &str) -> Result<Option<Account>> {
        let accounts = self.read_accounts_from_csv()?;
        Ok(accounts.into_iter().find(|a| a.account_id == account_id))
    }

    fn list_accounts(&self) -> Result<Vec<Account>> {
        self.read_accounts_from_csv()
    }

    fn find_account_by_address(&self, address: &str, chain_id: u64) -> Result<Option<Account>> {
        let accounts = self.read_accounts_from_csv()?;
        Ok(accounts
            .into_iter()
            .find(|a| a.address.reveal() == address && a.chain_id == chain_id))
    }

    fn save_balance(&mut self, balance: &Balance) -> Result<()> {
        let txt = format!(
            "account_id,available,locked,total,currency\n{},{},{},{},{}\n",
            balance.account_id,
            balance.available,
            balance.locked,
            balance.total,
            balance.currency.symbol(),
        );
        fs::write(&self.balances_path, txt)?;
        Ok(())
    }

    fn get_balance(&self, account_id: &str, currency: &str) -> Result<Option<Balance>> {
        if !self.balances_path.exists() {
            return Ok(None);
        }
        let mut rdr = csv::Reader::from_path(&self.balances_path)?;
        for result in rdr.records() {
            let record = result?;
            if record.len() >= 5 {
                let aid = &record[0];
                let cur = &record[4];
                if aid == account_id && cur == currency {
                    let available: f64 = record[1].parse().unwrap_or(0.0);
                    let locked: f64 = record[2].parse().unwrap_or(0.0);
                    let total: f64 = record[3].parse().unwrap_or(0.0);
                    return Ok(Some(Balance {
                        account_id: account_id.to_string(),
                        available,
                        locked,
                        total,
                        currency: crate::domain::Currency::USDC,
                        unrealized_pnl: 0.0,
                        realized_pnl: 0.0,
                        updated_at: None,
                    }));
                }
            }
        }
        Ok(None)
    }

    fn list_balances(&self, account_id: &str) -> Result<Vec<Balance>> {
        if !self.balances_path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&self.balances_path)?;
        let mut balances = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if record.len() >= 5 {
                let aid = &record[0];
                if aid == account_id {
                    let total: f64 = record[3].parse().unwrap_or(0.0);
                    balances.push(Balance::new(
                        account_id,
                        crate::domain::Currency::USDC,
                        total,
                    ));
                }
            }
        }
        Ok(balances)
    }

    fn save_allowance(&mut self, allowance: &Allowance) -> Result<()> {
        let txt = format!(
            "allowance_id,account_id,spender,amount,used,revoked\n{},{},{},{},{},{}\n",
            allowance.allowance_id,
            allowance.account_id,
            allowance.spender.reveal(),
            allowance.amount,
            allowance.used,
            allowance.revoked,
        );
        fs::write(&self.allowances_path, txt)?;
        Ok(())
    }

    fn get_allowance(&self, allowance_id: &str) -> Result<Option<Allowance>> {
        if !self.allowances_path.exists() {
            return Ok(None);
        }
        let mut rdr = csv::Reader::from_path(&self.allowances_path)?;
        for result in rdr.records() {
            let record = result?;
            if record.len() >= 6 {
                let aid = &record[0];
                if aid == allowance_id {
                    let amount: f64 = record[3].parse().unwrap_or(0.0);
                    let used: f64 = record[4].parse().unwrap_or(0.0);
                    let revoked_str = &record[5];
                    let revoked = revoked_str == "true";
                    let mut a = Allowance::new(
                        allowance_id.to_string(),
                        record[1].to_string(),
                        crate::domain::Address::new(record[2].to_string()),
                        amount,
                    );
                    a.used = used;
                    if revoked {
                        a.revoke();
                    }
                    return Ok(Some(a));
                }
            }
        }
        Ok(None)
    }

    fn list_allowances(&self, account_id: &str) -> Result<Vec<Allowance>> {
        if !self.allowances_path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&self.allowances_path)?;
        let mut allowances = Vec::new();
        for result in rdr.records() {
            let record = result?;
            if record.len() >= 6 {
                let aid2 = &record[1]; // account_id is second field
                if aid2 == account_id {
                    let amount: f64 = record[3].parse().unwrap_or(0.0);
                    allowances.push(Allowance::new(
                        record[0].to_string(),
                        account_id.to_string(),
                        crate::domain::Address::new(record[2].to_string()),
                        amount,
                    ));
                }
            }
        }
        Ok(allowances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Currency, Network};
    use chrono::Local;

    fn temp_repo() -> CsvWalletRepository {
        let dir = std::env::temp_dir().join(format!("wallet-test-{}", rand::random::<u64>()));
        CsvWalletRepository::new(dir).unwrap()
    }

    #[test]
    fn csv_repo_wallet_save_and_load() {
        let mut repo = temp_repo();
        let wallet = Wallet::new("W-CSV".into(), "CSV测试".into(), Network::Polygon);
        repo.save_wallet(&wallet).unwrap();
        assert!(repo.get_wallet("W-CSV").unwrap().is_some());
    }

    #[test]
    fn csv_repo_account_save_and_load() {
        let mut repo = temp_repo();
        let account = Account::default_main(Local::now());
        repo.save_account(&account).unwrap();
        let loaded = repo.get_account("ACC-MAIN-001").unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn csv_repo_balance_save_and_load() {
        let mut repo = temp_repo();
        let balance = Balance::new("ACC-1", Currency::USDC, 500.0);
        repo.save_balance(&balance).unwrap();
        let loaded = repo.get_balance("ACC-1", "USDC").unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn csv_repo_health() {
        let repo = temp_repo();
        let h = repo.health();
        assert!(h.ok);
    }
}
