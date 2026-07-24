//! OMS 集成测试 — 订单生命周期（P2-04 第十节）。

use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::{Balance, OrderType, TimeInForce};
use pm_oms::prelude::*;

fn build_oms() -> Oms {
    create_default_oms().expect("创建 OMS 失败")
}

fn make_input(client_id: &str) -> CreateOrderInput {
    CreateOrderInput::limit(
        client_id,
        "mkt-test",
        Direction::Yes,
        Side::Buy,
        0.45,
        100.0,
        "S1",
        "R1",
        "O1",
    )
}

#[tokio::test]
async fn full_lifecycle_create_validate_submit_fill() {
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms
        .create_order(&make_input("CLI-LIFECYCLE-001"), now)
        .unwrap();
    assert_eq!(order.status, OrderStatus::Created);

    // 校验通过
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    let r = oms.validate_order(&mut order, &vctx, now).unwrap();
    assert!(r.all_passed);
    assert_eq!(order.status, OrderStatus::Validated);

    // 提交到 Mock Gateway
    let gr = oms.submit_order(&mut order, now).await.unwrap();
    assert!(gr.success);
    // Mock Gateway 可能返回 Accepted / Filled / PartiallyFilled 都算合法
    assert!(matches!(
        order.status,
        OrderStatus::Accepted | OrderStatus::Filled | OrderStatus::PartiallyFilled
    ));
}

#[tokio::test]
async fn lifecycle_validation_failure_path() {
    let oms = build_oms();
    let mut input = make_input("CLI-FAIL-001");
    input.price = -1.0;
    let mut order = oms.create_order(&input, Local::now()).unwrap();
    let r = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!r.all_passed);
    assert_eq!(order.status, OrderStatus::Rejected);
}

#[tokio::test]
async fn lifecycle_cancel_before_submit() {
    let oms = build_oms();
    let mut order = oms
        .create_order(&make_input("CLI-CANCEL-001"), Local::now())
        .unwrap();
    let gr = oms
        .cancel_order(&mut order, "用户主动取消", Local::now())
        .await
        .unwrap();
    assert_eq!(order.status, OrderStatus::Cancelled);
    // 已终态，再次取消返回的 message 应包含 "非活跃"
    assert!(gr.message.contains("非活跃") || gr.message.contains("未提交"));
}

#[tokio::test]
async fn lifecycle_replace_via_cancel_create_submit() {
    let oms = build_oms();
    let now = Local::now();
    let mut old = oms
        .create_order(&make_input("CLI-REPLACE-001"), now)
        .unwrap();

    let mut new_input = make_input("CLI-REPLACE-002");
    new_input.price = 0.50;

    let new_order = oms.replace_order(&mut old, &new_input, now).await.unwrap();
    assert_eq!(old.status, OrderStatus::Cancelled);
    assert_ne!(new_order.order_id, old.order_id);
    assert_eq!(oms.list_orders().unwrap().len(), 2);
}

#[test]
fn lifecycle_status_history_recorded() {
    use pm_gateway::create_mock_gateway;
    use std::sync::Arc;
    let cfg = OmsConfig::default();
    let gw = Arc::from(create_mock_gateway());
    let oms = Oms::new(cfg, gw).unwrap();
    let order = oms
        .create_order(&make_input("CLI-HIST-001"), Local::now())
        .unwrap();
    // 初始状态历史：Created（Order::new 时记录）
    assert!(order.status_history.len() >= 1);
}

#[tokio::test]
async fn lifecycle_state_changes_persisted_in_repo() {
    use pm_gateway::create_mock_gateway;
    use std::sync::Arc;
    let cfg = OmsConfig::default();
    let gw = Arc::from(create_mock_gateway());
    let oms = Oms::new(cfg, gw).unwrap();
    let now = Local::now();
    let mut order = oms
        .create_order(&make_input("CLI-PERSIST-001"), now)
        .unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    oms.validate_order(&mut order, &vctx, now).unwrap();
    let changes = oms
        .repository()
        .list_status_changes(&order.order_id)
        .unwrap();
    assert!(!changes.is_empty());
}

#[test]
fn lifecycle_create_is_idempotent_by_client_id() {
    let oms = build_oms();
    let input = make_input("CLI-IDEMPOTENT-001");
    let o1 = oms.create_order(&input, Local::now()).unwrap();
    let o2 = oms.create_order(&input, Local::now()).unwrap();
    assert_eq!(o1.order_id, o2.order_id);
    assert_eq!(oms.list_orders().unwrap().len(), 1);
}
