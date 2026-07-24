//! DryRun Workflow（P2-02）。
//!
//! 默认模式：Mock 客户端，完整交易生命周期，提交订单步骤仅构建+校验，不发送。

use anyhow::Result;

use crate::config::{WorkflowConfig, WorkflowMode};
use crate::engine::WorkflowEngine;
use crate::report::types::WorkflowReport;

use super::Workflow;

/// DryRun Workflow。
pub struct DryRunWorkflow {
    #[allow(dead_code)]
    cfg: WorkflowConfig,
}

impl DryRunWorkflow {
    /// 创建 DryRun Workflow。
    pub fn new(cfg: WorkflowConfig) -> Self {
        Self { cfg }
    }
}

impl Workflow for DryRunWorkflow {
    fn mode(&self) -> WorkflowMode {
        WorkflowMode::DryRun
    }

    async fn run(&self, engine: &mut WorkflowEngine) -> Result<WorkflowReport> {
        tracing::info!("╔══════════════════════════════════════════════════════════╗");
        tracing::info!("║  DryRun Workflow -- 禁止真实下单");
        tracing::info!("╚══════════════════════════════════════════════════════════╝");
        let report = engine.run_full_lifecycle().await;
        Ok(report)
    }
}
