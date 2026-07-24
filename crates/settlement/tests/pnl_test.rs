//! PnL Settlement 集成测试。

use chrono::Local;
use pm_core::Side;
use pm_settlement::prelude::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn create_position(
    id: &str,
    market: &str,
    direction: pm_execution::order::Direction,
    qty: f64,
    price: f64,
    order_id: &str,
    trade_id: &str,
) -> PositionState {
    let now = Local::now();
    PositionState::open(
        id.to_string(),
        market.to_string(),
        direction,
        Side::Buy,
        qty,
        price,
        order_id.to_string(),
        trade_id.to_string(),
        now,
    )
}

#[test]
fn test_empty_pnl_report() {
    let engine = PnLEngine::new(10000.0);
    let report = engine.calculate(&[], &[], Local::now());
    assert!(approx(report.realized_pnl, 0.0));
    assert!(approx(report.unrealized_pnl, 0.0));
    assert!(approx(report.total_pnl, 0.0));
    assert!(approx(report.roi, 0.0));
    assert_eq!(report.open_count, 0);
    assert_eq!(report.closed_count, 0);
}

#[test]
fn test_record_realized_accumulates() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(100.0);
    engine.record_realized(-50.0);
    engine.record_realized(25.0);

    assert!(approx(engine.accumulated_realized_pnl, 75.0));
}

#[test]
fn test_win_rate() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(10.0);
    engine.record_realized(20.0);
    engine.record_realized(-5.0);

    assert_eq!(engine.winning_trades(), 2);
    assert_eq!(engine.losing_trades(), 1);
    assert!(approx(engine.win_rate(), 2.0 / 3.0));
}

#[test]
fn test_profit_factor() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(100.0);
    engine.record_realized(50.0);
    engine.record_realized(-30.0);
    engine.record_realized(-20.0);

    // avg_profit = (100+50)/2 = 75, avg_loss = (30+20)/2 = 25
    assert!(approx(engine.profit_factor(), 3.0));
}

#[test]
fn test_profit_factor_no_losses() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(100.0);
    assert!(engine.profit_factor().is_infinite());
}

#[test]
fn test_profit_factor_no_trades() {
    let engine = PnLEngine::new(10000.0);
    assert_eq!(engine.winning_trades(), 0);
    assert_eq!(engine.losing_trades(), 0);
    assert!(approx(engine.win_rate(), 0.0));
}

#[test]
fn test_cumulative_roi() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(500.0);
    assert!(approx(engine.cumulative_roi(), 0.05)); // 500/10000
}

#[test]
fn test_calculate_from_open_positions() {
    let engine = PnLEngine::new(10000.0);
    let now = Local::now();

    let pos = create_position(
        "SPOS-001",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        100.0,
        0.50,
        "O-001",
        "T-001",
    );

    let report = engine.calculate(&[&pos], &[], now);
    assert_eq!(report.open_count, 1);
    assert!(approx(report.cost_basis, 50.0));
}

#[test]
fn test_calculate_from_closed_positions() {
    let engine = PnLEngine::new(10000.0);
    let now = Local::now();

    let mut pos = create_position(
        "SPOS-001",
        "mkt-btc",
        pm_execution::order::Direction::Yes,
        100.0,
        0.50,
        "O-001",
        "T-001",
    );
    pos.reduce(100.0, 0.60, now); // 平仓盈利 10

    let report = engine.calculate(&[], &[pos], now);
    assert_eq!(report.closed_count, 1);
    assert!(approx(report.realized_pnl, 10.0));
}

#[test]
fn test_historical_pnl_storage() {
    let mut engine = PnLEngine::new(10000.0);
    engine.record_realized(10.0);
    engine.record_realized(-5.0);
    engine.record_realized(20.0);

    let history = engine.historical_pnl();
    assert_eq!(history.len(), 3);
    assert!(approx(history[0], 10.0));
    assert!(approx(history[1], -5.0));
    assert!(approx(history[2], 20.0));
}

#[test]
fn test_print_zh() {
    let engine = PnLEngine::new(10000.0);
    let report = engine.calculate(&[], &[], Local::now());
    engine.print_zh(&report); // smoke test
}
