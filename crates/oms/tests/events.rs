//! OMS 集成测试 — 事件总线（P2-04 第十节）。

use std::sync::{Arc, Mutex};
use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::Balance;
use pm_oms::prelude::*;

/// 测试用订阅者：收集事件名。
struct CollectSub {
    name: String,
    sink: Arc<Mutex<Vec<String>>>,
}
impl CollectSub {
    fn new(name: &str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name: name.to_string(),
            sink,
        }
    }
}
impl Subscriber for CollectSub {
    fn name(&self) -> &str {
        &self.name
    }
    fn on_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        self.sink.lock().unwrap().push(event.event_name_zh().to_string());
        Ok(())
    }
}

fn build_oms() -> Oms {
    create_default_oms().expect("创建 OMS 失败")
}

fn make_input() -> CreateOrderInput {
    CreateOrderInput::limit(
        "CLI-EV-001",
        "mkt-1",
        Direction::Yes,
        Side::Buy,
        0.45,
        100.0,
        "S1",
        "R1",
        "O1",
    )
}

#[test]
fn order_created_event_published() {
    let oms = build_oms();
    oms.create_order(&make_input(), Local::now()).unwrap();
    let m = oms.metrics_snapshot();
    assert_eq!(m.total_created, 1);
}

#[tokio::test]
async fn order_validated_and_submitted_events_published() {
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms.create_order(&make_input(), now).unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    oms.validate_order(&mut order, &vctx, now).unwrap();
    oms.submit_order(&mut order, now).await.unwrap();
    let m = oms.metrics_snapshot();
    assert_eq!(m.total_validated, 1);
    assert_eq!(m.total_submitted, 1);
    assert!(m.total_accepted + m.total_filled + m.total_partially_filled >= 1);
}

#[tokio::test]
async fn order_cancelled_event_published() {
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms.create_order(&make_input(), now).unwrap();
    oms.cancel_order(&mut order, "测试", now).await.unwrap();
    let m = oms.metrics_snapshot();
    assert_eq!(m.total_cancelled, 1);
}

#[test]
fn custom_subscriber_receives_events() {
    let mut oms = build_oms();
    let sink = Arc::new(Mutex::new(Vec::new()));
    let sub = CollectSub::new("audit", sink.clone());
    oms.subscribe(Box::new(sub));
    oms.create_order(&make_input(), Local::now()).unwrap();
    let s = sink.lock().unwrap();
    assert!(!s.is_empty());
    assert!(s[0].contains("订单创建"));
}

#[test]
fn subscriber_failure_does_not_break_publish() {
    struct FailSub;
    impl Subscriber for FailSub {
        fn name(&self) -> &str { "fail" }
        fn on_event(&self, _: &OrderEvent) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("模拟失败"))
        }
    }
    let mut oms = build_oms();
    oms.subscribe(Box::new(FailSub));
    // 不应 panic
    oms.create_order(&make_input(), Local::now()).unwrap();
}

#[tokio::test]
async fn full_event_chain_recorded() {
    let oms = build_oms();
    let now = Local::now();
    let mut order = oms.create_order(&make_input(), now).unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    oms.validate_order(&mut order, &vctx, now).unwrap();
    oms.submit_order(&mut order, now).await.unwrap();
    let m = oms.metrics_snapshot();
    // 至少 create + validated + submitted = 3 个事件（加上 accepted/filled 等）
    assert!(m.total_created >= 1);
    assert!(m.total_validated >= 1);
    assert!(m.total_submitted >= 1);
}

#[test]
fn event_to_csv_row_format() {
    let ev = OrderEvent::OrderFilled {
        order_id: "OMS-001".into(),
        avg_price: 0.45,
        slippage: 0.01,
        timestamp: Local::now(),
    };
    let row = pm_oms::events::event_to_csv_row(&ev);
    assert_eq!(row.len(), 5);
    assert_eq!(row[1], "OrderFilled");
    assert_eq!(row[2], "完全成交");
}
