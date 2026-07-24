//! Workflow 报告数据类型（P2-02）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::recorder::{ApiCallRecord, WorkflowTrace};
use crate::validator::ValidationReport;

// ============================================================================
// StepSummary
// ============================================================================

/// 步骤摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSummary {
    /// 步骤中文名。
    pub step_zh: String,
    /// 是否成功。
    pub success: bool,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// API 调用数。
    pub api_call_count: usize,
    /// 失败原因。
    pub failure_reason: Option<String>,
}

// ============================================================================
// WorkflowReport
// ============================================================================

/// 完整 Workflow 报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowReport {
    /// 运行 ID。
    pub run_id: String,
    /// 生成时间。
    pub timestamp: DateTime<Utc>,
    /// 模式（中文）。
    pub mode: String,
    /// 整体是否成功。
    pub success: bool,
    /// 步骤总数。
    pub total_steps: usize,
    /// 成功步骤数。
    pub passed_steps: usize,
    /// 失败步骤数。
    pub failed_steps: usize,
    /// 各步骤摘要。
    pub steps: Vec<StepSummary>,
    /// API 调用顺序。
    pub api_sequence: Vec<ApiCallRecord>,
    /// 平均步骤耗时（毫秒）。
    pub avg_duration_ms: f64,
    /// 总耗时（毫秒）。
    pub total_duration_ms: u64,
    /// 成功率（0.0~1.0）。
    pub success_rate: f64,
    /// 整体健康评分（0-100）。
    pub health_score: u32,
    /// 校验报告。
    pub validation: ValidationReport,
    /// 完整 Trace。
    pub trace: WorkflowTrace,
}

impl WorkflowReport {
    /// 从 Trace + 校验结果构建报告。
    pub fn from_trace(trace: WorkflowTrace, validation: ValidationReport) -> Self {
        let total_steps = trace.steps.len();
        let passed_steps = trace.steps.iter().filter(|r| r.success).count();
        let failed_steps = total_steps.saturating_sub(passed_steps);

        let steps: Vec<StepSummary> = trace
            .steps
            .iter()
            .map(|r| StepSummary {
                step_zh: r.step_zh.clone(),
                success: r.success,
                duration_ms: r.duration_ms,
                api_call_count: r.api_calls.len(),
                failure_reason: r.failure_reason.clone(),
            })
            .collect();

        let api_sequence: Vec<ApiCallRecord> = trace
            .steps
            .iter()
            .flat_map(|r| r.api_calls.iter().cloned())
            .collect();

        let avg_duration_ms = if total_steps == 0 {
            0.0
        } else {
            trace.steps.iter().map(|r| r.duration_ms).sum::<u64>() as f64 / total_steps as f64
        };

        let success_rate = if total_steps == 0 {
            0.0
        } else {
            passed_steps as f64 / total_steps as f64
        };

        let mode = trace.mode.clone();
        let success = validation.passed && failed_steps == 0;

        let health_score = Self::compute_health_score(
            success,
            failed_steps,
            &validation,
            avg_duration_ms,
        );

        Self {
            run_id: trace.run_id.clone(),
            timestamp: Utc::now(),
            mode,
            success,
            total_steps,
            passed_steps,
            failed_steps,
            steps,
            api_sequence,
            avg_duration_ms,
            total_duration_ms: trace.total_duration_ms,
            success_rate,
            health_score,
            validation,
            trace,
        }
    }

    /// 计算健康评分（0-100）。
    fn compute_health_score(
        success: bool,
        failed_steps: usize,
        validation: &ValidationReport,
        avg_duration_ms: f64,
    ) -> u32 {
        let mut score: i32 = 100;
        // 每个失败步骤扣 8 分
        score -= (failed_steps as i32) * 8;
        // 校验失败每条扣 15 分
        score -= (validation.failures.len() as i32) * 15;
        // 平均耗时过高扣分
        if avg_duration_ms > 5000.0 {
            score -= 10;
        }
        if !success {
            score -= 20;
        }
        score.max(0) as u32
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let icon = if self.success { "✅" } else { "❌" };
        format!(
            "{} Workflow 报告 | 模式: {} | 成功率: {:.0}% | 步骤: {}/{} | 平均耗时: {:.0}ms | 总耗时: {}ms | 健康评分: {}/100 | 校验: {}",
            icon,
            self.mode,
            self.success_rate * 100.0,
            self.passed_steps,
            self.total_steps,
            self.avg_duration_ms,
            self.total_duration_ms,
            self.health_score,
            if self.validation.passed { "通过" } else { "失败" },
        )
    }
}
