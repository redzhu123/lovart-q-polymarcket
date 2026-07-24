//! Settlement Engine 全流程集成测试。
//!
//! 验证从成交到结算的端到端流程。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn create_engine_with_capital(capital: f64) -> SettlementEngine {
    let config = SettlementConfig {
        initial_capital: capital,
        default_account_id: "ACCT-MAIN-001".to_string(),
        fee_rule: FeeRule::zero_fee(),
        enable_event_bus: true,
    };
    let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
    SettlementEngine::new(config, repo).unwrap()
}

fn buy(
    tid: &str,
    oid: &str,
    mid: &str,
    dir: pm_execution::order::Direction,
    price: f64,
    qty: f64,
) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: tid.to_string(),
        order_id: oid.to_string(),
        client_order_id: format!("C-{}", oid),
        exchange_order_id: None,
        market_id: mid.to_string(),
        account_id: "ACCT-MAIN-001".to_string(),
        direction: dir,
        side: Side::Buy,
        fill_price: price,
        fill_quantity: qty,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

fn sell(
    tid: &str,
    oid: &str,
    mid: &str,
    dir: pm_execution::order::Direction,
    price: f64,
    qty: f64,
) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: tid.to_string(),
        order_id: oid.to_string(),
        client_order_id: format!("C-{}", oid),
        exchange_order_id: None,
        market_id: mid.to_string(),
        account_id: "ACCT-MAIN-001".to_string(),
        direction: dir,
        side: Side::Sell,
        fill_price: price,
        fill_quantity: qty,
        filled_at: Local::now(),
        is_taker: true,
        gateway_name: "Mock".to_string(),
    }
}

#[test]
fn test_full_trading_day() {
    let mut engine = create_engine_with_capital(10000.0);

    // 开盘：3 笔买入
    engine.process_fill(&buy(
        "T01",
        "O01",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        100.0,
    ));
    engine.process_fill(&buy(
        "T02",
        "O02",
        "mkt-eth",
        pm_execution::order::Direction::Yes,
        0.40,
        200.0,
    ));
    engine.process_fill(&buy(
        "T03",
        "O03",
        "mkt-sol",
        pm_execution::order::Direction::No,
        0.30,
        150.0,
    ));

    // 验证状态
    assert_eq!(engine.position_mgr.open_count(), 3);
    let bal = engine.balance_mgr.get("ACCT-MAIN-001").unwrap();
    let total_cost = 50.0 + 80.0 + 45.0; // 175
    assert!(approx(bal.available, 10000.0 - total_cost));

    // 盘中：加仓 1 笔
    engine.process_fill(&buy(
        "T04",
        "O04",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.55,
        50.0,
    ));
    assert_eq!(engine.position_mgr.open_count(), 3); // 加仓，不是新开
    let pos = engine
        .position_mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.quantity, 150.0));

    // 盘中：平仓 1 笔（盈利）
    engine.process_fill(&sell(
        "T05",
        "O05",
        "mkt-eth",
        pm_execution::order::Direction::Yes,
        0.50,
        200.0,
    ));
    assert_eq!(engine.position_mgr.open_count(), 2);
    assert_eq!(engine.position_mgr.closed_count(), 1);

    // 验证 PnL
    assert!(approx(engine.pnl_engine.accumulated_realized_pnl, 20.0)); // 200*(0.50-0.40)

    // 收盘：全部平仓
    engine.process_fill(&sell(
        "T06",
        "O06",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.52,
        150.0,
    ));
    engine.process_fill(&sell(
        "T07",
        "O07",
        "mkt-sol",
        pm_execution::order::Direction::No,
        0.25,
        150.0,
    ));

    assert_eq!(engine.position_mgr.open_count(), 0);
    assert_eq!(engine.position_mgr.closed_count(), 3);

    // 验证流水
    assert!(engine.ledger.count() >= 7);

    // 验证仓库
    let settlements = engine.repository.list_settlements().unwrap();
    assert_eq!(settlements.len(), 7);
}

#[test]
fn test_multi_account_scenario() {
    let config = SettlementConfig {
        initial_capital: 10000.0,
        default_account_id: "ACCT-A".to_string(),
        fee_rule: FeeRule::zero_fee(),
        enable_event_bus: false,
    };
    let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
    let mut engine = SettlementEngine::new(config, repo).unwrap();

    // 第二个账户
    engine
        .balance_mgr
        .init_account("ACCT-B".into(), 5000.0, Local::now());

    // 账户 A 买入
    let mut fill_a = buy(
        "TA01",
        "OA01",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        100.0,
    );
    fill_a.account_id = "ACCT-A".into();
    engine.process_fill(&fill_a);

    // 账户 B 买入
    let mut fill_b = buy(
        "TB01",
        "OB01",
        "mkt-eth",
        pm_execution::order::Direction::Yes,
        0.45,
        100.0,
    );
    fill_b.account_id = "ACCT-B".into();
    engine.process_fill(&fill_b);

    assert_eq!(engine.balance_mgr.account_count(), 2);
}

#[test]
fn test_settlement_idempotency() {
    let mut engine = create_engine_with_capital(10000.0);

    // 同一个订单的多次成交（部分成交场景）
    let r1 = engine.process_fill(&buy(
        "T01",
        "OMS-001",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        50.0,
    ));
    let r2 = engine.process_fill(&buy(
        "T02",
        "OMS-001",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.52,
        50.0,
    ));

    assert!(r1.status.is_success());
    assert!(r2.status.is_success());
    assert_ne!(r1.settlement_id, r2.settlement_id);

    // 同一持仓被加仓
    let pos = engine
        .position_mgr
        .find_open("mkt-btc", pm_execution::order::Direction::Yes)
        .unwrap();
    assert!(approx(pos.quantity, 100.0));
    assert_eq!(pos.order_ids.len(), 1); // same order_id
    assert_eq!(pos.trade_ids.len(), 2); // two different trade_ids
}

#[test]
fn test_metrics_end_to_end() {
    let mut engine = create_engine_with_capital(10000.0);

    // 成功交易
    engine.process_fill(&buy(
        "T01",
        "O01",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        100.0,
    ));
    engine.process_fill(&sell(
        "T02",
        "O02",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.55,
        100.0,
    ));

    // 失败交易
    let mut bad = buy(
        "T03",
        "O03",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        -0.50,
        100.0,
    );
    bad.fill_price = -0.50;
    engine.process_fill(&bad);

    let snap = engine.metrics.snapshot();
    assert_eq!(snap.total_fills, 3);
    assert_eq!(snap.successful_settlements, 2);
    assert_eq!(snap.failed_settlements, 1);
    assert!(approx(snap.success_rate, 2.0 / 3.0));
}

#[test]
fn test_print_dashboard() {
    let mut engine = create_engine_with_capital(10000.0);
    engine.process_fill(&buy(
        "T01",
        "O01",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        0.50,
        100.0,
    ));
    engine.print_dashboard(); // smoke test
}

#[test]
fn test_factory_functions() {
    let engine = pm_settlement::create_default_settlement().unwrap();
    assert_eq!(engine.position_mgr.open_count(), 0);

    let dir = std::env::temp_dir();
    let engine2 = pm_settlement::create_csv_settlement(
        dir.join("int_fills.csv"),
        dir.join("int_settlements.csv"),
        dir.join("int_ledger.csv"),
    )
    .unwrap();
    assert_eq!(engine2.position_mgr.open_count(), 0);
}
