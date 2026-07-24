//! OMS 集成测试 — 订单校验（P2-04 第十节）。

use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::{Balance, OrderType, TimeInForce};
use pm_oms::prelude::*;

fn build_oms() -> Oms {
    create_default_oms().expect("创建 OMS 失败")
}

fn input(client_id: &str, price: f64, qty: f64) -> CreateOrderInput {
    let mut i = CreateOrderInput::limit(
        client_id,
        "mkt-1",
        Direction::Yes,
        Side::Buy,
        price,
        qty,
        "S1",
        "R1",
        "O1",
    );
    i
}

#[test]
fn validation_passes_for_normal_order() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&input("CLI-V1", 0.45, 100.0), Local::now())
        .unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    assert!(r.all_passed);
    assert_eq!(order.status, OrderStatus::Validated);
}

#[test]
fn validation_rejects_zero_price() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&input("CLI-V2", 0.0, 100.0), Local::now())
        .unwrap();
    let r = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!r.all_passed);
    let s = r.summary_zh();
    assert!(s.contains("价格"));
    assert_eq!(order.status, OrderStatus::Rejected);
}

#[test]
fn validation_rejects_zero_quantity() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&input("CLI-V3", 0.45, 0.0), Local::now())
        .unwrap();
    let r = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!r.all_passed);
    assert_eq!(order.status, OrderStatus::Rejected);
}

#[test]
fn validation_rejects_insufficient_balance() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&input("CLI-V4", 0.45, 100.0), Local::now())
        .unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10.0)); // 10 < 45
    let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    assert!(!r.all_passed);
    assert!(r.summary_zh().contains("余额"));
}

#[test]
fn validation_skipped_balance_for_sell() {
    let oms = build_oms();
    let mut i = input("CLI-V5", 0.45, 100.0);
    i.side = Side::Sell;
    let mut order = oms.create_order(&i, Local::now()).unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(0.0)); // Sell 不消耗
    let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    assert!(r.all_passed, "Sell 应该跳过余额检查：{}", r.summary_zh());
}

#[test]
fn validation_rejects_market_order_with_ioc() {
    let oms = build_oms();
    let mut i = input("CLI-V6", 0.45, 100.0);
    i.order_type = OrderType::Market;
    i.time_in_force = TimeInForce::Ioc;
    let mut order = oms.create_order(&i, Local::now()).unwrap();
    let r = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!r.all_passed);
}

#[test]
fn validation_rejects_closed_market() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&input("CLI-V7", 0.45, 100.0), Local::now())
        .unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.market_open = false;
    let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    assert!(!r.all_passed);
    assert!(r.summary_zh().contains("市场"));
}

#[test]
fn validation_rejects_missing_market_id() {
    let oms = build_oms();
    let mut i = input("CLI-V8", 0.45, 100.0);
    i.market_id = "".into();
    let mut order = oms.create_order(&i, Local::now()).unwrap();
    let r = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!r.all_passed);
    assert!(r.summary_zh().contains("market_id"));
}

#[test]
fn validation_rejects_active_order_limit_exceeded() {
    let oms = build_oms();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(100_000.0));
    vctx.max_active_orders = 3;
    vctx.active_order_count = 3;
    let mut order = oms
        .create_order(&input("CLI-V9", 0.45, 100.0), Local::now())
        .unwrap();
    let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    assert!(!r.all_passed);
    assert!(r.summary_zh().contains("活跃"));
}
