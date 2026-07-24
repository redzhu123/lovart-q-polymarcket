//! 市场数据 Workflow 测试。
//!
//! 验证「加载市场」步骤：GET /markets 被调用、成功、并记录。

use pm_api_workflow::config::WorkflowConfig;
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn loading_market_step_loads_markets() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    assert!(report.success, "完整生命周期应成功: {}", report.summary_zh());

    let market_step = report
        .trace
        .steps
        .iter()
        .find(|s| s.step == WorkflowState::LoadingMarket);
    assert!(market_step.is_some(), "应包含「加载市场」步骤");
    let step = market_step.unwrap();
    assert!(step.success, "「加载市场」步骤应成功");

    let has_markets_call = step
        .api_calls
        .iter()
        .any(|c| c.method == "GET" && c.path == "/markets");
    assert!(has_markets_call, "「加载市场」应发起 GET /markets 调用");
}

#[tokio::test]
async fn market_call_appears_in_api_sequence() {
    init();
    let cfg = WorkflowConfig::default();
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_full_lifecycle().await;

    let first_read = report.api_sequence.first();
    assert!(first_read.is_some(), "应至少有一条 API 调用");
    assert_eq!(first_read.unwrap().path, "/markets", "首条 API 调用应为 /markets");
}
