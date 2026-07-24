//! Live ReadOnly Workflow（P2-02）。
//!
//! 真实接口，仅允许读取：Markets / OrderBook / Balance（如已认证）/ Position（如已认证）。
//! 禁止 Place Order / Cancel Order。
//!
//! `enable_live_reads=false`（默认）时使用 Mock 客户端离线校验只读约束；
//! `enable_live_reads=true` 时使用真实网络（需代理 / 认证）。

use anyhow::Result;

use crate::config::{WorkflowConfig, WorkflowMode};
use crate::engine::WorkflowEngine;
use crate::report::types::WorkflowReport;

use super::Workflow;

/// Live ReadOnly Workflow。
pub struct LiveReadOnlyWorkflow {
    cfg: WorkflowConfig,
}

impl LiveReadOnlyWorkflow {
    /// 创建 Live ReadOnly Workflow。
    pub fn new(cfg: WorkflowConfig) -> Self {
        Self { cfg }
    }
}

impl Workflow for LiveReadOnlyWorkflow {
    fn mode(&self) -> WorkflowMode {
        WorkflowMode::LiveReadOnly
    }

    async fn run(&self, engine: &mut WorkflowEngine) -> Result<WorkflowReport> {
        tracing::info!("╔══════════════════════════════════════════════════════════╗");
        tracing::info!("║  Live ReadOnly Workflow -- 仅读取，禁止下单/撤单");
        tracing::info!("╚══════════════════════════════════════════════════════════╝");

        if self.cfg.enable_live_reads {
            tracing::warn!("⚠️ 真实读取已启用（仅 GET）：Markets / OrderBook / Balance / Position");
        } else {
            tracing::info!("enable_live_reads=false -> 使用 Mock 客户端离线校验只读约束");
        }
        tracing::info!("🔒 禁止：Place Order（POST /order）");
        tracing::info!("🔒 禁止：Cancel Order（DELETE /order）");

        let report = engine.run_readonly_lifecycle().await;
        Ok(report)
    }
}
