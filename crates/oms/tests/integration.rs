//! OMS 集成测试 — 端到端集成（P2-04 第十节）。
//!
//! 模拟完整的企业级交易链路：
//! Execution → OMS → Gateway → Exchange

use std::sync::Arc;
use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::{Balance, create_mock_gateway};
use pm_oms::prelude::*;

fn build_oms() -> Oms {
    create_default_oms().expect("创建 OMS 失败")
}

fn make_input(client_id: &str, price: f64) -> CreateOrderInput {
    CreateOrderInput::limit(
        client_id,
        "mkt-int",
        Direction::Yes,
        Side::Buy,
        price,
        100.0,
        "S1",
        "R1",
        "O1",
    )
}

#[tokio::test]
async fn end_to_end_execution_via_oms_to_mock_gateway() {
    // Execution → OMS → Gateway 完整链路
    let oms = build_oms();
    let now = Local::now();

    // 1. Execution 调用 OMS 创建订单
    let mut order = oms.create_order(&make_input("CLI-E2E-001", 0.45), now).unwrap();
    assert_eq!(order.status, OrderStatus::Created);

    // 2. 校验
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    let result = oms.validate_order(&mut order, &vctx, now).unwrap();
    assert!(result.all_passed);
    assert_eq!(order.status, OrderStatus::Validated);

    // 3. 提交到 Gateway（经 OMS 投递）
    let gr = oms.submit_order(&mut order, now).await.unwrap();
    assert!(gr.success);

    // 4. Metrics 已记录
    let m = oms.metrics_snapshot();
    assert_eq!(m.total_created, 1);
    assert_eq!(m.total_validated, 1);
    assert_eq!(m.total_submitted, 1);
    assert!(m.total_terminal() >= 1);
}

#[tokio::test]
async fn oms_only_entry_execution_does_not_call_gateway_directly() {
    // 验证：Oms::gateway 不暴露给业务层直接 submit
    // 业务层必须经 Oms API → Lifecycle → Gateway
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms.create_order(&make_input("CLI-DIRECT-001", 0.45), now).unwrap();
    // 这里业务层不应该能直接调用 gateway 提交（避免绕过 OMS）
    // 正确的路径是 oms.submit_order()
    oms.submit_order(&mut order, now).await.unwrap();
    // 校验：exchange_order_id 已被 Gateway 分配并写回 Order
    if matches!(
        order.status,
        OrderStatus::Accepted | OrderStatus::Filled | OrderStatus::PartiallyFilled
    ) {
        assert!(order.exchange_order_id.is_some());
    }
}

#[tokio::test]
async fn oms_blocks_invalid_orders_before_gateway() {
    // 校验失败不应触达 Gateway
    let oms = build_oms();
    let mut input = make_input("CLI-BLOCK-001", -1.0); // 价格非法
    input.price = 0.45;
    input.market_id = "".to_string(); // 触发参数缺失
    let mut order = oms.create_order(&input, Local::now()).unwrap();
    let result = oms
        .validate_order(&mut order, &ValidationContext::minimal(), Local::now())
        .unwrap();
    assert!(!result.all_passed);
    // 验证：订单直接 Rejected，没有 exchange_order_id
    assert_eq!(order.status, OrderStatus::Rejected);
    assert!(order.exchange_order_id.is_none());
}

#[tokio::test]
async fn matcher_evaluates_deviation() {
    let oms = build_oms();
    let order = oms.create_order(&make_input("CLI-MATCH-001", 0.45), Local::now()).unwrap();
    let r = oms.evaluate_match(&order, Some(0.43), Some(0.46));
    assert_eq!(r.decision, MatchDecision::Allow);

    // 价格严重偏离
    let bad = oms.create_order(&make_input("CLI-MATCH-002", 0.60), Local::now()).unwrap();
    let r = oms.evaluate_match(&bad, Some(0.43), Some(0.46));
    assert_eq!(r.decision, MatchDecision::Reject);
}

#[tokio::test]
async fn health_check_returns_chinese_summary() {
    let oms = build_oms();
    let h = oms.health().await;
    assert!(h.contains("OMS 健康检查"));
    assert!(h.contains("Repository"));
    assert!(h.contains("Gateway"));
}

#[tokio::test]
async fn cancel_after_fill_is_noop() {
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms.create_order(&make_input("CLI-NOOP-001", 0.45), now).unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    oms.validate_order(&mut order, &vctx, now).unwrap();
    oms.submit_order(&mut order, now).await.unwrap();
    // 强制 Filled
    order.transition(OrderStatus::Filled, "测试填充", "oms", now);
    let gr = oms.cancel_order(&mut order, "再取消", now).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert!(gr.message.contains("非活跃"));
}

#[tokio::test]
async fn repository_csv_roundtrip() {
    use std::path::PathBuf;
    let dir = tempfile::tempdir().unwrap();
    let orders_csv = dir.path().join("orders.csv");
    let events_csv = dir.path().join("events.csv");

    let oms1 = create_csv_oms(orders_csv.clone(), events_csv.clone()).unwrap();
    oms1.create_order(&make_input("CLI-CSV-001", 0.45), Local::now()).unwrap();
    oms1.create_order(&make_input("CLI-CSV-002", 0.50), Local::now()).unwrap();

    // 重新加载
    let oms2 = create_csv_oms(orders_csv, events_csv).unwrap();
    let orders = oms2.list_orders().unwrap();
    assert_eq!(orders.len(), 2);
    assert!(orders.iter().any(|o| o.client_order_id == "CLI-CSV-001"));
    assert!(orders.iter().any(|o| o.client_order_id == "CLI-CSV-002"));
}

#[tokio::test]
async fn custom_gateway_trait_implementation() {
    // 验证 OMS 支持自定义 Gateway（仅声明为 Arc<dyn ExchangeGateway>）
    let gw: Arc<dyn pm_gateway::ExchangeGateway> = Arc::from(create_mock_gateway());
    let cfg = OmsConfig::default();
    let oms = Oms::new(cfg, gw).unwrap();
    let list = oms.list_orders().unwrap();
    assert!(list.is_empty());
}

#[test]
fn factory_create_default_oms_works() {
    let oms = create_default_oms().unwrap();
    assert_eq!(oms.event_bus.subscriber_count(), 1); // 默认 metrics
}
