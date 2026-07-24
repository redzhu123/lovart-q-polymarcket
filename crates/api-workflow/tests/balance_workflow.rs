//! 余额 Workflow 测试。
//!
//! 验证「检查余额」与「同步余额」步骤：GET /balances 被调用、成功、并记录。

use pm_api_workflow::config::WorkflowConfig;
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn checking_balance_step_queries_balances() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    let step = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::CheckingBalance)
        .expect("应包含「检查余额」步骤");
    assert!(step.success, "「检查余额」步骤应成功");
    let call = step
        .api_calls
        .iter()
        .find(|c| c.method == "GET" && c.path == "/balances");
    assert!(call.is_some(), "「检查余额」应发起 GET /balances 调用");
}

#[tokio::test]
async fn sync_balance_step_after_position() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    // 持仓更新后必须同步余额（SyncBalance 在 SyncPosition 之后）
    let pos_idx = report
        .trace
        .steps
        .iter()
        .position(|s| s.step == WorkflowState::SyncPosition);
    let bal_idx = report
        .trace
        .steps
        .iter()
        .position(|s| s.step == WorkflowState::SyncBalance);
    assert!(pos_idx.is_some() && bal_idx.is_some(), "应包含同步持仓与同步余额步骤");
    assert!(
        bal_idx.unwrap() > pos_idx.unwrap(),
        "同步余额必须在同步持仓之后"
    );

    let bal_call = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::SyncBalance)
        .and_then(|s| s.api_calls.iter().find(|c| c.path == "/balances"));
    assert!(bal_call.is_some(), "「同步余额」应发起 GET /balances 调用");
}
