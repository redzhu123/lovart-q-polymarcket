//! Workflow 校验器（P2-02）。
//!
//! 检查 Workflow 是否完整：
//! - Order 提交后必须查询 Order Status。
//! - Order Filled 后必须同步 Position。
//! - Position 更新后必须同步 Balance。
//! - 任何一步遗漏 -> Workflow Fail。
//!
//! LiveReadOnly：禁止任何写操作（Place / Cancel Order）。

use serde::{Deserialize, Serialize};

use crate::config::WorkflowMode;
use crate::recorder::WorkflowTrace;
use crate::state_machine::WorkflowState;

// ============================================================================
// RuleResult / ValidationReport
// ============================================================================

/// 单条校验规则结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    /// 规则名称（中文）。
    pub rule: String,
    /// 是否通过。
    pub passed: bool,
    /// 详情（中文）。
    pub detail: String,
}

impl RuleResult {
    pub fn pass(rule: &str, detail: &str) -> Self {
        Self {
            rule: rule.to_string(),
            passed: true,
            detail: detail.to_string(),
        }
    }

    pub fn fail(rule: &str, detail: &str) -> Self {
        Self {
            rule: rule.to_string(),
            passed: false,
            detail: detail.to_string(),
        }
    }
}

/// 校验报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// 整体是否通过。
    pub passed: bool,
    /// 各规则结果。
    pub rules: Vec<RuleResult>,
    /// 失败原因列表。
    pub failures: Vec<String>,
}

impl ValidationReport {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let icon = if self.passed { "✅" } else { "❌" };
        format!(
            "{} Workflow 校验: {}/{} 规则通过 | 失败 {} 条",
            icon,
            self.rules.iter().filter(|r| r.passed).count(),
            self.rules.len(),
            self.failures.len()
        )
    }

    /// 详细报告（中文）。
    pub fn detailed_zh(&self) -> String {
        let mut out = String::new();
        out.push_str("═══════════════════════════════════════════════════════════\n");
        out.push_str("【Workflow 校验报告】\n");
        out.push_str("───────────────────────────────────────────────────────────\n");
        for r in &self.rules {
            let icon = if r.passed { "✅" } else { "❌" };
            out.push_str(&format!("  {} {} - {}\n", icon, r.rule, r.detail));
        }
        if !self.failures.is_empty() {
            out.push_str("───────────────────────────────────────────────────────────\n");
            out.push_str("失败原因:\n");
            for f in &self.failures {
                out.push_str(&format!("  ❌ {}\n", f));
            }
        }
        out.push_str(&format!(
            "───────────────────────────────────────────────────────────\n"
        ));
        out.push_str(&format!(
            "结果: {}\n",
            if self.passed {
                "✅ 通过"
            } else {
                "❌ 失败"
            }
        ));
        out
    }
}

// ============================================================================
// WorkflowValidator
// ============================================================================

/// Workflow 校验器。
pub struct WorkflowValidator;

impl WorkflowValidator {
    /// 校验一条 Workflow Trace。
    pub fn validate(trace: &WorkflowTrace, mode: WorkflowMode) -> ValidationReport {
        match mode {
            WorkflowMode::DryRun | WorkflowMode::Replay => Self::validate_full(trace),
            WorkflowMode::LiveReadOnly => Self::validate_readonly(trace),
        }
    }

    /// 全生命周期校验（DryRun / Replay）。
    fn validate_full(trace: &WorkflowTrace) -> ValidationReport {
        let mut rules = Vec::new();
        let mut failures = Vec::new();

        let visited = |s: WorkflowState| -> Option<usize> {
            trace.steps.iter().position(|r| r.step == s && r.success)
        };

        // R1: 到达 Completed
        let reached_completed = trace
            .steps
            .iter()
            .any(|r| r.step == WorkflowState::Completed && r.success);
        let has_failed = trace.steps.iter().any(|r| r.step == WorkflowState::Failed);
        if reached_completed && !has_failed {
            rules.push(RuleResult::pass("到达终态", "Workflow 正确到达 已完成"));
        } else {
            let detail = if has_failed {
                "Workflow 进入 已失败 终态".to_string()
            } else {
                "未到达 已完成 终态".to_string()
            };
            failures.push(detail.clone());
            rules.push(RuleResult::fail("到达终态", &detail));
        }

        // R2: 包含 SubmittingOrder（DryRun 下单）
        match visited(WorkflowState::SubmittingOrder) {
            Some(_) => rules.push(RuleResult::pass("提交订单", "包含 提交订单(DryRun) 步骤")),
            None => {
                let d = "缺少 提交订单(DryRun) 步骤".to_string();
                failures.push(d.clone());
                rules.push(RuleResult::fail("提交订单", &d));
            }
        }

        // R3: 提交后必须查询订单状态（SyncOrder 在 SubmittingOrder 之后）
        let r3 = match (
            visited(WorkflowState::SubmittingOrder),
            visited(WorkflowState::SyncOrder),
        ) {
            (Some(i), Some(j)) if j > i => {
                RuleResult::pass("提交后查询订单状态", "提交订单后已查询订单状态（同步订单）")
            }
            _ => {
                let d = "订单提交后未查询订单状态（缺少同步订单）".to_string();
                failures.push(d.clone());
                RuleResult::fail("提交后查询订单状态", &d)
            }
        };
        rules.push(r3);

        // R4: 成交后必须同步持仓（SyncPosition 在 SyncOrder 之后）
        let r4 = match (
            visited(WorkflowState::SyncOrder),
            visited(WorkflowState::SyncPosition),
        ) {
            (Some(i), Some(j)) if j > i => {
                RuleResult::pass("成交后同步持仓", "订单成交后已同步持仓（同步持仓）")
            }
            _ => {
                let d = "订单成交后未同步持仓（缺少同步持仓）".to_string();
                failures.push(d.clone());
                RuleResult::fail("成交后同步持仓", &d)
            }
        };
        rules.push(r4);

        // R5: 持仓更新后必须同步余额（SyncBalance 在 SyncPosition 之后）
        let r5 = match (
            visited(WorkflowState::SyncPosition),
            visited(WorkflowState::SyncBalance),
        ) {
            (Some(i), Some(j)) if j > i => {
                RuleResult::pass("持仓后同步余额", "持仓更新后已同步余额（同步余额）")
            }
            _ => {
                let d = "持仓更新后未同步余额（缺少同步余额）".to_string();
                failures.push(d.clone());
                RuleResult::fail("持仓后同步余额", &d)
            }
        };
        rules.push(r5);

        // R6: 无真实写操作发送（所有写 api_call 必须 dry_run）
        let real_writes: Vec<String> = trace
            .steps
            .iter()
            .flat_map(|r| r.api_calls.iter().map(move |c| (r.step, c)))
            .filter(|(_, c)| c.is_write() && !c.dry_run)
            .map(|(s, c)| format!("{}: {} {}", s.as_zh(), c.method, c.path))
            .collect();
        if real_writes.is_empty() {
            rules.push(RuleResult::pass(
                "无真实写操作",
                "所有写操作均为 DryRun，未真实发送",
            ));
        } else {
            let d = format!("检测到真实写操作: {}", real_writes.join("; "));
            failures.push(d.clone());
            rules.push(RuleResult::fail("无真实写操作", &d));
        }

        let passed = failures.is_empty();
        ValidationReport {
            passed,
            rules,
            failures,
        }
    }

    /// 只读校验（LiveReadOnly）。
    fn validate_readonly(trace: &WorkflowTrace) -> ValidationReport {
        let mut rules = Vec::new();
        let mut failures = Vec::new();

        let visited =
            |s: WorkflowState| -> bool { trace.steps.iter().any(|r| r.step == s && r.success) };

        // R1: 到达 Completed
        let reached_completed = trace
            .steps
            .iter()
            .any(|r| r.step == WorkflowState::Completed && r.success);
        if reached_completed {
            rules.push(RuleResult::pass(
                "到达终态",
                "只读 Workflow 正确到达 已完成",
            ));
        } else {
            let d = "只读 Workflow 未到达 已完成 终态".to_string();
            failures.push(d.clone());
            rules.push(RuleResult::fail("到达终态", &d));
        }

        // R2: 禁止任何写操作（POST/DELETE/PUT/PATCH）
        let writes: Vec<String> = trace
            .steps
            .iter()
            .flat_map(|r| r.api_calls.iter().map(move |c| (r.step, c)))
            .filter(|(_, c)| c.is_write())
            .map(|(s, c)| format!("{}: {} {}", s.as_zh(), c.method, c.path))
            .collect();
        if writes.is_empty() {
            rules.push(RuleResult::pass(
                "无写操作",
                "未执行任何 Place/Cancel Order 写操作",
            ));
        } else {
            let d = format!("检测到禁止的写操作: {}", writes.join("; "));
            failures.push(d.clone());
            rules.push(RuleResult::fail("无写操作", &d));
        }

        // R3: 必须包含市场读取
        if visited(WorkflowState::LoadingMarket) {
            rules.push(RuleResult::pass("读取市场", "已读取 Markets"));
        } else {
            let d = "未读取 Markets".to_string();
            failures.push(d.clone());
            rules.push(RuleResult::fail("读取市场", &d));
        }

        // R4: 必须包含订单簿读取
        if visited(WorkflowState::LoadingOrderBook) {
            rules.push(RuleResult::pass("读取订单簿", "已读取 OrderBook"));
        } else {
            let d = "未读取 OrderBook".to_string();
            failures.push(d.clone());
            rules.push(RuleResult::fail("读取订单簿", &d));
        }

        // R5: 禁止下单相关状态
        let write_states: Vec<&str> = trace
            .steps
            .iter()
            .filter(|r| r.step.is_write_state())
            .map(|r| r.step.as_zh())
            .collect();
        if write_states.is_empty() {
            rules.push(RuleResult::pass(
                "无下单步骤",
                "未出现构建/提交/等待/同步订单等下单步骤",
            ));
        } else {
            let d = format!("出现禁止的下单步骤: {}", write_states.join("; "));
            failures.push(d.clone());
            rules.push(RuleResult::fail("无下单步骤", &d));
        }

        let passed = failures.is_empty();
        ValidationReport {
            passed,
            rules,
            failures,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::{ApiCallRecord, StepRecord, WorkflowRecorder};

    fn build_trace(steps: Vec<WorkflowState>) -> WorkflowTrace {
        let mut rec = WorkflowRecorder::new("test");
        for s in steps {
            let mut r = StepRecord::start(s);
            if s == WorkflowState::LoadingMarket {
                r.add_api_call(ApiCallRecord::dry_run_local("GET", "/markets", None));
            }
            r.finish();
            rec.record(r);
        }
        rec.trace("DryRun（模拟）")
    }

    #[test]
    fn full_lifecycle_passes() {
        let trace = build_trace(vec![
            WorkflowState::LoadingMarket,
            WorkflowState::LoadingOrderBook,
            WorkflowState::CheckingBalance,
            WorkflowState::BuildingOrder,
            WorkflowState::SubmittingOrder,
            WorkflowState::WaitingResult,
            WorkflowState::SyncOrder,
            WorkflowState::SyncTrade,
            WorkflowState::SyncPosition,
            WorkflowState::SyncBalance,
            WorkflowState::Completed,
        ]);
        let report = WorkflowValidator::validate(&trace, WorkflowMode::DryRun);
        assert!(
            report.passed,
            "完整生命周期应通过: {}",
            report.detailed_zh()
        );
    }

    #[test]
    fn missing_sync_position_fails() {
        let trace = build_trace(vec![
            WorkflowState::LoadingMarket,
            WorkflowState::SubmittingOrder,
            WorkflowState::SyncOrder,
            // 缺少 SyncPosition
            WorkflowState::SyncBalance,
            WorkflowState::Completed,
        ]);
        let report = WorkflowValidator::validate(&trace, WorkflowMode::DryRun);
        assert!(!report.passed);
        assert!(report.failures.iter().any(|f| f.contains("同步持仓")));
    }

    #[test]
    fn missing_sync_order_fails() {
        let trace = build_trace(vec![
            WorkflowState::SubmittingOrder,
            WorkflowState::SyncPosition,
            WorkflowState::SyncBalance,
            WorkflowState::Completed,
        ]);
        let report = WorkflowValidator::validate(&trace, WorkflowMode::DryRun);
        assert!(!report.passed);
        assert!(report.failures.iter().any(|f| f.contains("订单状态")));
    }

    #[test]
    fn real_write_detected() {
        let mut rec = WorkflowRecorder::new("test");
        let mut r = StepRecord::start(WorkflowState::SubmittingOrder);
        // 真实 POST（非 DryRun）
        r.add_api_call(ApiCallRecord {
            method: "POST".into(),
            path: "/order".into(),
            request_body: None,
            status: Some(200),
            response_summary: None,
            latency_ms: 50,
            dry_run: false,
        });
        r.finish();
        rec.record(r);
        let trace = rec.trace("DryRun（模拟）");
        let report = WorkflowValidator::validate(&trace, WorkflowMode::DryRun);
        assert!(!report.passed);
        assert!(report.failures.iter().any(|f| f.contains("真实写操作")));
    }

    #[test]
    fn readonly_rejects_writes() {
        let mut rec = WorkflowRecorder::new("test");
        let mut r = StepRecord::start(WorkflowState::LoadingMarket);
        r.add_api_call(ApiCallRecord::dry_run_local("GET", "/markets", None));
        r.finish();
        rec.record(r);
        let mut r2 = StepRecord::start(WorkflowState::LoadingOrderBook);
        r2.add_api_call(ApiCallRecord {
            method: "DELETE".into(),
            path: "/order".into(),
            request_body: None,
            status: Some(200),
            response_summary: None,
            latency_ms: 10,
            dry_run: true,
        });
        r2.finish();
        rec.record(r2);
        let mut r3 = StepRecord::start(WorkflowState::Completed);
        r3.finish();
        rec.record(r3);
        let trace = rec.trace("Live ReadOnly（真实只读）");
        let report = WorkflowValidator::validate(&trace, WorkflowMode::LiveReadOnly);
        assert!(!report.passed);
    }

    #[test]
    fn readonly_passes_with_reads_only() {
        let mut rec = WorkflowRecorder::new("test");
        for s in [
            WorkflowState::LoadingMarket,
            WorkflowState::LoadingOrderBook,
            WorkflowState::CheckingBalance,
            WorkflowState::SyncPosition,
            WorkflowState::SyncBalance,
            WorkflowState::Completed,
        ] {
            let mut r = StepRecord::start(s);
            r.add_api_call(ApiCallRecord::dry_run_local("GET", "/markets", None));
            r.finish();
            rec.record(r);
        }
        let trace = rec.trace("Live ReadOnly（真实只读）");
        let report = WorkflowValidator::validate(&trace, WorkflowMode::LiveReadOnly);
        assert!(report.passed, "只读路径应通过: {}", report.detailed_zh());
    }
}
