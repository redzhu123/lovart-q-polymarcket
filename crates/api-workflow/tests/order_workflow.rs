//! 订单 Workflow 测试。
//!
//! 验证完整下单生命周期（DryRun）：构建订单 -> DryRun 提交（不发送）
//! -> 同步订单 -> 同步成交。校验完整性规则通过、无真实写操作。

use pm_api_workflow::config::WorkflowConfig;
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn order_lifecycle_dryrun_completes() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    assert!(report.success, "订单生命周期应成功: {}", report.summary_zh());
    assert!(report.validation.passed, "校验应通过");
}

#[tokio::test]
async fn submitting_order_is_dryrun() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    // 提交订单步骤必须存在且为 DryRun（未发送）
    let submit_step = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::SubmittingOrder);
    assert!(submit_step.is_some(), "应包含「提交订单(DryRun)」步骤");
    assert!(submit_step.unwrap().success);

    // 构建订单步骤应包含一个 dry_run 的 POST /order
    let build_step = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::BuildingOrder)
        .expect("应包含「构建订单」步骤");
    let dryrun_post = build_step
        .api_calls
        .iter()
        .find(|c| c.method == "POST" && c.path == "/order");
    assert!(dryrun_post.is_some(), "「构建订单」应构造 POST /order 请求");
    assert!(dryrun_post.unwrap().dry_run, "POST /order 必须为 DryRun（未发送）");
}

#[tokio::test]
async fn order_status_queried_after_submit() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    // 提交后必须查询订单状态（SyncOrder）
    let has_sync_order = report
        .trace
        .steps
        .iter()
        .any(|s| s.step == WorkflowState::SyncOrder && s.success);
    assert!(has_sync_order, "提交订单后必须查询订单状态（同步订单）");

    let sync_order_call = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::SyncOrder)
        .and_then(|s| s.api_calls.iter().find(|c| c.path == "/orders"));
    assert!(sync_order_call.is_some(), "「同步订单」应发起 GET /orders 调用");
}

#[tokio::test]
async fn no_real_write_operations() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    let real_writes: Vec<_> = report
        .api_sequence
        .iter()
        .filter(|c| c.is_write() && !c.dry_run)
        .collect();
    assert!(real_writes.is_empty(), "DryRun 不应发送任何真实写操作: {:?}", real_writes);
}
