//! Settlement Engine 集成测试。
//!
//! 验证 process_fill() 完整工作流。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn create_engine() -> SettlementEngine {
    let config = SettlementConfig::default();
    let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
    SettlementEngine::new(config, repo).unwrap()
}

fn buy_fill(
    trade_id: &str,
    order_id: &str,
    market_id: &str,
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
        direction: pm_execution::order::Direction::Yes,
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
        direction: pm_execution::order::Direction::Yes,
        side: Side::Sell,
        fill_price: price,
        fill_quantity: qty,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

#[test]
fn test_single_buy_settlement() {
    let mut engine = create_engine();
    let result = engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 200.0));

    assert!(result.status.is_success());
    assert!(approx(result.balance_after, 9900.0)); // 10000 - 100
    assert_eq!(engine.position_mgr.open_count(), 1);
    assert!(engine.ledger.count() > 0);
}

#[test]
fn test_buy_then_sell_profit() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    let result = engine.process_fill(&sell_fill("T-002", "O-002", "mkt-btc", 0.60, 100.0));

    assert!(result.status.is_success());
    assert!(approx(result.realized_pnl, 10.0));
    assert!(approx(result.balance_after, 10010.0));
    assert_eq!(engine.position_mgr.open_count(), 0);
}

#[test]
fn test_buy_then_sell_loss() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    let result = engine.process_fill(&sell_fill("T-002", "O-002", "mkt-btc", 0.40, 100.0));

    assert!(result.status.is_success());
    assert!(approx(result.realized_pnl, -10.0));
    assert!(approx(result.balance_after, 9990.0));
}

#[test]
fn test_multiple_buys_add_to_position() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    engine.process_fill(&buy_fill("T-002", "O-002", "mkt-btc", 0.60, 100.0));

    assert_eq!(engine.position_mgr.open_count(), 1);
    let pos = engine
        .position_mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.quantity, 200.0));
    assert!(approx(pos.average_price, 0.55));
}

#[test]
fn test_partial_sell_reduces_position() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 200.0));
    engine.process_fill(&sell_fill("T-002", "O-002", "mkt-btc", 0.55, 80.0));

    assert_eq!(engine.position_mgr.open_count(), 1);
    let pos = engine
        .position_mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.quantity, 120.0));
}

#[test]
fn test_different_directions_separate_positions() {
    let mut engine = create_engine();

    let mut buy_yes = buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0);
    buy_yes.direction = pm_execution::order::Direction::Yes;
    engine.process_fill(&buy_yes);

    let mut buy_no = buy_fill("T-002", "O-002", "mkt-btc", 0.40, 100.0);
    buy_no.direction = pm_execution::order::Direction::No;
    engine.process_fill(&buy_no);

    assert_eq!(engine.position_mgr.open_count(), 2);
}

#[test]
fn test_validation_rejects_bad_price() {
    let mut engine = create_engine();
    let mut bad = buy_fill("T-001", "O-001", "mkt-btc", -0.50, 100.0);
    bad.fill_price = -0.50;
    let result = engine.process_fill(&bad);
    assert!(!result.status.is_success());
}

#[test]
fn test_validation_rejects_price_above_one() {
    let mut engine = create_engine();
    let mut bad = buy_fill("T-001", "O-001", "mkt-btc", 1.50, 100.0);
    bad.fill_price = 1.50;
    let result = engine.process_fill(&bad);
    assert!(!result.status.is_success());
}

#[test]
fn test_validation_rejects_sell_without_position() {
    let mut engine = create_engine();
    let result = engine.process_fill(&sell_fill("T-001", "O-001", "mkt-btc", 0.60, 100.0));
    assert!(!result.status.is_success());
}

#[test]
fn test_ledger_entries_on_multiple_fills() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    engine.process_fill(&buy_fill("T-002", "O-002", "mkt-eth", 0.45, 200.0));
    engine.process_fill(&sell_fill("T-003", "O-003", "mkt-btc", 0.55, 100.0));

    assert!(engine.ledger.count() >= 3);
}

#[test]
fn test_metrics_accurate() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    engine.process_fill(&buy_fill("T-002", "O-002", "mkt-eth", 0.45, 200.0));

    let snap = engine.metrics.snapshot();
    assert_eq!(snap.total_fills, 2);
    assert_eq!(snap.successful_settlements, 2);
    assert_eq!(snap.failed_settlements, 0);
}

#[test]
fn test_metrics_track_failures() {
    let mut engine = create_engine();

    let mut bad = buy_fill("T-001", "O-001", "mkt-btc", -1.0, 100.0);
    bad.fill_price = -1.0;
    engine.process_fill(&bad);

    let snap = engine.metrics.snapshot();
    assert_eq!(snap.failed_settlements, 1);
}

#[test]
fn test_repository_persists_results() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    engine.process_fill(&buy_fill("T-002", "O-002", "mkt-eth", 0.45, 200.0));

    let settlements = engine.repository.list_settlements().unwrap();
    assert_eq!(settlements.len(), 2);
}

#[test]
fn test_pnl_engine_after_multiple_trades() {
    let mut engine = create_engine();

    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    engine.process_fill(&sell_fill("T-002", "O-002", "mkt-btc", 0.60, 100.0));

    // 已实现盈亏 = 10.0
    assert!(approx(engine.pnl_engine.cumulative_roi(), 0.001)); // 10/10000
    assert_eq!(engine.pnl_engine.winning_trades(), 1);
    assert_eq!(engine.pnl_engine.losing_trades(), 0);
}

#[test]
fn test_settlement_id_unique() {
    let mut engine = create_engine();

    let r1 = engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));
    let r2 = engine.process_fill(&buy_fill("T-002", "O-002", "mkt-eth", 0.45, 200.0));

    assert_ne!(r1.settlement_id, r2.settlement_id);
}

#[test]
fn test_balance_init_correct() {
    let engine = create_engine();
    let bal = engine.balance_mgr.get("ACCT-MAIN-001").unwrap();
    assert!(approx(bal.available, 10000.0));
    assert!(approx(bal.frozen, 0.0));
    assert!(approx(bal.equity, 10000.0));
}

#[test]
fn test_event_bus_fires_events() {
    use pm_settlement::prelude::*;
    use std::sync::{Arc, Mutex};

    let mut engine = create_engine();
    let sink = Arc::new(Mutex::new(Vec::new()));

    struct TestSub {
        sink: Arc<Mutex<Vec<String>>>,
    }
    impl SettlementSubscriber for TestSub {
        fn name(&self) -> &str {
            "test"
        }
        fn on_event(&self, event: &SettlementEvent) -> anyhow::Result<()> {
            self.sink
                .lock()
                .unwrap()
                .push(event.event_name_zh().to_string());
            Ok(())
        }
    }

    engine
        .event_bus
        .subscribe(Box::new(TestSub { sink: sink.clone() }));
    engine.process_fill(&buy_fill("T-001", "O-001", "mkt-btc", 0.50, 100.0));

    let events = sink.lock().unwrap();
    assert!(events.iter().any(|e| e == "接收成交"));
    assert!(events.iter().any(|e| e == "结算完成"));
}
