//! AccountManager — 账户管理器（P2-05 第三节 多账户支持）。

use crate::domain::{Account, Currency};
use chrono::{DateTime, Local};

/// 账户管理器：支持多账户。
pub struct AccountManager {
    accounts: Vec<Account>,
}

impl AccountManager {
    pub fn new(accounts: Vec<Account>) -> Self {
        tracing::info!(account_count = %accounts.len(), "账户管理器初始化");
        Self { accounts }
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    /// 创建新账户。
    pub fn create_account(
        &mut self,
        account_id: String,
        name: String,
        currency: Currency,
        initial_balance: f64,
        now: DateTime<Local>,
    ) -> &Account {
        let acct = Account::new(account_id, name, currency, initial_balance, now);
        tracing::info!(
            account_id = %acct.account_id,
            name = %acct.name,
            initial_balance = %initial_balance,
            "新账户创建"
        );
        self.accounts.push(acct);
        self.accounts.last().unwrap()
    }

    /// 按 ID 查找账户。
    pub fn find_by_id(&self, account_id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.account_id == account_id)
    }

    /// 按 ID 查找账户（可变）。
    pub fn find_by_id_mut(&mut self, account_id: &str) -> Option<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.account_id == account_id)
    }

    /// 添加持仓到账户。
    pub fn add_position_to_account(&mut self, account_id: &str, position_id: &str) -> bool {
        if let Some(acct) = self.find_by_id_mut(account_id) {
            if !acct.position_ids.contains(&position_id.to_string()) {
                acct.position_ids.push(position_id.to_string());
            }
            true
        } else {
            false
        }
    }

    /// 总可用资金（所有账户之和）。
    pub fn total_available(&self) -> f64 {
        self.accounts.iter().map(|a| a.balance.available).sum()
    }

    /// 中文打印账户列表。
    pub fn print_zh(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  账户列表");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        for acct in &self.accounts {
            println!("  账户: {} ({})", acct.name, acct.account_id);
            println!("    货币    : {}", acct.currency.as_str());
            println!("    总余额  : {:.2}", acct.balance.total);
            println!("    可用    : {:.2}", acct.balance.available);
            println!("    冻结    : {:.2}", acct.balance.frozen);
            println!("    持仓数  : {}", acct.position_ids.len());
            println!();
        }
        println!("  总可用资金: {:.2}", self.total_available());
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn create_and_find_account() {
        let now = Local::now();
        let mut mgr = AccountManager::new(Vec::new());
        mgr.create_account(
            "ACCT-001".into(),
            "测试".into(),
            Currency::usdc(),
            5000.0,
            now,
        );
        let acct = mgr.find_by_id("ACCT-001");
        assert!(acct.is_some());
        assert!(approx(acct.unwrap().balance.available, 5000.0));
    }

    #[test]
    fn add_position_to_account() {
        let now = Local::now();
        let mut mgr = AccountManager::new(vec![Account::default_main(now)]);
        assert!(mgr.add_position_to_account("ACCT-MAIN-001", "POS-001"));
        let acct = mgr.find_by_id("ACCT-MAIN-001").unwrap();
        assert!(acct.position_ids.contains(&"POS-001".to_string()));
    }

    #[test]
    fn total_available_sum() {
        let now = Local::now();
        let a1 = Account::new("A1".into(), "A1".into(), Currency::usdc(), 3000.0, now);
        let a2 = Account::new("A2".into(), "A2".into(), Currency::usdc(), 2000.0, now);
        let mgr = AccountManager::new(vec![a1, a2]);
        assert!(approx(mgr.total_available(), 5000.0));
    }
}
