//! Live ReadOnly Workflow 测试。
//!
//! - 非忽略用例：enable_live_reads=false -> Mock 客户端离线校验只读约束（无写操作、校验通过）。
//! - 忽略用例：enable_live_reads=true -> 真实网络（需代理 + 认证）。

use pm_api_workflow::config::{WorkflowConfig, WorkflowMode};
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn readonly_enforces_no_writes() {
    init();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::LiveReadOnly,
        // enable_live_reads 默认 false -> Mock 客户端，可离线校验只读约束
        ..WorkflowConfig::default()
    };
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_readonly_lifecycle().await;

    // 不应出现任何写操作
    let writes: Vec<_> = report
        .api_sequence
        .iter()
        .filter(|c| c.is_write())
        .collect();
    assert!(writes.is_empty(), "Live ReadOnly 不应包含任何写操作: {:?}", writes);

    // 不应出现下单相关状态
    let has_write_state = report
        .trace
        .steps
        .iter()
        .any(|s| s.step.is_write_state());
    assert!(!has_write_state, "Live ReadOnly 不应包含下单相关步骤");

    assert!(report.validation.passed, "只读校验应通过: {}", report.summary_zh());
}

#[tokio::test]
async fn readonly_reads_markets_and_orderbook() {
    init();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::LiveReadOnly,
        ..WorkflowConfig::default()
    };
    let mut engine = WorkflowEngine::new(cfg);
    let report = engine.run_readonly_lifecycle().await;

    let paths: Vec<&str> = report
        .api_sequence
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    assert!(paths.iter().any(|p| *p == "/markets"), "应读取 /markets");
    assert!(paths.iter().any(|p| p.starts_with("/book")), "应读取 /book");
}

#[tokio::test]
#[ignore = "Live 测试 - 需要网络连接和 HTTPS_PROXY"]
async fn live_readonly_real_network() {
    init();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::LiveReadOnly,
        enable_live_reads: true,
        ..WorkflowConfig::default()
    };
    // 真实网络：需 HTTPS_PROXY（中国用户）+ 可选 POLYMARKET_API_KEY
    let report = pm_api_workflow::run_live_readonly(&cfg).await.unwrap();
    tracing::info!("{}", report.summary_zh());
    assert!(report.total_steps > 0);
}
