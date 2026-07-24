//! Workflow 报告生成器（P2-02）。
//!
//! 生成 `reports/workflow/` 下的三种报告（全中文）：
//! - `workflow-report.md`
//! - `workflow-report.json`
//! - `workflow-trace.json`

use std::fs;
use std::path::Path;

use anyhow::Result;

use super::types::WorkflowReport;

/// 生成的文件路径。
#[derive(Debug, Clone)]
pub struct GeneratedPaths {
    pub md_path: String,
    pub json_path: String,
    pub trace_path: String,
}

/// 报告生成器。
pub struct ReportGenerator {
    output_dir: String,
}

impl ReportGenerator {
    /// 创建新的报告生成器（自动创建输出目录）。
    pub fn new(output_dir: &str) -> Self {
        if let Err(e) = fs::create_dir_all(output_dir) {
            tracing::warn!(dir = %output_dir, error = %e, "创建报告目录失败");
        }
        Self {
            output_dir: output_dir.to_string(),
        }
    }

    /// 生成全部报告。
    pub fn generate(&self, report: &WorkflowReport) -> Result<GeneratedPaths> {
        let md_path = format!("{}/workflow-report.md", self.output_dir);
        let json_path = format!("{}/workflow-report.json", self.output_dir);
        let trace_path = format!("{}/workflow-trace.json", self.output_dir);

        self.generate_markdown(report, &md_path)?;
        self.generate_json(report, &json_path)?;
        self.generate_trace(report, &trace_path)?;

        tracing::info!("【报告已生成】");
        tracing::info!("  Markdown: {}", md_path);
        tracing::info!("  JSON:     {}", json_path);
        tracing::info!("  Trace:    {}", trace_path);

        Ok(GeneratedPaths {
            md_path,
            json_path,
            trace_path,
        })
    }

    /// 生成 Markdown 报告（全中文）。
    fn generate_markdown(&self, report: &WorkflowReport, path: &str) -> Result<()> {
        let mut md = String::new();

        let icon = if report.success { "✅ 通过" } else { "❌ 失败" };

        md.push_str("# Polymarket API Workflow 报告\n\n");
        md.push_str(&format!(
            "**模式**: {}  \n\
             **运行 ID**: {}  \n\
             **生成时间**: {}  \n\
             **整体结果**: {}\n\n",
            report.mode,
            report.run_id,
            report.timestamp.format("%Y-%m-%d %H:%M:%S"),
            icon,
        ));

        // 健康评分
        md.push_str("## 整体健康评分\n\n");
        md.push_str(&format!("**{}/100**\n\n", report.health_score));

        // 摘要
        md.push_str("## 摘要\n\n");
        md.push_str("| 指标 | 值 |\n|------|----|\n");
        md.push_str(&format!("| 步骤总数 | {} |\n", report.total_steps));
        md.push_str(&format!("| 成功步骤 | {} |\n", report.passed_steps));
        md.push_str(&format!("| 失败步骤 | {} |\n", report.failed_steps));
        md.push_str(&format!("| 成功率 | {:.0}% |\n", report.success_rate * 100.0));
        md.push_str(&format!("| 平均步骤耗时 | {:.0}ms |\n", report.avg_duration_ms));
        md.push_str(&format!("| 总耗时 | {}ms |\n", report.total_duration_ms));
        md.push_str(&format!("| 健康评分 | {}/100 |\n\n", report.health_score));

        // 步骤明细
        md.push_str("## 步骤耗时明细\n\n");
        md.push_str("| 步骤 | 结果 | 耗时 | API调用数 | 失败原因 |\n");
        md.push_str("|------|------|------|----------|----------|\n");
        for s in &report.steps {
            let result = if s.success { "✅" } else { "❌" };
            let reason = s.failure_reason.clone().unwrap_or_default();
            md.push_str(&format!(
                "| {} | {} | {}ms | {} | {} |\n",
                s.step_zh, result, s.duration_ms, s.api_call_count, reason
            ));
        }
        md.push('\n');

        // API 调用顺序
        md.push_str("## API 调用顺序\n\n");
        if report.api_sequence.is_empty() {
            md.push_str("（无 API 调用）\n\n");
        } else {
            md.push_str("| # | 方法 | 路径 | 状态 | 耗时 | DryRun |\n");
            md.push_str("|---|------|------|------|------|--------|\n");
            for (i, c) in report.api_sequence.iter().enumerate() {
                let status = c
                    .status
                    .map(|s| format!("{}", s))
                    .unwrap_or_else(|| "未发送".to_string());
                let dry = if c.dry_run { "是" } else { "否" };
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {}ms | {} |\n",
                    i + 1,
                    c.method,
                    c.path,
                    status,
                    c.latency_ms,
                    dry
                ));
            }
            md.push('\n');
        }

        // 失败步骤
        let failed: Vec<_> = report.steps.iter().filter(|s| !s.success).collect();
        if !failed.is_empty() {
            md.push_str("## 失败步骤\n\n");
            for s in failed {
                md.push_str(&format!(
                    "- **{}**: {}\n",
                    s.step_zh,
                    s.failure_reason.clone().unwrap_or_default()
                ));
            }
            md.push('\n');
        }

        // 校验结果
        md.push_str("## Workflow 校验\n\n");
        md.push_str(&format!(
            "整体: {}\n\n",
            if report.validation.passed { "✅ 通过" } else { "❌ 失败" }
        ));
        md.push_str("| 规则 | 结果 | 详情 |\n|------|------|------|\n");
        for r in &report.validation.rules {
            let result = if r.passed { "✅" } else { "❌" };
            md.push_str(&format!("| {} | {} | {} |\n", r.rule, result, r.detail));
        }
        if !report.validation.failures.is_empty() {
            md.push_str("\n### 失败原因\n\n");
            for f in &report.validation.failures {
                md.push_str(&format!("- {}\n", f));
            }
        }
        md.push('\n');

        md.push_str("---\n\n*由 pm-api-workflow 自动生成*\n");

        fs::write(path, md)?;
        tracing::info!("Markdown 报告已写入: {}", path);
        Ok(())
    }

    /// 生成 JSON 报告（含 trace + 校验）。
    fn generate_json(&self, report: &WorkflowReport, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        fs::write(path, json)?;
        tracing::info!("JSON 报告已写入: {}", path);
        Ok(())
    }

    /// 生成 Trace JSON（仅步骤轨迹）。
    fn generate_trace(&self, report: &WorkflowReport, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(&report.trace)?;
        fs::write(path, json)?;
        tracing::info!("Trace 已写入: {}", path);
        Ok(())
    }

    /// 读取最近一次报告路径（用于 CLI 展示）。
    pub fn latest_paths(&self) -> GeneratedPaths {
        GeneratedPaths {
            md_path: format!("{}/workflow-report.md", self.output_dir),
            json_path: format!("{}/workflow-report.json", self.output_dir),
            trace_path: format!("{}/workflow-trace.json", self.output_dir),
        }
    }

    /// 报告是否已存在。
    pub fn report_exists(&self) -> bool {
        Path::new(&format!("{}/workflow-report.md", self.output_dir)).exists()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::{ApiCallRecord, StepRecord, WorkflowRecorder};
    use crate::state_machine::WorkflowState;
    use crate::validator::WorkflowValidator;
    use crate::config::WorkflowMode;

    fn sample_report() -> WorkflowReport {
        let mut rec = WorkflowRecorder::new("unit-test");
        for s in [
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
        ] {
            let mut r = StepRecord::start(s);
            if s == WorkflowState::LoadingMarket {
                r.add_api_call(ApiCallRecord::dry_run_local("GET", "/markets", None));
            }
            r.finish();
            rec.record(r);
        }
        let trace = rec.trace("DryRun（模拟）");
        let validation = WorkflowValidator::validate(&trace, WorkflowMode::DryRun);
        WorkflowReport::from_trace(trace, validation)
    }

    #[test]
    fn generate_all_formats() {
        let report = sample_report();
        let dir = tempfile::tempdir().unwrap();
        let generator = ReportGenerator::new(dir.path().to_str().unwrap());
        let paths = generator.generate(&report).unwrap();

        assert!(Path::new(&paths.md_path).exists());
        assert!(Path::new(&paths.json_path).exists());
        assert!(Path::new(&paths.trace_path).exists());

        let md = fs::read_to_string(&paths.md_path).unwrap();
        assert!(md.contains("Workflow 报告"));
        assert!(md.contains("健康评分"));
        assert!(md.contains("API 调用顺序"));

        let json = fs::read_to_string(&paths.json_path).unwrap();
        assert!(json.contains("run_id"));
        assert!(json.contains("validation"));

        let trace = fs::read_to_string(&paths.trace_path).unwrap();
        assert!(trace.contains("steps"));
    }

    #[test]
    fn report_summary_is_chinese() {
        let report = sample_report();
        let s = report.summary_zh();
        assert!(s.contains("Workflow 报告"));
        assert!(s.contains("健康评分"));
        assert!(report.success);
    }
}
