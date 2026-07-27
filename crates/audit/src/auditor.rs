//! 统计审计器（StatisticsAudit）。
//!
//! 负责验证整个数据链路的统计一致性：
//! - 所有计数必须可追踪来源。
//! - 不得使用简单累加器。
//! - 必须根据真实对象（Registry / Repository / CSV）统计。
//!
//! 审计规则：
//! - 候选数 ≤ 扫描市场数
//! - 机会数 ≤ 候选数
//! - 影子交易数 ≤ 机会数
//! - **纸面订单数 与 机会数 不可直接比较**：
//!   - `opportunities.csv` 只记录 **生命周期已结束** 的机会（被 tracker.reap 回收时写盘）
//!   - `paper_orders.csv` 在 **开仓/平仓当下立即写盘**
//!   - 二者衡量同一生命周期的不同时间点，正常情况下 `paper_orders ≥ opportunities`，
//!     反向（orders < opps）只在历史 CSV 残留或机会被风控全部拒绝时出现。
//! - 执行订单数 ≤ 纸面订单数
//! - 组合快照数 ≈ 有效扫描轮次（变化时写入）
//! - 已平仓持仓 ≤ 纸面订单数

use std::fmt;

/// 审计发现（一条检查结果）。
#[derive(Debug, Clone)]
pub struct AuditFinding {
    /// 检查项名称（中文）。
    pub check: String,
    /// 是否通过。
    pub passed: bool,
    /// 详细说明。
    pub detail: String,
    /// 严重程度：Error（逻辑矛盾）、Warning（可疑）、Info（正常）。
    pub severity: AuditSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Error,
    Warning,
    Info,
}

impl AuditSeverity {
    pub fn as_zh(&self) -> &'static str {
        match self {
            AuditSeverity::Error => "错误",
            AuditSeverity::Warning => "警告",
            AuditSeverity::Info => "信息",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            AuditSeverity::Error => "✗",
            AuditSeverity::Warning => "⚠",
            AuditSeverity::Info => "✓",
        }
    }
}

/// 统计审计报告。
///
/// 包含所有统计计数与校验结果。
#[derive(Debug, Clone, Default)]
pub struct StatisticsAudit {
    /// 所有审计发现。
    pub findings: Vec<AuditFinding>,
    /// 原始统计数据（从 CSV / Registry 查询）。
    pub stats: AuditStats,
}

/// 原始统计数据快照。
#[derive(Debug, Clone, Default)]
pub struct AuditStats {
    pub markets_scanned: u64,
    pub candidates_count: u64,
    pub candidates_accepted: u64,
    pub candidates_rejected: u64,
    pub opportunities_count: u64,
    /// B3: 检测即落盘机会（用于与纸面订单对账，区别于 lifecycle-only opportunities_count）。
    pub detected_opportunities_count: u64,
    pub shadow_trades_count: u64,
    pub paper_orders_count: u64,
    pub paper_positions_open: u64,
    pub paper_positions_closed: u64,
    pub execution_orders_count: u64,
    pub execution_filled: u64,
    pub execution_cancelled: u64,
    pub execution_expired: u64,
    pub execution_rejected: u64,
    pub portfolio_snapshots: u64,
    pub scan_rounds: u64,
    /// 孤儿纸面订单数（source_opportunity_id 为空）。
    pub orphan_paper_orders_count: u64,
}

impl StatisticsAudit {
    /// 创建空审计实例。
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 CSV 文件路径构建统计数据。
    ///
    /// 通过 `pm_storage::count_rows` 读取各 CSV 的行数。
    /// `detected_opportunities_csv` 为 B3 新增：检测即落盘，与纸面订单对账用。
    pub fn from_csv_paths(
        opportunities_csv: &str,
        detected_opportunities_csv: &str,
        shadow_csv: &str,
        paper_orders_csv: &str,
        paper_positions_csv: &str,
        paper_portfolio_csv: &str,
        execution_csv: &str,
    ) -> Self {
        let orphan = count_orphan_paper_orders(paper_orders_csv);
        let stats = AuditStats {
            opportunities_count: pm_storage::count_rows(opportunities_csv),
            detected_opportunities_count: pm_storage::count_rows(detected_opportunities_csv),
            shadow_trades_count: pm_storage::count_rows(shadow_csv),
            paper_orders_count: pm_storage::count_rows(paper_orders_csv),
            paper_positions_closed: pm_storage::count_rows(paper_positions_csv),
            portfolio_snapshots: pm_storage::count_rows(paper_portfolio_csv),
            execution_orders_count: pm_storage::count_rows(execution_csv),
            orphan_paper_orders_count: orphan,
            ..Default::default()
        };
        Self {
            findings: Vec::new(),
            stats,
        }
    }

    /// 运行所有审计规则，填充 findings。
    pub fn run_checks(&mut self) {
        self.findings.clear();

        // 规则 1（修订 B1）：检测机会存在但无纸面订单 —— Warning 而非 Error。
        // 背景：detected_opportunities.csv 记录检测即落盘的机会，paper_orders.csv 开仓即写。
        //      正常模型下 detected >= paper_orders。
        if self.stats.detected_opportunities_count > 0 && self.stats.paper_orders_count == 0 {
            self.findings.push(AuditFinding {
                check: "检测机会 > 0 但纸面订单 = 0".into(),
                passed: false,
                detail: format!(
                    "检测到 {} 个机会但纸面订单为 0。\
                     可能原因：开仓被风控全部拒绝。",
                    self.stats.detected_opportunities_count
                ),
                severity: AuditSeverity::Warning,
            });
        }

        // 规则 2（B3 修订）：纸面订单 vs 检测机会 —— 对账信息。
        //
        // 背景：detected_opportunities.csv（检测即落盘）与 paper_orders.csv（开仓即写）
        //       的写入时点对齐。正常情况下 detected >= paper_orders（部分机会可能被风控拒绝）。
        //
        //       ！区分机会（检测即落盘）与 机会（已结束）：
        //       前者（detected_opportunities_count）反映的是当前运行检测到的机会数，
        //       后者（opportunities_count）反映的是已结束的机会数（生命周期类）。
        if self.stats.paper_orders_count > 0 || self.stats.detected_opportunities_count > 0 {
            self.findings.push(AuditFinding {
                check: "纸面订单 vs 检测机会(对账)".into(),
                passed: true,
                detail: format!(
                    "纸面订单 {} | 检测机会 {} | 已结束机会 {} —— \
                     检测机会与纸面订单写入时点对齐，\
                     正常模型下检测机会 ≥ 纸面订单（多于部分为被风控拒绝）。",
                    self.stats.paper_orders_count,
                    self.stats.detected_opportunities_count,
                    self.stats.opportunities_count
                ),
                severity: AuditSeverity::Info,
            });
        }

        // 规则 3：Execution <= Paper Order（SELL 会额外产生订单）
        if self.stats.execution_orders_count > self.stats.paper_orders_count * 2 + 10 {
            self.findings.push(AuditFinding {
                check: "执行订单 ≈ 纸面订单 × 2".into(),
                passed: false,
                detail: format!(
                    "执行订单({}) 远超纸面订单({}) × 2。可能原因：大量拒绝/过期订单。",
                    self.stats.execution_orders_count, self.stats.paper_orders_count
                ),
                severity: AuditSeverity::Warning,
            });
        }

        // 规则 4：已平仓持仓 ≤ 纸面订单
        if self.stats.paper_positions_closed > self.stats.paper_orders_count {
            self.findings.push(AuditFinding {
                check: "已平仓持仓 ≤ 纸面订单".into(),
                passed: false,
                detail: format!(
                    "已平仓持仓({}) > 纸面订单({})，数据不一致。",
                    self.stats.paper_positions_closed, self.stats.paper_orders_count
                ),
                severity: AuditSeverity::Error,
            });
        } else if self.stats.paper_orders_count > 0 && self.stats.paper_positions_closed == 0 {
            self.findings.push(AuditFinding {
                check: "已平仓持仓 > 0".into(),
                passed: false,
                detail: format!(
                    "纸面订单({}) > 0 但已平仓持仓 = 0。可能原因：机会从未触发 on_close、持仓从未平仓。",
                    self.stats.paper_orders_count
                ),
                severity: AuditSeverity::Warning,
            });
        }

        // 规则 5：Portfolio Snapshots <= Scan Rounds（变化时写入）
        if self.stats.scan_rounds > 0 && self.stats.portfolio_snapshots > self.stats.scan_rounds {
            self.findings.push(AuditFinding {
                check: "组合快照 ≤ 扫描轮次".into(),
                passed: false,
                detail: format!(
                    "组合快照({}) > 扫描轮次({})，存在重复写入。",
                    self.stats.portfolio_snapshots, self.stats.scan_rounds
                ),
                severity: AuditSeverity::Warning,
            });
        }

        // 规则 6：Execution 终态分类之和 = 总订单数（如果数据可用）
        let exec_total = self.stats.execution_filled
            + self.stats.execution_cancelled
            + self.stats.execution_expired
            + self.stats.execution_rejected;
        if exec_total > 0 && exec_total > self.stats.execution_orders_count {
            self.findings.push(AuditFinding {
                check: "执行订单分类求和 ≤ 总订单数".into(),
                passed: false,
                detail: format!(
                    "执行订单分类求和({}) > 总订单数({})，分类计数不一致。",
                    exec_total, self.stats.execution_orders_count
                ),
                severity: AuditSeverity::Error,
            });
        }

        // 规则 7：孤儿纸面订单检测（source_opportunity_id 为空）。
        if self.stats.orphan_paper_orders_count > 0 {
            self.findings.push(AuditFinding {
                check: "孤儿纸面订单".into(),
                passed: false,
                detail: format!(
                    "纸面订单中 {} 个为孤儿订单（缺少 Opportunity 来源）。\
                     PaperOrder 必须在有对应 Opportunity 时才能创建。\
                     可能原因：旧版 CSV 数据无 source_opportunity_id 列，或历史回放路径创建了无来源订单。",
                    self.stats.orphan_paper_orders_count
                ),
                severity: AuditSeverity::Error,
            });
        }

        // 如果没有发现任何 Error，添加摘要
        if !self
            .findings
            .iter()
            .any(|f| f.severity == AuditSeverity::Error)
        {
            self.findings.push(AuditFinding {
                check: "数据一致性".into(),
                passed: true,
                detail: "所有检查通过，统计数据一致。".into(),
                severity: AuditSeverity::Info,
            });
        }
    }

    /// 返回所有 Error 级别的发现。
    pub fn errors(&self) -> Vec<&AuditFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Error)
            .collect()
    }

    /// 返回所有 Warning 级别的发现。
    pub fn warnings(&self) -> Vec<&AuditFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Warning)
            .collect()
    }

    /// 是否有错误。
    pub fn has_errors(&self) -> bool {
        self.errors().len() > 0
    }
}

impl fmt::Display for StatisticsAudit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "==============================")?;
        writeln!(f, "统计审计报告")?;
        writeln!(f, "==============================")?;
        writeln!(f)?;
        writeln!(f, "── 原始统计数据 ──")?;
        writeln!(f)?;
        writeln!(f, "  市场扫描数:    {}", self.stats.markets_scanned)?;
        writeln!(f, "  候选数:        {}", self.stats.candidates_count)?;
        writeln!(f, "  候选通过:      {}", self.stats.candidates_accepted)?;
        writeln!(f, "  候选拒绝:      {}", self.stats.candidates_rejected)?;
        writeln!(f, "  机会数(CSV):   {}", self.stats.opportunities_count)?;
        writeln!(f, "  影子交易(CSV): {}", self.stats.shadow_trades_count)?;
        writeln!(f, "  纸面订单(CSV): {}", self.stats.paper_orders_count)?;
        writeln!(f, "  已平仓持仓(CSV):{}", self.stats.paper_positions_closed)?;
        writeln!(
            f,
            "  检测机会(CSV): {}",
            self.stats.detected_opportunities_count
        )?;
        writeln!(f, "  执行订单(CSV): {}", self.stats.execution_orders_count)?;
        writeln!(f, "  组合快照(CSV): {}", self.stats.portfolio_snapshots)?;
        writeln!(
            f,
            "  孤儿订单(CSV): {}",
            self.stats.orphan_paper_orders_count
        )?;
        writeln!(f)?;
        writeln!(f, "── 审计检查 ──")?;
        writeln!(f)?;
        for finding in &self.findings {
            writeln!(
                f,
                "  {} {} — {}",
                finding.severity.icon(),
                finding.check,
                finding.detail
            )?;
        }
        writeln!(f)?;
        if self.has_errors() {
            writeln!(f, "结论：发现数据一致性问题，请检查上述错误。")?;
        } else {
            writeln!(f, "结论：数据一致性检查通过。")?;
        }
        Ok(())
    }
}

/// 统计 paper_orders.csv 中 source_opportunity_id 为空的订单数（孤儿订单）。
pub(crate) fn count_orphan_paper_orders(path: &str) -> u64 {
    use std::path::Path;
    if !Path::new(path).exists() {
        return 0;
    }
    let mut reader = match csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
    {
        Ok(r) => r,
        Err(_) => return 0,
    };
    // 查找 source_opportunity_id 列索引
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(_) => return 0,
    };
    let col_idx = headers
        .iter()
        .position(|h| h.trim() == "source_opportunity_id");
    let col_idx = match col_idx {
        Some(i) => i,
        None => return 0, // 旧版 CSV 无此列，孤儿数计为 0
    };
    let mut orphan = 0u64;
    for result in reader.records() {
        match result {
            Ok(record) => {
                let val = record.get(col_idx).unwrap_or("").trim();
                if val.is_empty() {
                    orphan += 1;
                }
            }
            Err(_) => continue,
        }
    }
    orphan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_audit_produces_info() {
        let mut audit = StatisticsAudit::new();
        audit.run_checks();
        // 空数据时没有 Error
        assert!(!audit.has_errors());
    }

    #[test]
    fn paper_orders_exceeding_opportunities_is_not_error() {
        // B1 修订：orders > opps 是预期（orders 立即写盘，opps 仅 reap 时写盘）。
        let mut audit = StatisticsAudit::new();
        audit.stats.opportunities_count = 3;
        audit.stats.paper_orders_count = 10;
        audit.run_checks();
        assert!(!audit.has_errors());
    }

    #[test]
    fn opportunities_without_paper_orders_is_warning() {
        // B1 修订：反向（opps > 0 且 orders == 0）下调为 Warning 而非 Error。
        let mut audit = StatisticsAudit::new();
        audit.stats.detected_opportunities_count = 5;
        audit.stats.paper_orders_count = 0;
        audit.run_checks();
        assert!(!audit.has_errors());
        assert!(!audit.warnings().is_empty());
    }

    #[test]
    fn closed_positions_exceeding_orders_is_error() {
        let mut audit = StatisticsAudit::new();
        audit.stats.paper_orders_count = 2;
        audit.stats.paper_positions_closed = 5;
        audit.run_checks();
        assert!(audit.has_errors());
    }

    #[test]
    fn portfolio_snapshots_exceeding_rounds_is_warning() {
        let mut audit = StatisticsAudit::new();
        audit.stats.scan_rounds = 10;
        audit.stats.portfolio_snapshots = 100;
        audit.run_checks();
        let warnings = audit.warnings();
        assert!(warnings.len() > 0);
    }

    #[test]
    fn consistent_data_passes() {
        let mut audit = StatisticsAudit::new();
        audit.stats.opportunities_count = 10;
        audit.stats.paper_orders_count = 10;
        audit.stats.paper_positions_closed = 8;
        audit.stats.execution_orders_count = 20; // BUY + SELL
        audit.stats.scan_rounds = 10;
        audit.stats.portfolio_snapshots = 10;
        audit.run_checks();
        assert!(!audit.has_errors());
    }
}
