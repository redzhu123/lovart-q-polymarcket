//! Allowance Manager（P2-06 第四节）。
//!
//! 授权额度管理：
//! - 创建/撤销授权
//! - 消耗额度追踪
//! - 过期检测

use chrono::Local;

use crate::domain::{Address, Allowance};

// ============================================================================
// AllowanceManager
// ============================================================================

/// 授权管理器（P2-06 第四节）。
///
/// 管理账户的授权额度。
pub struct AllowanceManager {
    allowances: Vec<Allowance>,
    next_id_counter: u64,
}

impl AllowanceManager {
    /// 创建授权管理器。
    pub fn new() -> Self {
        Self {
            allowances: Vec::new(),
            next_id_counter: 1,
        }
    }

    /// 创建授权。
    pub fn approve(&mut self, account_id: &str, spender: Address, amount: f64) -> &Allowance {
        let allowance_id = format!("ALW-{:04}", self.next_id_counter);
        self.next_id_counter += 1;

        let allowance = Allowance::new(allowance_id, account_id.to_string(), spender, amount);

        tracing::info!(
            allowance_id = %allowance.allowance_id,
            account_id = %account_id,
            amount = amount,
            "创建授权"
        );

        self.allowances.push(allowance);
        self.allowances.last().unwrap()
    }

    /// 查找授权（按 ID）。
    pub fn find_by_id(&self, allowance_id: &str) -> Option<&Allowance> {
        self.allowances
            .iter()
            .find(|a| a.allowance_id == allowance_id)
    }

    /// 获取账户的所有授权。
    pub fn list_for_account(&self, account_id: &str) -> Vec<&Allowance> {
        self.allowances
            .iter()
            .filter(|a| a.account_id == account_id)
            .collect()
    }

    /// 获取所有授权。
    pub fn all(&self) -> &[Allowance] {
        &self.allowances
    }

    /// 获取可用授权（未撤销、未过期、有剩余额度）。
    pub fn usable(&self, account_id: &str) -> Vec<&Allowance> {
        self.allowances
            .iter()
            .filter(|a| a.account_id == account_id && a.is_usable())
            .collect()
    }

    /// 授权数量。
    pub fn count(&self) -> usize {
        self.allowances.len()
    }

    /// 消耗额度。
    pub fn consume(&mut self, allowance_id: &str, amount: f64) -> anyhow::Result<bool> {
        let allowance = self
            .allowances
            .iter_mut()
            .find(|a| a.allowance_id == allowance_id)
            .ok_or_else(|| anyhow::anyhow!("授权不存在: {}", allowance_id))?;

        if !allowance.is_usable() {
            return Ok(false);
        }

        Ok(allowance.consume(amount))
    }

    /// 撤销授权。
    pub fn revoke(&mut self, allowance_id: &str) -> anyhow::Result<()> {
        let allowance = self
            .allowances
            .iter_mut()
            .find(|a| a.allowance_id == allowance_id)
            .ok_or_else(|| anyhow::anyhow!("授权不存在: {}", allowance_id))?;

        allowance.revoke();
        tracing::info!(allowance_id = %allowance_id, "授权已撤销");
        Ok(())
    }

    /// 清理过期授权。
    pub fn purge_expired(&mut self) -> usize {
        let before = self.allowances.len();
        let now = Local::now();
        self.allowances.retain(|a| {
            if let Some(expiry) = a.expires_at {
                now < expiry
            } else {
                true
            }
        });
        let removed = before - self.allowances.len();
        if removed > 0 {
            tracing::info!(removed = removed, "清理过期授权");
        }
        removed
    }

    /// 健康检查。
    pub fn health(&self) -> AllowanceManagerHealth {
        let total = self.count();
        let usable = self.allowances.iter().filter(|a| a.is_usable()).count();
        let revoked = self.allowances.iter().filter(|a| a.revoked).count();

        AllowanceManagerHealth {
            total,
            usable,
            revoked,
            expired: total.saturating_sub(usable + revoked),
        }
    }

    /// 打印所有授权（中文）。
    pub fn print_zh(&self) {
        println!();
        println!("══════════════════════════════════════");
        println!("  授权列表");
        println!("══════════════════════════════════════");
        println!();

        if self.allowances.is_empty() {
            println!("  （无授权记录）");
        } else {
            for a in &self.allowances {
                println!("  {}", a.summary_zh());
            }
        }
        println!();
    }
}

impl Default for AllowanceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AllowanceManagerHealth
// ============================================================================

/// 授权管理器健康状态。
#[derive(Debug, Clone)]
pub struct AllowanceManagerHealth {
    pub total: usize,
    pub usable: usize,
    pub revoked: usize,
    pub expired: usize,
}

impl AllowanceManagerHealth {
    pub fn summary_zh(&self) -> String {
        format!(
            "授权总数: {} | 可用: {} | 已撤销: {} | 已过期: {}",
            self.total, self.usable, self.revoked, self.expired,
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
    fn allowance_manager_approve() {
        let mut mgr = AllowanceManager::new();
        let a = mgr.approve("ACC-1", Address::new("0xSpender"), 1000.0);
        assert_eq!(a.amount, 1000.0);
        assert!(a.is_usable());
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn allowance_manager_consume() {
        let mut mgr = AllowanceManager::new();
        mgr.approve("ACC-1", Address::new("0xSpender"), 1000.0);

        assert!(mgr.consume("ALW-0001", 300.0).unwrap());
        let a = mgr.find_by_id("ALW-0001").unwrap();
        assert_eq!(a.remaining(), 700.0);
    }

    #[test]
    fn allowance_manager_revoke() {
        let mut mgr = AllowanceManager::new();
        mgr.approve("ACC-1", Address::new("0xSpender"), 1000.0);

        mgr.revoke("ALW-0001").unwrap();
        let a = mgr.find_by_id("ALW-0001").unwrap();
        assert!(a.revoked);
        assert!(!a.is_usable());
    }

    #[test]
    fn allowance_manager_list_for_account() {
        let mut mgr = AllowanceManager::new();
        mgr.approve("ACC-1", Address::new("0xA"), 500.0);
        mgr.approve("ACC-1", Address::new("0xB"), 300.0);
        mgr.approve("ACC-2", Address::new("0xC"), 200.0);

        assert_eq!(mgr.list_for_account("ACC-1").len(), 2);
        assert_eq!(mgr.list_for_account("ACC-2").len(), 1);
    }

    #[test]
    fn allowance_manager_health() {
        let mut mgr = AllowanceManager::new();
        mgr.approve("ACC-1", Address::new("0xA"), 500.0);
        mgr.approve("ACC-1", Address::new("0xB"), 300.0);

        let h = mgr.health();
        assert_eq!(h.total, 2);
        assert_eq!(h.usable, 2);
    }
}
