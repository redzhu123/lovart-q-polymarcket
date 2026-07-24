//! Replay Workflow 测试。
//!
//! 验证从 fixtures/ 完整回放：不访问网络、生成报告文件、校验通过。

use pm_api_workflow::config::{WorkflowConfig, WorkflowMode};
use pm_api_workflow::prelude::*;

fn init() {
    pm_api_workflow::init_logging("warn");
}

#[tokio::test]
async fn replay_completes_from_fixtures() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::Replay,
        report_dir: dir.path().to_str().unwrap().to_string(),
        ..WorkflowConfig::default()
    };

    // Mock 客户端，不访问网络
    assert_eq!(cfg.to_api_test_config().mode, pm_api_test::client::config::ClientMode::Mock);

    let report = pm_api_workflow::run_replay(&cfg).await.unwrap();

    assert!(report.success, "Replay 应成功: {}", report.summary_zh());
    assert!(report.validation.passed, "Replay 校验应通过");
    assert_eq!(report.mode, WorkflowMode::Replay.as_zh());
}

#[tokio::test]
async fn replay_generates_report_files() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::Replay,
        report_dir: dir.path().to_str().unwrap().to_string(),
        ..WorkflowConfig::default()
    };

    pm_api_workflow::run_replay(&cfg).await.unwrap();

    let md = dir.path().join("workflow-report.md");
    let json = dir.path().join("workflow-report.json");
    let trace = dir.path().join("workflow-trace.json");
    assert!(md.exists(), "workflow-report.md 应已生成");
    assert!(json.exists(), "workflow-report.json 应已生成");
    assert!(trace.exists(), "workflow-trace.json 应已生成");

    let md_content = std::fs::read_to_string(&md).unwrap();
    assert!(md_content.contains("Workflow 报告"));
    assert!(md_content.contains("API 调用顺序"));

    let trace_content = std::fs::read_to_string(&trace).unwrap();
    assert!(trace_content.contains("steps"));
}

#[tokio::test]
async fn replay_missing_fixtures_fails() {
    init();
    let dir = tempfile::tempdir().unwrap();
    let cfg = WorkflowConfig {
        mode: WorkflowMode::Replay,
        fixtures_dir: dir.path().to_str().unwrap().to_string(), // 空目录，无 fixtures
        report_dir: dir.path().to_str().unwrap().to_string(),
        ..WorkflowConfig::default()
    };

    let result = pm_api_workflow::run_replay(&cfg).await;
    assert!(result.is_err(), "fixtures 缺失时 Replay 应失败");
}
