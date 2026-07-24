//! Workflow 录制器（P2-02）。
//!
//! 自动记录每一步：开始时间 / 结束时间 / 耗时 / API 请求 / API 响应 / 失败原因。
//! 生成 Workflow Trace，供报告与校验使用。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use pm_api_test::client::http::ApiResponse;

use crate::state_machine::WorkflowState;

// ============================================================================
// ApiCallRecord
// ============================================================================

/// 一次 API 调用记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallRecord {
    /// HTTP 方法（GET / POST / DELETE / ...）。
    pub method: String,
    /// 请求路径。
    pub path: String,
    /// 请求体（JSON）。
    pub request_body: Option<Value>,
    /// HTTP 状态码（未发送时为 None）。
    pub status: Option<u16>,
    /// 响应摘要（截断的响应体）。
    pub response_summary: Option<String>,
    /// 耗时（毫秒）。
    pub latency_ms: u64,
    /// 是否为 DryRun（未真实发送）。
    pub dry_run: bool,
}

impl ApiCallRecord {
    /// 从真实 ApiResponse 构造。
    pub fn from_response(
        method: &str,
        path: &str,
        request_body: Option<&Value>,
        resp: &ApiResponse,
        dry_run: bool,
    ) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            request_body: request_body.cloned(),
            status: Some(resp.status),
            response_summary: Some(Self::summarize_body(&resp.body)),
            latency_ms: resp.latency_ms,
            dry_run,
        }
    }

    /// 构造本地 DryRun 调用（未发送，无状态码）。
    pub fn dry_run_local(method: &str, path: &str, request_body: Option<&Value>) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            request_body: request_body.cloned(),
            status: None,
            response_summary: Some("DryRun 未发送至交易所".to_string()),
            latency_ms: 0,
            dry_run: true,
        }
    }

    /// 是否为写操作（POST / DELETE / PUT / PATCH）。
    pub fn is_write(&self) -> bool {
        matches!(
            self.method.to_uppercase().as_str(),
            "POST" | "DELETE" | "PUT" | "PATCH"
        )
    }

    /// 截断响应体为摘要。
    fn summarize_body(body: &Value) -> String {
        let s = serde_json::to_string(body).unwrap_or_default();
        if s.len() > 200 {
            format!("{}...(已截断)", &s[..200])
        } else {
            s
        }
    }

    /// 中文单行摘要。
    pub fn summary_zh(&self) -> String {
        let dry = if self.dry_run { " [DryRun]" } else { "" };
        let status = self
            .status
            .map(|s| format!("HTTP {}", s))
            .unwrap_or_else(|| "未发送".to_string());
        format!(
            "{} {} -> {} | {}ms{}",
            self.method, self.path, status, self.latency_ms, dry
        )
    }
}

// ============================================================================
// StepRecord
// ============================================================================

/// 单步记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    /// 步骤状态。
    pub step: WorkflowState,
    /// 步骤中文名。
    pub step_zh: String,
    /// 开始时间。
    pub started_at: DateTime<Utc>,
    /// 结束时间。
    pub ended_at: Option<DateTime<Utc>>,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
    /// 该步骤产生的 API 调用（可能为 0，如本地构建步骤）。
    pub api_calls: Vec<ApiCallRecord>,
    /// 是否成功。
    pub success: bool,
    /// 失败原因。
    pub failure_reason: Option<String>,
    /// 备注。
    pub notes: Vec<String>,
}

impl StepRecord {
    /// 开始一个步骤。
    pub fn start(step: WorkflowState) -> Self {
        let started_at = Utc::now();
        tracing::info!(
            "【步骤开始】{}（{}）",
            step.as_zh(),
            started_at.format("%H:%M:%S%.3f")
        );
        Self {
            step_zh: step.as_zh().to_string(),
            step,
            started_at,
            ended_at: None,
            duration_ms: 0,
            api_calls: Vec::new(),
            success: true,
            failure_reason: None,
            notes: Vec::new(),
        }
    }

    /// 添加 API 调用记录。
    pub fn add_api_call(&mut self, call: ApiCallRecord) {
        tracing::info!("  [API] {}", call.summary_zh());
        self.api_calls.push(call);
    }

    /// 添加备注。
    pub fn add_note(&mut self, note: &str) {
        tracing::info!("  ℹ️ {}", note);
        self.notes.push(note.to_string());
    }

    /// 标记失败。
    pub fn fail(&mut self, reason: &str) {
        self.success = false;
        self.failure_reason = Some(reason.to_string());
        tracing::warn!("  ❌ 步骤失败: {}", reason);
    }

    /// 完成步骤（计算耗时）。
    pub fn finish(&mut self) {
        let ended_at = Utc::now();
        self.duration_ms = ended_at
            .signed_duration_since(self.started_at)
            .num_milliseconds()
            .max(0) as u64;
        self.ended_at = Some(ended_at);
        let icon = if self.success { "✅" } else { "❌" };
        tracing::info!(
            "【步骤结束】{} {} | 耗时: {}ms",
            self.step_zh,
            icon,
            self.duration_ms
        );
    }

    /// 是否包含写操作。
    pub fn has_write_call(&self) -> bool {
        self.api_calls.iter().any(|c| c.is_write())
    }
}

// ============================================================================
// WorkflowTrace
// ============================================================================

/// 完整 Workflow Trace。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTrace {
    /// 运行 ID。
    pub run_id: String,
    /// 模式（中文）。
    pub mode: String,
    /// 起始时间。
    pub started_at: DateTime<Utc>,
    /// 结束时间。
    pub ended_at: DateTime<Utc>,
    /// 总耗时（毫秒）。
    pub total_duration_ms: u64,
    /// 步骤记录。
    pub steps: Vec<StepRecord>,
}

// ============================================================================
// WorkflowRecorder
// ============================================================================

/// Workflow 录制器。
pub struct WorkflowRecorder {
    /// 运行 ID。
    run_id: String,
    /// 起始时间。
    started_at: DateTime<Utc>,
    /// 步骤记录。
    records: Vec<StepRecord>,
}

impl WorkflowRecorder {
    /// 创建新的录制器。
    pub fn new(run_id: &str) -> Self {
        let started_at = Utc::now();
        tracing::info!("【录制器】启动 Workflow 录制 | run_id={}", run_id);
        Self {
            run_id: run_id.to_string(),
            started_at,
            records: Vec::new(),
        }
    }

    /// 记录一个已完成步骤。
    pub fn record(&mut self, step: StepRecord) {
        self.records.push(step);
    }

    /// 步骤总数。
    pub fn step_count(&self) -> usize {
        self.records.len()
    }

    /// 成功步骤数。
    pub fn passed_count(&self) -> usize {
        self.records.iter().filter(|r| r.success).count()
    }

    /// 失败步骤数。
    pub fn failed_count(&self) -> usize {
        self.records.iter().filter(|r| !r.success).count()
    }

    /// 步骤记录引用。
    pub fn records(&self) -> &[StepRecord] {
        &self.records
    }

    /// 生成 Workflow Trace。
    pub fn trace(&self, mode: &str) -> WorkflowTrace {
        let ended_at = Utc::now();
        let total_duration_ms = ended_at
            .signed_duration_since(self.started_at)
            .num_milliseconds()
            .max(0) as u64;
        WorkflowTrace {
            run_id: self.run_id.clone(),
            mode: mode.to_string(),
            started_at: self.started_at,
            ended_at,
            total_duration_ms,
            steps: self.records.clone(),
        }
    }

    /// 所有 API 调用顺序（跨步骤）。
    pub fn api_sequence(&self) -> Vec<ApiCallRecord> {
        self.records
            .iter()
            .flat_map(|r| r.api_calls.iter().cloned())
            .collect()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_record_lifecycle() {
        let mut step = StepRecord::start(WorkflowState::LoadingMarket);
        step.add_note("开始加载市场");
        step.finish();
        assert!(step.success);
        assert!(step.duration_ms <= 1000); // 应该很快
    }

    #[test]
    fn step_record_failure() {
        let mut step = StepRecord::start(WorkflowState::CheckingBalance);
        step.fail("余额查询失败");
        step.finish();
        assert!(!step.success);
        assert_eq!(step.failure_reason.as_deref(), Some("余额查询失败"));
    }

    #[test]
    fn api_call_write_detection() {
        let get_call = ApiCallRecord::dry_run_local("GET", "/markets", None);
        let post_call = ApiCallRecord::dry_run_local("POST", "/order", None);
        assert!(!get_call.is_write());
        assert!(post_call.is_write());
    }

    #[test]
    fn recorder_collects_steps() {
        let mut rec = WorkflowRecorder::new("test-001");
        let mut s1 = StepRecord::start(WorkflowState::LoadingMarket);
        s1.finish();
        rec.record(s1);
        let mut s2 = StepRecord::start(WorkflowState::Completed);
        s2.finish();
        rec.record(s2);
        assert_eq!(rec.step_count(), 2);
        assert_eq!(rec.passed_count(), 2);
        assert_eq!(rec.failed_count(), 0);
    }

    #[test]
    fn trace_serializes() {
        let mut rec = WorkflowRecorder::new("test-002");
        let mut s = StepRecord::start(WorkflowState::Idle);
        s.finish();
        rec.record(s);
        let trace = rec.trace("DryRun（模拟）");
        let json = serde_json::to_string(&trace).unwrap();
        assert!(json.contains("run_id"));
        assert!(json.contains("steps"));
    }

    #[test]
    fn api_sequence_preserves_order() {
        let mut rec = WorkflowRecorder::new("test-003");
        let mut s1 = StepRecord::start(WorkflowState::LoadingMarket);
        s1.add_api_call(ApiCallRecord::dry_run_local("GET", "/markets", None));
        s1.finish();
        rec.record(s1);
        let mut s2 = StepRecord::start(WorkflowState::LoadingOrderBook);
        s2.add_api_call(ApiCallRecord::dry_run_local("GET", "/book", None));
        s2.finish();
        rec.record(s2);
        let seq = rec.api_sequence();
        assert_eq!(seq.len(), 2);
        assert_eq!(seq[0].path, "/markets");
        assert_eq!(seq[1].path, "/book");
    }
}
