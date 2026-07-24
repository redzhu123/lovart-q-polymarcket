//! OMS 集成测试 — 启动恢复（P2-04 第十节）。

use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::Balance;
use pm_oms::prelude::*;
use std::path::PathBuf;

fn build_oms_with_csv(dir: &std::path::Path) -> Oms {
    let orders_csv = dir.join("orders.csv");
    let events_csv = dir.join("events.csv");
    create_csv_oms(orders_csv, events_csv).expect("创建 OMS 失败")
}

fn make_input(client_id: &str) -> CreateOrderInput {
    CreateOrderInput::limit(
        client_id,
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

#[tokio::test]
async fn recover_empty_repo() {
    let dir = tempfile::tempdir().unwrap();
    let oms = build_oms_with_csv(dir.path());
    let report = oms.recover().await;
    assert_eq!(report.pending_recovery, 0);
    assert_eq!(report.failed_count, 0);
}

#[tokio::test]
async fn recover_only_terminal_orders_no_sync() {
    let dir = tempfile::tempdir().unwrap();
    let oms = build_oms_with_csv(dir.path());
    let mut order = oms
        .create_order(&make_input("CLI-R1"), Local::now())
        .unwrap();
    let mut vctx = ValidationContext::minimal();
    vctx.balance = Some(Balance::mock(10_000.0));
    oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
    oms.submit_order(&mut order, Local::now()).await.unwrap();
    oms.cancel_order(&mut order, "测试", Local::now())
        .await
        .unwrap();

    let report = oms.recover().await;
    assert_eq!(report.pending_recovery, 0);
}

#[tokio::test]
async fn recover_persists_across_instances() {
    let dir = tempfile::tempdir().unwrap();
    {
        let oms = build_oms_with_csv(dir.path());
        oms.create_order(&make_input("CLI-PERSIST-1"), Local::now())
            .unwrap();
    }
    // 重新创建实例（模拟重启）
    let oms2 = build_oms_with_csv(dir.path());
    let orders = oms2.list_orders().unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].client_order_id, "CLI-PERSIST-1");
}

#[tokio::test]
async fn recover_with_active_orders_via_csv() {
    let dir = tempfile::tempdir().unwrap();
    {
        let oms = build_oms_with_csv(dir.path());
        // 创建 3 个活跃订单
        for i in 0..3 {
            oms.create_order(&make_input(&format!("CLI-ACTIVE-{}", i)), Local::now())
                .unwrap();
        }
    }
    let oms2 = build_oms_with_csv(dir.path());
    let report = oms2.recover().await;
    // 这些订单没 exchange_order_id，恢复时会跳过
    assert_eq!(report.total_loaded, 3);
    assert_eq!(report.synced_count, 3); // 没有 eid 被识别为 synced
    assert_eq!(report.failed_count, 0);
}

#[tokio::test]
async fn sync_order_without_exchange_id_noop() {
    let dir = tempfile::tempdir().unwrap();
    let oms = build_oms_with_csv(dir.path());
    let mut order = oms
        .create_order(&make_input("CLI-SYNC-1"), Local::now())
        .unwrap();
    let report = oms.sync_order(&mut order).await.unwrap();
    assert!(!report.status_changed);
    assert!(report.message.contains("未提交"));
}

#[tokio::test]
async fn recovery_report_summary_chinese() {
    let dir = tempfile::tempdir().unwrap();
    let oms = build_oms_with_csv(dir.path());
    oms.create_order(&make_input("CLI-REC"), Local::now())
        .unwrap();
    let report = oms.recover().await;
    let s = report.summary_zh();
    assert!(s.contains("OMS 恢复报告"));
    assert!(s.contains("加载订单"));
}
