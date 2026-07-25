//! 报告生成器：Explain Report + Audit Report。
//!
//! 提供两种报告：
//! - `ExplainReport`：完整数据链路分析（用于 `cargo run -- explain`）。
//! - `AuditReport`：自动审计报告（用于 `cargo run -- audit`）。
//!
//! 所有报告均为中文。

#[cfg(test)]
use crate::rejection::CandidateRejection;

use crate::auditor::{AuditSeverity, AuditStats};
use crate::counter::RejectionCounter;

/// 完整数据链路解释报告。
#[derive(Debug, Clone)]
pub struct ExplainReport {
    /// 统计快照。
    pub stats: AuditStats,
    /// 拒绝原因计数器。
    pub rejection_counter: RejectionCounter,
    /// 额外说明。
    pub notes: Vec<String>,
}

impl ExplainReport {
    /// 创建空报告。
    pub fn new() -> Self {
        Self {
            stats: AuditStats::default(),
            rejection_counter: RejectionCounter::new(),
            notes: Vec::new(),
        }
    }

    /// 从 CSV 路径构建报告。
    pub fn from_csv_paths(
        opportunities_csv: &str,
        detected_opportunities_csv: &str,
        shadow_csv: &str,
        paper_orders_csv: &str,
        paper_positions_csv: &str,
        paper_portfolio_csv: &str,
        execution_csv: &str,
    ) -> Self {
        let stats = AuditStats {
            opportunities_count: pm_storage::count_rows(opportunities_csv),
            detected_opportunities_count: pm_storage::count_rows(detected_opportunities_csv),
            shadow_trades_count: pm_storage::count_rows(shadow_csv),
            paper_orders_count: pm_storage::count_rows(paper_orders_csv),
            paper_positions_closed: pm_storage::count_rows(paper_positions_csv),
            portfolio_snapshots: pm_storage::count_rows(paper_portfolio_csv),
            execution_orders_count: pm_storage::count_rows(execution_csv),
            ..Default::default()
        };
        Self {
            stats,
            rejection_counter: RejectionCounter::new(),
            notes: Vec::new(),
        }
    }

    /// 添加备注。
    pub fn add_note(&mut self, note: String) {
        self.notes.push(note);
    }

    /// 生成中文报告字符串。
    pub fn render_zh(&self) -> String {
        let mut out = String::new();

        out.push_str("=========================\n");
        out.push_str("V1.09 数据链路分析\n");
        out.push_str("=========================\n\n");

        // Pipeline statistics
        out.push_str("── 流水线统计 ──\n\n");
        if self.stats.markets_scanned > 0 {
            out.push_str(&format!(
                "  市场扫描数:     {}\n",
                self.stats.markets_scanned
            ));
        }
        if self.stats.candidates_count > 0 {
            out.push_str(&format!(
                "  候选数:         {}\n",
                self.stats.candidates_count
            ));
            out.push_str(&format!(
                "  候选通过:       {}\n",
                self.stats.candidates_accepted
            ));
            out.push_str(&format!(
                "  候选拒绝:       {}\n",
                self.stats.candidates_rejected
            ));
        }
        out.push_str(&format!(
            "  机会数(CSV):    {}\n",
            self.stats.opportunities_count
        ));
        out.push_str(&format!(
            "  影子交易(CSV):  {}\n",
            self.stats.shadow_trades_count
        ));
        out.push_str(&format!(
            "  纸面订单(CSV):  {}\n",
            self.stats.paper_orders_count
        ));
        out.push_str(&format!(
            "  执行订单(CSV):  {}\n",
            self.stats.execution_orders_count
        ));
        out.push_str(&format!(
            "  已平仓持仓(CSV):{}\n",
            self.stats.paper_positions_closed
        ));
        out.push_str(&format!(
            "  当前持仓:       {}\n",
            self.stats.paper_positions_open
        ));
        out.push_str(&format!(
            "  组合快照(CSV):  {}\n",
            self.stats.portfolio_snapshots
        ));
        out.push_str(&format!("  扫描轮次:       {}\n", self.stats.scan_rounds));
        out.push('\n');

        // Execution breakdown
        if self.stats.execution_orders_count > 0 {
            out.push_str("── 执行订单分类 ──\n\n");
            out.push_str(&format!(
                "  总订单:         {}\n",
                self.stats.execution_orders_count
            ));
            out.push_str(&format!(
                "  已成交:         {}\n",
                self.stats.execution_filled
            ));
            out.push_str(&format!(
                "  已取消:         {}\n",
                self.stats.execution_cancelled
            ));
            out.push_str(&format!(
                "  已过期:         {}\n",
                self.stats.execution_expired
            ));
            out.push_str(&format!(
                "  已拒绝:         {}\n",
                self.stats.execution_rejected
            ));
            out.push('\n');
        }

        // Rejection summary
        if self.rejection_counter.total() > 0 {
            out.push_str("=========================\n");
            out.push_str("拒绝原因 Top 10\n");
            out.push_str("=========================\n\n");
            let top = self.rejection_counter.top_n(10);
            let mut max_len = 0usize;
            for (reason, _) in &top {
                let zh = reason.as_zh();
                if zh.len() > max_len {
                    max_len = zh.len();
                }
            }
            for (reason, count) in &top {
                out.push_str(&format!(
                    "  {:<width$}:  {}\n",
                    reason.as_zh(),
                    count,
                    width = max_len + 4
                ));
            }
            out.push('\n');

            // Category breakdown
            let cats = self.rejection_counter.category_counts();
            out.push_str("── 拒绝类别 ──\n\n");
            out.push_str(&format!("  数据问题:  {}\n", cats.data_issues));
            out.push_str(&format!("  市场状态:  {}\n", cats.market_state));
            out.push_str(&format!("  策略过滤:  {}\n", cats.strategy_filter));
            out.push('\n');
        }

        // Notes
        if !self.notes.is_empty() {
            out.push_str("=========================\n");
            out.push_str("分析说明\n");
            out.push_str("=========================\n\n");
            for note in &self.notes {
                out.push_str(&format!("  • {}\n", note));
            }
            out.push('\n');
        }

        out.push_str("=========================\n");
        out
    }
}

impl Default for ExplainReport {
    fn default() -> Self {
        Self::new()
    }
}

/// 自动审计报告。
#[derive(Debug, Clone)]
pub struct AuditReport {
    pub stats: AuditStats,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

impl AuditReport {
    pub fn new() -> Self {
        Self {
            stats: AuditStats::default(),
            errors: Vec::new(),
            warnings: Vec::new(),
            infos: Vec::new(),
        }
    }

    /// 从 CSV 路径构建并运行审计。
    pub fn run(
        opportunities_csv: &str,
        detected_opportunities_csv: &str,
        shadow_csv: &str,
        paper_orders_csv: &str,
        paper_positions_csv: &str,
        paper_portfolio_csv: &str,
        execution_csv: &str,
    ) -> Self {
        let mut audit = crate::auditor::StatisticsAudit::from_csv_paths(
            opportunities_csv,
            detected_opportunities_csv,
            shadow_csv,
            paper_orders_csv,
            paper_positions_csv,
            paper_portfolio_csv,
            execution_csv,
        );
        audit.run_checks();

        let mut report = Self {
            stats: audit.stats,
            errors: Vec::new(),
            warnings: Vec::new(),
            infos: Vec::new(),
        };

        for f in &audit.findings {
            let line = format!("{} {} — {}", f.severity.icon(), f.check, f.detail);
            match f.severity {
                AuditSeverity::Error => report.errors.push(line),
                AuditSeverity::Warning => report.warnings.push(line),
                AuditSeverity::Info => report.infos.push(line),
            }
        }

        report
    }

    /// 渲染为中文报告字符串。
    pub fn render_zh(&self) -> String {
        let mut out = String::new();

        out.push_str("=========================\n");
        out.push_str("V1.09 自动审计报告\n");
        out.push_str("=========================\n\n");

        out.push_str("── 统计数据 ──\n\n");
        out.push_str(&format!(
            "  机会数(CSV):    {}\n",
            self.stats.opportunities_count
        ));
        out.push_str(&format!(
            "  影子交易(CSV):  {}\n",
            self.stats.shadow_trades_count
        ));
        out.push_str(&format!(
            "  纸面订单(CSV):  {}\n",
            self.stats.paper_orders_count
        ));
        out.push_str(&format!(
            "  已平仓持仓(CSV):{}\n",
            self.stats.paper_positions_closed
        ));
        out.push_str(&format!(
            "  执行订单(CSV):  {}\n",
            self.stats.execution_orders_count
        ));
        out.push_str(&format!(
            "  组合快照(CSV):  {}\n",
            self.stats.portfolio_snapshots
        ));
        out.push('\n');

        if !self.errors.is_empty() {
            out.push_str("── 错误 ──\n\n");
            for e in &self.errors {
                out.push_str(&format!("  {}\n", e));
            }
            out.push('\n');
        }

        if !self.warnings.is_empty() {
            out.push_str("── 警告 ──\n\n");
            for w in &self.warnings {
                out.push_str(&format!("  {}\n", w));
            }
            out.push('\n');
        }

        if !self.infos.is_empty() {
            out.push_str("── 信息 ──\n\n");
            for i in &self.infos {
                out.push_str(&format!("  {}\n", i));
            }
            out.push('\n');
        }

        if self.errors.is_empty() {
            out.push_str("✅ 审计通过：未发现数据一致性问题。\n");
        } else {
            out.push_str(&format!(
                "❌ 发现 {} 个数据一致性问题，请修复后重新运行。\n",
                self.errors.len()
            ));
        }
        out.push('\n');
        out.push_str("=========================\n");

        out
    }
}

impl Default for AuditReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_report_renders_all_sections() {
        let mut report = ExplainReport::new();
        report.stats.opportunities_count = 10;
        report.stats.shadow_trades_count = 10;
        report.stats.paper_orders_count = 8;
        report.stats.execution_orders_count = 16;
        report.stats.paper_positions_closed = 7;
        report.stats.portfolio_snapshots = 8;
        report
            .rejection_counter
            .record(CandidateRejection::SpreadTooSmall);
        report
            .rejection_counter
            .record(CandidateRejection::SpreadTooSmall);
        report
            .rejection_counter
            .record(CandidateRejection::MarketClosed);

        let rendered = report.render_zh();
        assert!(rendered.contains("数据链路分析"));
        assert!(rendered.contains("机会数(CSV)"));
        assert!(rendered.contains("纸面订单(CSV)"));
        assert!(rendered.contains("拒绝原因 Top 10"));
        assert!(rendered.contains("价差过小"));
    }

    #[test]
    fn audit_report_detects_errors() {
        let report = AuditReport {
            stats: AuditStats {
                opportunities_count: 5,
                paper_orders_count: 0,
                ..Default::default()
            },
            errors: vec!["测试错误 — 机会数 > 0 但纸面订单 = 0".into()],
            warnings: Vec::new(),
            infos: Vec::new(),
        };

        let rendered = report.render_zh();
        assert!(rendered.contains("测试错误"));
        assert!(rendered.contains("发现 1 个数据一致性问题"));
    }

    #[test]
    fn audit_report_passes_when_no_errors() {
        let report = AuditReport {
            stats: AuditStats::default(),
            errors: Vec::new(),
            warnings: Vec::new(),
            infos: vec!["测试信息".into()],
        };

        let rendered = report.render_zh();
        assert!(rendered.contains("审计通过"));
    }
}
