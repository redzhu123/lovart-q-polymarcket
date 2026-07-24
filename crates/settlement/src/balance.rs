//! Balance Settlement（余额结算 — P2-06 第五节）。
//!
//! 所有成交驱动的余额变化均由本模块统一处理。
//!
//! 职责：
//! - 可用资金管理
//! - 冻结资金管理
//! - 预留资金管理（接口预留）
//! - 账户权益重算
//! - 钱包余额同步
//! - 净资产价值（NAV）计算
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use std::collections::HashMap;

use crate::types::{BalanceState, FeeBreakdown, TradeFillEvent};

// ============================================================================
// BalanceManager — Settlement 余额管理器
// ============================================================================

/// Settlement 余额管理器。
///
/// 维护账户余额状态。所有余额变化必须通过本管理器。
#[derive(Debug, Clone)]
pub struct BalanceManager {
    /// 账户余额映射。
    balances: HashMap<String, BalanceState>,
}

impl Default for BalanceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BalanceManager {
    /// 创建新余额管理器。
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    /// 初始化账户余额。
    pub fn init_account(&mut self, account_id: String, initial_balance: f64, now: DateTime<Local>) {
        let bal = BalanceState::new(account_id.clone(), initial_balance, now);
        tracing::info!(
            account_id = %account_id,
            initial_balance = %initial_balance,
            "账户余额初始化"
        );
        self.balances.insert(account_id, bal);
    }

    /// 获取账户余额。
    pub fn get(&self, account_id: &str) -> Option<&BalanceState> {
        self.balances.get(account_id)
    }

    /// 获取可变引用。
    fn get_mut(&mut self, account_id: &str) -> Option<&mut BalanceState> {
        self.balances.get_mut(account_id)
    }

    /// 确保账户存在（自动创建）。
    fn ensure_account(&mut self, account_id: &str, default_balance: f64, now: DateTime<Local>) {
        if !self.balances.contains_key(account_id) {
            self.init_account(account_id.to_string(), default_balance, now);
        }
    }

    /// 开仓冻结资金。
    ///
    /// 成交前冻结所需资金（cost + estimated_fee）。
    ///
    /// # 返回
    ///
    /// - `before_available`：冻结前可用余额。
    /// - `after_available`：冻结后可用余额。
    /// - 如果余额不足返回 `None`。
    pub fn freeze_for_open(
        &mut self,
        event: &TradeFillEvent,
        cost: f64,
        _fee: &FeeBreakdown,
        now: DateTime<Local>,
    ) -> Option<(f64, f64)> {
        self.ensure_account(&event.account_id, 0.0, now);
        let bal = self.get_mut(&event.account_id).unwrap();
        let before = bal.available;

        let total_required = cost + _fee.total_fee;

        if bal.available < total_required {
            tracing::warn!(
                account_id = %event.account_id,
                available = %bal.available,
                required = %total_required,
                trade_id = %event.trade_id,
                "冻结失败：余额不足"
            );
            return None;
        }

        bal.freeze(total_required);
        tracing::info!(
            account_id = %event.account_id,
            trade_id = %event.trade_id,
            cost = %cost,
            fee = %_fee.total_fee,
            before = %before,
            after = %bal.available,
            frozen = %bal.frozen,
            "开仓资金冻结完成"
        );
        Some((before, bal.available))
    }

    /// 成交扣款（从冻结中扣除成本 + 手续费）。
    ///
    /// # 返回
    ///
    /// - `before_balance`：扣款前余额（available + frozen）。
    /// - `after_balance`：扣款后余额。
    pub fn debit_fill(
        &mut self,
        event: &TradeFillEvent,
        cost: f64,
        fee: &FeeBreakdown,
        _now: DateTime<Local>,
    ) -> (f64, f64) {
        let bal = self
            .get_mut(&event.account_id)
            .expect("账户不存在：请先调用 freeze_for_open");
        let before = bal.available + bal.frozen;
        let total = cost + fee.total_fee;

        bal.debit(total);
        // 额外扣除手续费（debit 只扣了冻结部分，手续费可能从 available 中额外扣除）
        if fee.total_fee > 0.0 {
            bal.charge_fee(fee.total_fee);
        }
        let after = bal.available + bal.frozen;

        tracing::info!(
            account_id = %event.account_id,
            trade_id = %event.trade_id,
            cost = %cost,
            fee = %fee.total_fee,
            before = %before,
            after = %after,
            delta = %(after - before),
            "成交扣款完成"
        );
        (before, after)
    }

    /// 平仓入账。
    ///
    /// # 返回
    ///
    /// - `before_balance`：入账前余额。
    /// - `after_balance`：入账后余额。
    pub fn credit_close(
        &mut self,
        event: &TradeFillEvent,
        proceeds: f64,
        fee: &FeeBreakdown,
        now: DateTime<Local>,
    ) -> (f64, f64) {
        self.ensure_account(&event.account_id, 0.0, now);
        let bal = self.get_mut(&event.account_id).unwrap();
        let before = bal.available + bal.frozen;
        let net = proceeds - fee.total_fee;

        if net > 0.0 {
            bal.credit(net);
        } else if net < 0.0 {
            // 平仓亏损 + 手续费
            bal.charge_fee(net.abs());
        }

        let after = bal.available + bal.frozen;
        tracing::info!(
            account_id = %event.account_id,
            trade_id = %event.trade_id,
            proceeds = %proceeds,
            fee = %fee.total_fee,
            net = %net,
            before = %before,
            after = %after,
            "平仓入账完成"
        );
        (before, after)
    }

    /// 释放冻结资金（订单取消 / 拒绝时）。
    ///
    /// # 返回
    ///
    /// 释放金额。
    pub fn unfreeze(&mut self, account_id: &str, amount: f64) -> Option<f64> {
        let bal = self.get_mut(account_id)?;
        let before = bal.frozen;
        bal.unfreeze(amount);
        let released = before - bal.frozen;
        tracing::info!(
            account_id = %account_id,
            amount = %amount,
            released = %released,
            "冻结资金已释放"
        );
        Some(released)
    }

    /// 同步交易所钱包余额。
    pub fn sync_wallet(&mut self, account_id: &str, balance: f64, now: DateTime<Local>) {
        self.ensure_account(account_id, balance, now);
        if let Some(bal) = self.get_mut(account_id) {
            bal.sync_wallet(balance);
            bal.updated_at = now;
            tracing::info!(
                account_id = %account_id,
                wallet_balance = %balance,
                local_balance = %(bal.available + bal.frozen),
                "钱包余额同步完成"
            );
        }
    }

    /// 获取账户权益。
    pub fn equity(&self, account_id: &str) -> Option<f64> {
        self.get(account_id).map(|b| b.equity)
    }

    /// 获取可用余额。
    pub fn available(&self, account_id: &str) -> Option<f64> {
        self.get(account_id).map(|b| b.available)
    }

    /// 获取冻结余额。
    pub fn frozen(&self, account_id: &str) -> Option<f64> {
        self.get(account_id).map(|b| b.frozen)
    }

    /// 获取净资产价值（NAV）。
    pub fn nav(&self, account_id: &str) -> Option<f64> {
        self.get(account_id).map(|b| b.nav)
    }

    /// 所有账户的合计权益。
    pub fn total_equity(&self) -> f64 {
        self.balances.values().map(|b| b.equity).sum::<f64>()
    }

    /// 所有账户的合计可用余额。
    pub fn total_available(&self) -> f64 {
        self.balances.values().map(|b| b.available).sum::<f64>()
    }

    /// 账户列表。
    pub fn account_ids(&self) -> Vec<&str> {
        self.balances.keys().map(|s| s.as_str()).collect()
    }

    /// 账户数量。
    pub fn account_count(&self) -> usize {
        self.balances.len()
    }

    /// 打印全部余额（中文 CLI 输出）。
    pub fn print_zh(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  Settlement 账户余额");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!(
            "  账户数: {}  |  合计可用: {:.2} USDC  |  合计权益: {:.2} USDC",
            self.account_count(),
            self.total_available(),
            self.total_equity()
        );
        println!();
        println!(
            "  {:<20} {:<12} {:<12} {:<12} {:<12} {:<12}",
            "账户", "可用", "冻结", "预留", "权益", "NAV"
        );
        println!("  {}", "─".repeat(80));
        for bal in self.balances.values() {
            println!(
                "  {:<20} {:<12.2} {:<12.2} {:<12.2} {:<12.2} {:<12.2}",
                bal.account_id, bal.available, bal.frozen, bal.reserved, bal.equity, bal.nav,
            );
        }
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Direction, FeeBreakdown};
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn sample_fill() -> TradeFillEvent {
        TradeFillEvent {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        }
    }

    #[test]
    fn init_and_get_balance() {
        let mut mgr = BalanceManager::new();
        let now = Local::now();
        mgr.init_account("ACCT-001".into(), 10000.0, now);
        let bal = mgr.get("ACCT-001").unwrap();
        assert!(approx(bal.available, 10000.0));
        assert!(approx(bal.equity, 10000.0));
    }

    #[test]
    fn freeze_and_debit_flow() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);

        let fill = sample_fill();
        let fee = FeeBreakdown::zero();
        let freeze_result = mgr.freeze_for_open(&fill, 50.0, &fee, now);
        assert!(freeze_result.is_some());
        let (before, after) = freeze_result.unwrap();
        assert!(approx(before, 10000.0));
        assert!(approx(after, 9950.0));

        let (db_before, db_after) = mgr.debit_fill(&fill, 50.0, &fee, now);
        assert!(approx(db_before, 10000.0));
        assert!(approx(db_after, 9950.0));

        assert!(approx(mgr.available("ACCT-MAIN").unwrap(), 9950.0));
    }

    #[test]
    fn freeze_rejected_when_insufficient() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 100.0, now);

        let fill = sample_fill();
        let fee = FeeBreakdown::zero();
        let result = mgr.freeze_for_open(&fill, 500.0, &fee, now);
        assert!(result.is_none());
    }

    #[test]
    fn credit_close_adds_funds() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);

        let fill = sample_fill();
        let fee = FeeBreakdown::zero();
        let (before, after) = mgr.credit_close(&fill, 60.0, &fee, now);
        assert!(approx(before, 10000.0));
        assert!(approx(after, 10060.0));
        assert!(approx(mgr.available("ACCT-MAIN").unwrap(), 10060.0));
    }

    #[test]
    fn credit_close_with_fee() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);

        let fill = sample_fill();
        let mut fee = FeeBreakdown::zero();
        fee.total_fee = 2.0;
        let (before, after) = mgr.credit_close(&fill, 60.0, &fee, now);
        assert!(approx(after - before, 58.0)); // 60 - 2
    }

    #[test]
    fn unfreeze_releases_funds() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);

        let fill = sample_fill();
        let fee = FeeBreakdown::zero();
        mgr.freeze_for_open(&fill, 50.0, &fee, now).unwrap();

        let released = mgr.unfreeze("ACCT-MAIN", 50.0).unwrap();
        assert!(approx(released, 50.0));
        assert!(approx(mgr.available("ACCT-MAIN").unwrap(), 10000.0));
        assert!(approx(mgr.frozen("ACCT-MAIN").unwrap(), 0.0));
    }

    #[test]
    fn ensure_account_auto_creates() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        let fill = sample_fill();
        let fee = FeeBreakdown::zero();
        // 不显式 init，freeze 应该自动创建
        let result = mgr.freeze_for_open(&fill, 50.0, &fee, now);
        // 自动创建的账户余额为 0，所以冻结会失败
        assert!(result.is_none());
        // 但账户应该存在了
        assert!(mgr.get("ACCT-MAIN").is_some());
    }

    #[test]
    fn sync_wallet_updates_balance() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);
        mgr.sync_wallet("ACCT-MAIN", 10050.0, now);
        assert!(approx(
            mgr.get("ACCT-MAIN").unwrap().wallet_balance,
            10050.0
        ));
    }

    #[test]
    fn totals_aggregate_correctly() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-001".into(), 5000.0, now);
        mgr.init_account("ACCT-002".into(), 3000.0, now);

        assert!(approx(mgr.total_equity(), 8000.0));
        assert!(approx(mgr.total_available(), 8000.0));
        assert_eq!(mgr.account_count(), 2);
    }

    #[test]
    fn print_zh_does_not_panic() {
        let now = Local::now();
        let mut mgr = BalanceManager::new();
        mgr.init_account("ACCT-MAIN".into(), 10000.0, now);
        mgr.print_zh();
    }
}
