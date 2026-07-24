//! Position Settlement 集成测试。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn buy_fill(
    trade_id: &str,
    order_id: &str,
    market_id: &str,
    direction: pm_execution::order::Direction,
    price: f64,
    qty: f64,
) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: trade_id.to_string(),
        order_id: order_id.to_string(),
        client_order_id: format!("CLI-{}", order_id),
        exchange_order_id: None,
        market_id: market_id.to_string(),
        account_id: "ACCT-MAIN-001".to_string(),
        direction,
        side: Side::Buy,
        fill_price: price,
        fill_quantity: qty,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

fn sell_fill(
    trade_id: &str,
    order_id: &str,
    market_id: &str,
    direction: pm_execution::order::Direction,
    price: f64,
    qty: f64,
) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: trade_id.to_string(),
        order_id: order_id.to_string(),
        client_order_id: format!("CLI-{}", order_id),
        exchange_order_id: None,
        market_id: market_id.to_string(),
        account_id: "ACCT-MAIN-001".to_string(),
        direction,
        side: Side::Sell,
        fill_price: price,
        fill_quantity: qty,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

#[test]
fn test_open_position_on_buy() {
    let mut mgr = PositionManager::new();
    let now = Local::now();
    let fill = buy_fill(
        "T-001",
        "O-001",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        100.0,
    );
    let (summary, realized, _) = mgr.apply_fill(&fill, now);

    assert!(summary.contains("新开仓"));
    assert!(approx(realized, 0.0));
    assert_eq!(mgr.open_count(), 1);
}

#[test]
fn test_add_to_existing_position() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.apply_fill(
        &buy_fill(
            "T-002",
            "O-002",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.60,
            100.0,
        ),
        now,
    );

    assert_eq!(mgr.open_count(), 1);
    let pos = mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.quantity, 200.0));
    assert!(approx(pos.average_price, 0.55));
}

#[test]
fn test_close_position_on_sell() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    let (summary, realized, _) = mgr.apply_fill(
        &sell_fill(
            "T-002",
            "O-002",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.60,
            100.0,
        ),
        now,
    );

    assert!(summary.contains("完全平仓"));
    assert!(approx(realized, 10.0));
    assert_eq!(mgr.open_count(), 0);
    assert_eq!(mgr.closed_count(), 1);
}

#[test]
fn test_partial_close() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            200.0,
        ),
        now,
    );
    let (summary, realized, _) = mgr.apply_fill(
        &sell_fill(
            "T-002",
            "O-002",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.55,
            80.0,
        ),
        now,
    );

    assert!(summary.contains("部分平仓"));
    assert!(approx(realized, 4.0));
    assert_eq!(mgr.open_count(), 1);
}

#[test]
fn test_sell_without_position() {
    let mut mgr = PositionManager::new();
    let now = Local::now();
    let (summary, realized, _) = mgr.apply_fill(
        &sell_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.60,
            100.0,
        ),
        now,
    );

    assert!(summary.contains("失败"));
    assert!(approx(realized, 0.0));
}

#[test]
fn test_different_directions_separate() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.apply_fill(
        &buy_fill(
            "T-002",
            "O-002",
            "mkt-btc",
            pm_execution::order::Direction::No,
            0.40,
            100.0,
        ),
        now,
    );

    assert_eq!(mgr.open_count(), 2);
}

#[test]
fn test_mark_position() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.mark_position("mkt-btc", pm_execution::order::Direction::Yes, 0.65, now);

    let pos = mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.mark_price, 0.65));
    assert!(approx(pos.unrealized_pnl, 15.0));
}

#[test]
fn test_pnl_totals() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.mark_position("mkt-btc", pm_execution::order::Direction::Yes, 0.55, now);

    assert!(approx(mgr.total_unrealized_pnl(), 5.0));
    assert!(approx(mgr.total_realized_pnl(), 0.0));
}

#[test]
fn test_realized_pnl_after_close() {
    let mut mgr = PositionManager::new();
    let now = Local::now();

    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.apply_fill(
        &sell_fill(
            "T-002",
            "O-002",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.60,
            100.0,
        ),
        now,
    );

    assert!(approx(mgr.total_realized_pnl(), 10.0));
    assert!(approx(mgr.total_unrealized_pnl(), 0.0));
}

#[test]
fn test_is_opening_closing() {
    assert!(PositionManager::is_opening(Side::Buy));
    assert!(!PositionManager::is_opening(Side::Sell));
    assert!(!PositionManager::is_closing(Side::Buy));
    assert!(PositionManager::is_closing(Side::Sell));
}

#[test]
fn test_print_zh() {
    let mut mgr = PositionManager::new();
    let now = Local::now();
    mgr.apply_fill(
        &buy_fill(
            "T-001",
            "O-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            0.50,
            100.0,
        ),
        now,
    );
    mgr.print_zh(); // smoke test
}
