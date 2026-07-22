//! pm-metrics：统一指标计数器 + 统计辅助。
//!
//! 统一统计 Scanner / Opportunity / Shadow / Paper / Execution / Portfolio 各维度运行计数，
//! 由 driver 在每轮更新，供仪表盘与 `report` 命令聚合展示。
//!
//! 设计：`Metrics` 为会话内计数器（非持久化），只接受**纯数字增量**，
//! 不依赖任何 engine crate（计数由 driver 从 `ScanEvents` 提取后传入），保持低耦合。
//! 持久化统计走各 engine 的 CSV。纯统计数学复用 [`pm_utils`]。

/// 统一指标计数器。
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    // Scanner
    pub scan_rounds: u64,
    // Opportunity
    pub opp_new: u64,
    pub opp_updated: u64,
    pub opp_finished: u64,
    // Shadow
    pub shadow_opened: u64,
    pub shadow_closed: u64,
    // Paper
    pub paper_opens: u64,
    pub paper_closes: u64,
    pub paper_rejections: u64,
    // Execution
    pub exec_submitted: u64,
    pub exec_filled: u64,
    pub exec_cancelled: u64,
    pub exec_expired: u64,
    pub exec_rejected: u64,
    // Portfolio
    pub portfolio_snapshots: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一轮扫描完成。
    pub fn record_round(&mut self) {
        self.scan_rounds += 1;
    }

    /// 累加本轮机会事件计数。
    pub fn add_opportunities(&mut self, new: u64, updated: u64, finished: u64) {
        self.opp_new += new;
        self.opp_updated += updated;
        self.opp_finished += finished;
    }

    /// 累加本轮 shadow 开/平计数。
    pub fn add_shadow(&mut self, opened: u64, closed: u64) {
        self.shadow_opened += opened;
        self.shadow_closed += closed;
    }

    /// 累加本轮 paper 开/平/拒计数。
    pub fn add_paper(&mut self, opens: u64, closes: u64, rejections: u64) {
        self.paper_opens += opens;
        self.paper_closes += closes;
        self.paper_rejections += rejections;
    }

    /// 累加本轮 execution 终端计数。
    pub fn add_exec(
        &mut self,
        submitted: u64,
        filled: u64,
        cancelled: u64,
        expired: u64,
        rejected: u64,
    ) {
        self.exec_submitted += submitted;
        self.exec_filled += filled;
        self.exec_cancelled += cancelled;
        self.exec_expired += expired;
        self.exec_rejected += rejected;
    }

    /// 记录一次组合快照写入。
    pub fn add_portfolio_snapshot(&mut self) {
        self.portfolio_snapshots += 1;
    }

    /// Execution Fill Rate = filled / submitted。
    pub fn exec_fill_rate(&self) -> f64 {
        pm_utils::ratio(self.exec_filled, self.exec_submitted)
    }

    /// Paper 开仓成功率 = opens / (opens + rejections)。
    pub fn paper_open_success_rate(&self) -> f64 {
        let denom = self.paper_opens + self.paper_rejections;
        pm_utils::ratio(self.paper_opens, denom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_counts() {
        let mut m = Metrics::new();
        m.record_round();
        m.add_opportunities(2, 3, 1);
        m.add_shadow(2, 1);
        m.add_paper(2, 1, 0);
        m.add_exec(2, 1, 0, 0, 0);
        m.add_portfolio_snapshot();

        assert_eq!(m.scan_rounds, 1);
        assert_eq!(m.opp_new, 2);
        assert_eq!(m.opp_updated, 3);
        assert_eq!(m.opp_finished, 1);
        assert_eq!(m.shadow_opened, 2);
        assert_eq!(m.paper_opens, 2);
        assert_eq!(m.exec_submitted, 2);
        assert_eq!(m.portfolio_snapshots, 1);
    }

    #[test]
    fn rates_zero_safe() {
        let m = Metrics::new();
        assert_eq!(m.exec_fill_rate(), 0.0);
        assert_eq!(m.paper_open_success_rate(), 0.0);
    }

    #[test]
    fn rates_computed() {
        let mut m = Metrics::new();
        m.add_exec(10, 7, 1, 1, 1);
        assert!((m.exec_fill_rate() - 0.7).abs() < 1e-9);
        m.add_paper(8, 0, 2);
        assert!((m.paper_open_success_rate() - 0.8).abs() < 1e-9);
    }
}
