//! pm-audit：数据链路审计（V1.09）。
//!
//! 本 crate 负责：
//! - [`trace_id`]：统一追踪标识符（TraceId），跨生命周期各阶段共享。
//! - [`lifecycle`]：生命周期阶段定义（Market → Candidate → Opportunity → ... → Report）。
//! - [`rejection`]：统一拒绝原因模型（CandidateRejection）。
//! - [`registry`]：通用内存注册表（DataRegistry）。
//! - [`counter`]：拒绝原因计数器（RejectionCounter）。
//! - [`auditor`]：统计审计器（StatisticsAudit），校验数据一致性。
//! - [`reporter`]：报告生成器（ExplainReport / AuditReport）。
//!
//! 设计原则：
//! - 所有统计必须可追踪来源（从 Registry / Repository / CSV 查询）。
//! - 不得使用简单 `++count` 累加器。
//! - 任何对象必须经过完整生命周期，禁止跨阶段创建。
//! - 所有拒绝必须记录原因。
//!
//! Simulation Only -- 不修改策略 / 套利算法 / 市场扫描逻辑。

pub mod auditor;
pub mod counter;
pub mod lifecycle;
pub mod registry;
pub mod rejection;
pub mod reporter;
pub mod trace_id;

// 重导出最常用类型
pub use auditor::{AuditFinding, AuditSeverity, AuditStats, StatisticsAudit};
pub use counter::RejectionCounter;
pub use lifecycle::LifecycleStage;
pub use registry::{DataRegistry, Identifiable};
pub use rejection::CandidateRejection;
pub use reporter::{AuditReport, ExplainReport};
pub use trace_id::TraceId;

/// 日志辅助：记录对象生命周期事件（中文）。
///
/// 使用 `tracing::info!` 输出结构化日志，统一格式为：
/// ```text
/// [TraceId] 生命周期阶段 | 市场: MarketId | 耗时: Xms | 拒绝原因: Reason
/// ```
pub fn log_lifecycle(
    trace_id: &TraceId,
    market_id: &str,
    stage: LifecycleStage,
    duration_ms: u128,
    rejection_reason: Option<CandidateRejection>,
) {
    let reason_str = rejection_reason
        .map(|r| format!(" | 拒绝原因: {}", r.as_zh()))
        .unwrap_or_default();
    tracing::info!(
        target: "lifecycle",
        "[{}] {} | 市场: {} | 耗时: {}ms{}",
        trace_id,
        stage.as_zh(),
        market_id,
        duration_ms,
        reason_str
    );
}
