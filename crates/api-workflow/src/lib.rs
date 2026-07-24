//! pm-api-workflow：API Workflow Engine（P2-02）。
//!
//! 验证所有 Polymarket API 能组成完整交易生命周期（Workflow）。
//!
//! # 职责边界
//!
//! - **仅依赖** 已认证的 `pm-api-test`（ApiClient / ResponseValidator / LiveGuard）。
//! - **不依赖** Strategy / Risk / Gateway / Execution。
//! - 只负责 API 调用流程（状态机 + 录制 + 校验 + 报告），不开发真实交易策略。
//!
//! # 安全
//!
//! - 默认 DryRun：所有 Workflow 使用 DryRun，禁止真实下单。
//! - Replay：从 `fixtures/` 读取 Mock 数据，完整回放，不访问网络。
//! - Live ReadOnly：真实接口，但仅允许读取（Markets / OrderBook / Balance / Position），
//!   禁止 Place Order / Cancel Order。
//! - 所有日志使用中文（tracing），禁止 println!。
//!
//! # 快速开始
//!
//! ```ignore
//! use pm_api_workflow::prelude::*;
//!
//! let cfg = WorkflowConfig::load_or_default("workflow.toml");
//! let engine = WorkflowEngine::new(cfg);
//! let report = engine.run_dryrun().await?;
//! println!("{}", report.summary_zh());
//! ```

pub mod config;
pub mod engine;
pub mod recorder;
pub mod report;
pub mod state_machine;
pub mod validator;
pub mod workflows;

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::config::{WorkflowConfig, WorkflowMode};
    pub use crate::engine::WorkflowEngine;
    pub use crate::recorder::{
        ApiCallRecord, StepRecord, WorkflowRecorder, WorkflowTrace,
    };
    pub use crate::report::{generator::ReportGenerator, types::WorkflowReport};
    pub use crate::state_machine::{StateMachine, WorkflowState};
    pub use crate::validator::{ValidationReport, WorkflowValidator};
    pub use crate::workflows::{dryrun::DryRunWorkflow, live::LiveReadOnlyWorkflow,
        replay::ReplayWorkflow, Workflow};
}

// 便利函数所用的内部类型导入。
use crate::config::WorkflowConfig;
use crate::engine::WorkflowEngine;
use crate::report::generator::ReportGenerator;
use crate::workflows::Workflow;
use crate::workflows::{dryrun::DryRunWorkflow, live::LiveReadOnlyWorkflow, replay::ReplayWorkflow};

// ============================================================================
// 便利函数（CLI / 测试使用）
// ============================================================================

/// 初始化中文 tracing 日志（与 pm-api-test 一致的风格）。
pub fn init_logging(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_env("PM_WORKFLOW_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

/// 运行 DryRun Workflow（默认模式，无网络、无真实下单）并生成报告。
pub async fn run_dryrun(cfg: &WorkflowConfig) -> anyhow::Result<crate::report::types::WorkflowReport> {
    let mut engine = WorkflowEngine::new(cfg.clone());
    let workflow = DryRunWorkflow::new(cfg.clone());
    let report = workflow.run(&mut engine).await?;
    let generator = ReportGenerator::new(&cfg.report_dir);
    generator.generate(&report)?;
    Ok(report)
}

/// 运行 Replay Workflow（从 fixtures/ 回放，不访问网络）并生成报告。
pub async fn run_replay(cfg: &WorkflowConfig) -> anyhow::Result<crate::report::types::WorkflowReport> {
    let mut engine = WorkflowEngine::new(cfg.clone());
    let workflow = ReplayWorkflow::new(cfg.clone());
    let report = workflow.run(&mut engine).await?;
    let generator = ReportGenerator::new(&cfg.report_dir);
    generator.generate(&report)?;
    Ok(report)
}

/// 运行 Live ReadOnly Workflow（真实接口，仅读取，禁止下单/撤单）并生成报告。
pub async fn run_live_readonly(
    cfg: &WorkflowConfig,
) -> anyhow::Result<crate::report::types::WorkflowReport> {
    let mut engine = WorkflowEngine::new(cfg.clone());
    let workflow = LiveReadOnlyWorkflow::new(cfg.clone());
    let report = workflow.run(&mut engine).await?;
    let generator = ReportGenerator::new(&cfg.report_dir);
    generator.generate(&report)?;
    Ok(report)
}
