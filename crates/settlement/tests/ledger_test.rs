//! Ledger（资金流水）集成测试。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn fill(trade_id: &str, order_id: &str, side: Side) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: trade_id.to_string(),
        order_id: order_id.to_string(),
        client_order_id: format!("CLI-{}", order_id),
        exchange_order_id: None,
        market_id: "mkt-btc".to_string(),
        account_id: "ACCT-MAIN-001".to_string(),
        direction: pm_execution::order::Direction::Yes,
        side,
        fill_price: 0.50,
        fill_quantity: 100.0,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

#[test]
fn test_ledger_append_only() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let f = fill("T-001", "O-001", Side::Buy);

    ledger.record_debit(&f, 50.0, 0.02, 10000.0, 9949.98, "测试1", now);
    assert_eq!(ledger.count(), 1);

    // 追加第二条
    ledger.record_debit(&f, 30.0, 0.01, 9949.98, 9919.97, "测试2", now);
    assert_eq!(ledger.count(), 2);

    // 第一条不变
    assert_eq!(ledger.entries()[0].trade_id, "T-001");
}

#[test]
fn test_ledger_debit_credit_directions() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let mut f = fill("T-001", "O-001", Side::Buy);

    let d = ledger.record_debit(&f, 50.0, 0.02, 10000.0, 9949.98, "扣款", now);
    assert_eq!(d.direction, LedgerDirection::Debit);
    assert!(d.amount < 0.0);

    f.trade_id = "T-002".into();
    let c = ledger.record_credit(&f, 60.0, 0.02, 9949.98, 10009.96, "入账", now);
    assert_eq!(c.direction, LedgerDirection::Credit);
    assert!(c.amount > 0.0);
}

#[test]
fn test_ledger_totals() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let mut f = fill("T-001", "O-001", Side::Buy);

    ledger.record_debit(&f, 100.0, 5.0, 10000.0, 9895.0, "买入", now);

    f.trade_id = "T-002".into();
    f.side = Side::Sell;
    ledger.record_credit(&f, 120.0, 5.0, 9895.0, 10010.0, "卖出", now);

    assert!(approx(ledger.total_credits(), 120.0));
    assert!(approx(ledger.total_debits(), 100.0));
    assert!(approx(ledger.total_fees(), 10.0));
    assert!(approx(ledger.net_flow(), 10.0)); // 120 - 100 - 10
}

#[test]
fn test_ledger_filtering() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let f = fill("T-001", "O-001", Side::Buy);

    // 同一个订单两笔成交
    let mut f1 = f.clone();
    f1.trade_id = "T-001".into();
    ledger.record_debit(&f1, 50.0, 0.0, 10000.0, 9950.0, "第一笔", now);

    let mut f2 = f.clone();
    f2.trade_id = "T-002".into();
    f2.order_id = "O-002".into();
    ledger.record_debit(&f2, 30.0, 0.0, 9950.0, 9920.0, "第二笔", now);

    assert_eq!(ledger.by_order("O-001").len(), 1);
    assert_eq!(ledger.by_order("O-002").len(), 1);
    assert_eq!(ledger.by_trade("T-001").len(), 1);
    assert_eq!(ledger.by_trade("T-002").len(), 1);
}

#[test]
fn test_csv_export() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let f = fill("T-001", "O-001", Side::Buy);

    ledger.record_debit(&f, 50.0, 0.02, 10000.0, 9949.98, "测试", now);

    let csv = ledger.to_csv();
    assert!(csv.contains("ledger_id"));
    assert!(csv.contains("LEDGER-"));
    assert!(csv.contains("出账"));
}

#[test]
fn test_recent_entries() {
    let mut ledger = Ledger::new();
    let now = Local::now();

    for i in 0..15 {
        let f = fill(&format!("T-{:03}", i), &format!("O-{:03}", i), Side::Buy);
        ledger.record_debit(&f, 10.0, 0.0, 10000.0, 9990.0, "test", now);
    }

    assert_eq!(ledger.recent(10).len(), 10);
    assert_eq!(ledger.recent(20).len(), 15);
}

#[test]
fn test_ledger_id_unique() {
    let mut ledger = Ledger::new();
    let now = Local::now();
    let f = fill("T-001", "O-001", Side::Buy);

    let id1 = ledger
        .record_debit(&f, 10.0, 0.0, 1000.0, 990.0, "a", now)
        .ledger_id
        .clone();
    let id2 = ledger
        .record_debit(&f, 10.0, 0.0, 990.0, 980.0, "b", now)
        .ledger_id
        .clone();

    assert_ne!(id1, id2);
    assert!(id1.starts_with("LEDGER-"));
}
