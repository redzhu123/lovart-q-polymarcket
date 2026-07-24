//! Replay Workflow（P2-02）。
//!
//! 从 `fixtures/` 读取 Mock 数据，完整回放交易生命周期，不访问网络。

use anyhow::Result;

use crate::config::{WorkflowConfig, WorkflowMode};
use crate::engine::WorkflowEngine;
use crate::report::types::WorkflowReport;

use super::{Workflow, verify_fixtures};

/// Replay Workflow。
pub struct ReplayWorkflow {
    cfg: WorkflowConfig,
}

impl ReplayWorkflow {
    /// 创建 Replay Workflow。
    pub fn new(cfg: WorkflowConfig) -> Self {
        Self { cfg }
    }
}

impl Workflow for ReplayWorkflow {
    fn mode(&self) -> WorkflowMode {
        WorkflowMode::Replay
    }

    async fn run(&self, engine: &mut WorkflowEngine) -> Result<WorkflowReport> {
        tracing::info!("╔══════════════════════════════════════════════════════════╗");
        tracing::info!("║  Replay Workflow -- 从 fixtures/ 回放，禁止访问网络");
        tracing::info!("╚══════════════════════════════════════════════════════════╝");

        // 校验 fixtures 完整性（不访问网络，仅本地文件检查）
        verify_fixtures(&self.cfg.fixtures_dir)?;
        tracing::info!(dir = %self.cfg.fixtures_dir, "fixtures Mock 数据已就绪，开始回放");

        let report = engine.run_full_lifecycle().await;
        Ok(report)
    }
}
