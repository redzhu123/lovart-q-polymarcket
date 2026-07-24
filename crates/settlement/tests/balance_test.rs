//! Balance Settlement 集成测试。

use chrono::Local;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn test_balance_init() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 10000.0, now);

    let bal = mgr.get("ACCT-001").unwrap();
    assert!(approx(bal.available, 10000.0));
    assert!(approx(bal.frozen, 0.0));
    assert!(approx(bal.equity, 10000.0));
    assert!(approx(bal.nav, 10000.0));
}

#[test]
fn test_balance_get_nonexistent() {
    let mgr = BalanceManager::new();
    assert!(mgr.get("NONEXISTENT").is_none());
}

#[test]
fn test_balance_account_ids() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 5000.0, now);
    mgr.init_account("ACCT-002".into(), 3000.0, now);

    let ids = mgr.account_ids();
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_balance_totals() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 5000.0, now);
    mgr.init_account("ACCT-002".into(), 3000.0, now);
    mgr.init_account("ACCT-003".into(), 2000.0, now);

    assert!(approx(mgr.total_equity(), 10000.0));
    assert!(approx(mgr.total_available(), 10000.0));
    assert_eq!(mgr.account_count(), 3);
}

#[test]
fn test_balance_available_query() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 10000.0, now);

    assert!(approx(mgr.available("ACCT-001").unwrap(), 10000.0));
    assert!(approx(mgr.frozen("ACCT-001").unwrap(), 0.0));
    assert!(approx(mgr.nav("ACCT-001").unwrap(), 10000.0));
    assert!(approx(mgr.equity("ACCT-001").unwrap(), 10000.0));
}

#[test]
fn test_sync_wallet() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 10000.0, now);
    mgr.sync_wallet("ACCT-001", 10050.0, now);

    let bal = mgr.get("ACCT-001").unwrap();
    assert!(approx(bal.wallet_balance, 10050.0));
}

#[test]
fn test_unfreeze_nonexistent() {
    let mut mgr = BalanceManager::new();
    assert!(mgr.unfreeze("NONEXISTENT", 100.0).is_none());
}

#[test]
fn test_print_zh() {
    let mut mgr = BalanceManager::new();
    let now = Local::now();
    mgr.init_account("ACCT-001".into(), 10000.0, now);
    mgr.print_zh(); // smoke test
}
