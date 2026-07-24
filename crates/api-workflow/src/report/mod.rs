//! Workflow 报告模块（P2-02）。
//!
//! 生成 `reports/workflow/` 下的 Markdown / JSON / Trace 报告（全中文）。
//! 内容包括：Workflow 成功率、步骤耗时、失败步骤、API 调用顺序、平均耗时、整体健康评分。

pub mod generator;
pub mod types;

pub use generator::{GeneratedPaths, ReportGenerator};
pub use types::{StepSummary, WorkflowReport};
