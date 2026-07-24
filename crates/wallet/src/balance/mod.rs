//! Balance Manager（P2-06 第四节）。
//!
//! 余额追踪与管理：
//! - 多账户余额管理
//! - 冻结/解冻/扣款/入金
//! - 余额快照

use anyhow::Result;
use std::collections::HashMap;

use crate::domain::{Balance, Currency};

// ============================================================================
// BalanceManager
// ============================================================================

/// 余额管理器（P2-06 第四节）。
///
/// 管理多个账户的余额。
pub struct BalanceManager {
    /// (账户ID, 货币) -> Balance
    balances: HashMap<(String, Currency), Balance>,
}

impl BalanceManager {
    /// 创建余额管理器。
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    /// 设置初始余额。
    pub fn set_balance(&mut self, balance: Balance) {
        let currency_clone = balance.currency.clone();
        let key = (balance.account_id.clone(), currency_clone);
        tracing::debug!(
            account_id = %balance.account_id,
            currency = %balance.currency,
            amount = balance.total,
            "设置余额"
        );
        self.balances.insert(key, balance);
    }

    /// 获取余额。
    pub fn get(&self, account_id: &str, currency: Currency) -> Option<&Balance> {
        self.balances.get(&(account_id.to_string(), currency))
    }

    /// 获取可变余额。
    pub fn get_mut(&mut self, account_id: &str, currency: Currency) -> Option<&mut Balance> {
        self.balances.get_mut(&(account_id.to_string(), currency))
    }

    /// 获取账户所有余额。
    pub fn list_for_account(&self, account_id: &str) -> Vec<&Balance> {
        self.balances
            .iter()
            .filter(|((aid, _), _)| aid == account_id)
            .map(|(_, b)| b)
            .collect()
    }

    /// 全部余额。
    pub fn all(&self) -> Vec<&Balance> {
        self.balances.values().collect()
    }

    /// 余额条目数。
    pub fn count(&self) -> usize {
        self.balances.len()
    }

    /// 冻结资金。
    pub fn freeze(&mut self, account_id: &str, currency: Currency, amount: f64) -> Result<bool> {
        let balance = self
            .get_mut(account_id, currency.clone())
            .ok_or_else(|| anyhow::anyhow!("余额记录不存在: {}/{}", account_id, currency))?;
        Ok(balance.freeze(amount))
    }

    /// 解冻资金。
    pub fn unfreeze(&mut self, account_id: &str, currency: Currency, amount: f64) -> Result<bool> {
        let balance = self
            .get_mut(account_id, currency.clone())
            .ok_or_else(|| anyhow::anyhow!("余额记录不存在: {}/{}", account_id, currency))?;
        Ok(balance.unfreeze(amount))
    }

    /// 扣款（从锁定中扣除）。
    pub fn debit(&mut self, account_id: &str, currency: Currency, amount: f64) -> Result<bool> {
        let balance = self
            .get_mut(account_id, currency.clone())
            .ok_or_else(|| anyhow::anyhow!("余额记录不存在: {}/{}", account_id, currency))?;
        Ok(balance.debit(amount))
    }

    /// 入金。
    pub fn credit(&mut self, account_id: &str, currency: Currency, amount: f64) -> Result<()> {
        let balance = self
            .get_mut(account_id, currency.clone())
            .ok_or_else(|| anyhow::anyhow!("余额记录不存在: {}/{}", account_id, currency))?;
        balance.credit(amount);
        Ok(())
    }

    /// 获取总余额（指定货币）。
    pub fn total_by_currency(&self, currency: Currency) -> f64 {
        self.balances
            .values()
            .filter(|b| b.currency == currency)
            .map(|b| b.total)
            .sum()
    }

    /// 获取总可用余额（指定货币）。
    pub fn total_available_by_currency(&self, currency: Currency) -> f64 {
        self.balances
            .values()
            .filter(|b| b.currency == currency)
            .map(|b| b.available)
            .sum()
    }

    /// 健康检查。
    pub fn health(&self) -> BalanceManagerHealth {
        let total_available: f64 = self.balances.values().map(|b| b.available).sum();
        let total_locked: f64 = self.balances.values().map(|b| b.locked).sum();
        let currencies: std::collections::HashSet<Currency> =
            self.balances.values().map(|b| b.currency.clone()).collect();

        BalanceManagerHealth {
            entry_count: self.count(),
            total_available,
            total_locked,
            currency_count: currencies.len(),
        }
    }

    /// 打印所有余额（中文）。
    pub fn print_zh(&self) {
        println!();
        println!("══════════════════════════════════════");
        println!("  余额列表");
        println!("══════════════════════════════════════");
        println!();

        if self.balances.is_empty() {
            println!("  （无余额记录）");
        } else {
            for ((_aid, _), b) in &self.balances {
                println!("  {}", b.summary_zh());
            }
        }
        println!();
    }
}

impl Default for BalanceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BalanceManagerHealth
// ============================================================================

/// 余额管理器健康状态。
#[derive(Debug, Clone)]
pub struct BalanceManagerHealth {
    pub entry_count: usize,
    pub total_available: f64,
    pub total_locked: f64,
    pub currency_count: usize,
}

impl BalanceManagerHealth {
    pub fn summary_zh(&self) -> String {
        format!(
            "余额条目: {} | 可用总计: {:.2} | 锁定总计: {:.2} | 货币种类: {}",
            self.entry_count, self.total_available, self.total_locked, self.currency_count,
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
    fn balance_manager_set_and_get() {
        let mut mgr = BalanceManager::new();
        mgr.set_balance(Balance::new("ACC-1", Currency::USDC, 1000.0));
        assert!(mgr.get("ACC-1", Currency::USDC).is_some());
        assert_eq!(mgr.get("ACC-1", Currency::USDC).unwrap().total, 1000.0);
    }

    #[test]
    fn balance_manager_freeze_debit_cycle() {
        let mut mgr = BalanceManager::new();
        mgr.set_balance(Balance::new("ACC-1", Currency::USDC, 1000.0));

        assert!(mgr.freeze("ACC-1", Currency::USDC, 300.0).unwrap());
        assert!(mgr.debit("ACC-1", Currency::USDC, 300.0).unwrap());

        let b = mgr.get("ACC-1", Currency::USDC).unwrap();
        assert_eq!(b.total, 700.0);
        assert_eq!(b.available, 700.0);
        assert_eq!(b.locked, 0.0);
    }

    #[test]
    fn balance_manager_credit() {
        let mut mgr = BalanceManager::new();
        mgr.set_balance(Balance::new("ACC-1", Currency::USDC, 1000.0));
        mgr.credit("ACC-1", Currency::USDC, 500.0).unwrap();

        let b = mgr.get("ACC-1", Currency::USDC).unwrap();
        assert_eq!(b.total, 1500.0);
    }

    #[test]
    fn balance_manager_total_by_currency() {
        let mut mgr = BalanceManager::new();
        mgr.set_balance(Balance::new("ACC-1", Currency::USDC, 1000.0));
        mgr.set_balance(Balance::new("ACC-2", Currency::USDC, 500.0));
        mgr.set_balance(Balance::new("ACC-1", Currency::ETH, 10.0));

        assert_eq!(mgr.total_by_currency(Currency::USDC), 1500.0);
        assert_eq!(mgr.total_by_currency(Currency::ETH), 10.0);
    }

    #[test]
    fn balance_manager_health() {
        let mut mgr = BalanceManager::new();
        mgr.set_balance(Balance::new("ACC-1", Currency::USDC, 1000.0));
        mgr.set_balance(Balance::new("ACC-2", Currency::ETH, 5.0));

        let h = mgr.health();
        assert_eq!(h.entry_count, 2);
        assert_eq!(h.currency_count, 2);
    }
}
