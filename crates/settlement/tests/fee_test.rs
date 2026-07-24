//! Fee Engine 集成测试。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn taker_fill(price: f64, qty: f64) -> TradeFillEvent {
    TradeFillEvent {
        trade_id: "T-001".to_string(),
        order_id: "O-001".to_string(),
        client_order_id: "CLI-001".to_string(),
        exchange_order_id: None,
        market_id: "mkt-btc".to_string(),
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

fn maker_fill(price: f64, qty: f64) -> TradeFillEvent {
    let mut f = taker_fill(price, qty);
    f.is_taker = false;
    f
}

#[test]
fn test_zero_fee_engine() {
    let engine = FeeEngine::zero_fee();
    let fill = taker_fill(0.50, 200.0);
    let bd = engine.calculate(&fill);
    assert!(approx(bd.total_fee, 0.0));
    assert!(approx(bd.maker_fee, 0.0));
    assert!(approx(bd.taker_fee, 0.0));
    assert_eq!(bd.fee_rule, "ZeroFee");
}

#[test]
fn test_default_fee_rates() {
    let engine = FeeEngine::default();
    assert!(engine.active_rule.maker_rate > 0.0);
    assert!(engine.active_rule.taker_rate > engine.active_rule.maker_rate);
    assert!(engine.active_rule.trading_rate > 0.0);
}

#[test]
fn test_taker_vs_maker_fee() {
    let engine = FeeEngine::default();
    let taker_bd = engine.calculate(&taker_fill(0.50, 200.0));
    let maker_bd = engine.calculate(&maker_fill(0.50, 200.0));

    // Taker 手续费更高
    assert!(taker_bd.taker_fee > 0.0);
    assert!(maker_bd.maker_fee > 0.0);
    assert!(taker_bd.taker_fee > maker_bd.maker_fee);
}

#[test]
fn test_fee_proportional_to_notional() {
    let engine = FeeEngine::default();
    let small = engine.calculate(&taker_fill(0.50, 100.0)); // notional=50
    let large = engine.calculate(&taker_fill(0.50, 400.0)); // notional=200

    // 4x notional → 4x fee
    let ratio = large.taker_fee / small.taker_fee;
    assert!((ratio - 4.0).abs() < 0.01);
}

#[test]
fn test_min_fee_enforced() {
    let mut rule = FeeRule::zero_fee();
    rule.min_fee = 1.0;
    rule.taker_rate = 0.0005;
    let engine = FeeEngine::new(rule);
    let bd = engine.calculate(&taker_fill(0.50, 100.0));
    // 0.0005 * 50 = 0.025, but min = 1.0
    assert!(approx(bd.total_fee, 1.0));
}

#[test]
fn test_max_fee_capped() {
    let mut engine = FeeEngine::default();
    engine.active_rule.max_fee = 0.001;
    let bd = engine.calculate(&taker_fill(0.50, 10000.0)); // large notional
    assert!(bd.total_fee <= 0.001 + 1e-9);
}

#[test]
fn test_fee_components_sum_to_total() {
    let engine = FeeEngine::default();
    let bd = engine.calculate(&taker_fill(0.50, 200.0));
    let sum = bd.maker_fee + bd.taker_fee + bd.trading_fee + bd.settlement_fee;
    assert!(approx(sum, bd.total_fee));
}

#[test]
fn test_effective_rate() {
    let engine = FeeEngine::default();
    let taker_rate = engine.effective_rate(true);
    let maker_rate = engine.effective_rate(false);
    assert!(taker_rate > maker_rate);
}

#[test]
fn test_set_rule_switches() {
    let mut engine = FeeEngine::default();
    let old_name = engine.active_rule.name.clone();
    engine.set_rule(FeeRule::zero_fee());
    assert_ne!(engine.active_rule.name, old_name);
    assert_eq!(engine.active_rule.name, "ZeroFee");

    let bd = engine.calculate(&taker_fill(0.50, 100.0));
    assert!(approx(bd.total_fee, 0.0));
}

#[test]
fn test_fee_with_settlement_engine() {
    let mut config = SettlementConfig::default();
    config.fee_rule = FeeRule::default(); // standard fees
    let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
    let mut engine = SettlementEngine::new(config, repo).unwrap();

    let result = engine.process_fill(&taker_fill(0.50, 200.0));
    assert!(result.status.is_success());
    assert!(result.fee_breakdown.total_fee > 0.0);
}
