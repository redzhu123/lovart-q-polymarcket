//! 持仓 Workflow 测试。
//!
//! 验证「同步持仓」步骤：GET /positions 被调用、成功、并记录。

use pm_api_workflow::config::WorkflowConfig;
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn sync_position_step_queries_positions() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    let step = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::SyncPosition)
        .expect("应包含「同步持仓」步骤");
    assert!(step.success, "「同步持仓」步骤应成功");
    let call = step
        .api_calls
        .iter()
        .find(|c| c.method == "GET" && c.path == "/positions");
    assert!(call.is_some(), "「同步持仓」应发起 GET /positions 调用");
}

#[tokio::test]
async fn sync_position_after_sync_trade() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    // 成交后必须同步持仓（SyncPosition 在 SyncTrade 之后）
    let trade_idx = report
        .trace
        .steps
        .iter()
        .position(|s| s.step == WorkflowState::SyncTrade);
    let pos_idx = report
        .trace
        .steps
        .iter()
        .position(|s| s.step == WorkflowState::SyncPosition);
    assert!(trade_idx.is_some() && pos_idx.is_some());
    assert!(
        pos_idx.unwrap() > trade_idx.unwrap(),
        "同步持仓必须在同步成交之后"
    );
}
