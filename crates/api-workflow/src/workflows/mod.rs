//! Workflow 实现（P2-02）。
//!
//! 三种 Workflow：
//! - [`dryrun::DryRunWorkflow`]：默认，Mock，全生命周期，DryRun 下单。
//! - [`replay::ReplayWorkflow`]：从 fixtures/ 确定性回放，不访问网络。
//! - [`live::LiveReadOnlyWorkflow`]：真实接口只读，禁止下单/撤单。

pub mod dryrun;
pub mod live;
pub mod replay;

use anyhow::Result;

use crate::config::WorkflowMode;
use crate::engine::WorkflowEngine;
use crate::report::types::WorkflowReport;

/// Workflow 统一接口。
///
/// 使用 `async fn`（trait 仅在本 crate 内以具体类型调用，运行时为 current_thread，
/// 无需 Send 约束），故允许 `async_fn_in_trait`。
#[allow(async_fn_in_trait)]
pub trait Workflow: Send + Sync {
    /// 运行模式。
    fn mode(&self) -> WorkflowMode;

    /// 执行 Workflow，返回报告。
    async fn run(&self, engine: &mut WorkflowEngine) -> Result<WorkflowReport>;
}

/// fixtures 目录下必须存在的 Mock 数据文件（与 pm-api-test 共享，禁止重复）。
pub const FIXTURE_FILES: &[&str] = &[
    "markets",
    "market-detail",
    "orderbook",
    "trades",
    "balance",
    "orders",
    "positions",
    "server-time",
];

/// 校验 fixtures 目录完整性。
pub fn verify_fixtures(fixtures_dir: &str) -> Result<()> {
    let mut missing: Vec<String> = Vec::new();
    for name in FIXTURE_FILES {
        let path = format!("{}/{}.json", fixtures_dir, name);
        if !std::path::Path::new(&path).exists() {
            missing.push(path);
        }
    }
    if missing.is_empty() {
        tracing::info!(dir = %fixtures_dir, count = %FIXTURE_FILES.len(), "fixtures 目录校验通过");
        Ok(())
    } else {
        let msg = format!("fixtures 目录缺少文件: {}", missing.join(", "));
        tracing::error!("{}", msg);
        anyhow::bail!(msg)
    }
}
