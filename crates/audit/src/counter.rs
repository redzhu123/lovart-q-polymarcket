//! 拒绝原因计数器。
//!
//! 按拒绝原因分组统计，支持累加、查询 Top-N、中文报告。

use std::collections::HashMap;
use std::fmt;

use crate::rejection::CandidateRejection;

/// 拒绝原因计数器。
///
/// 维护 `CandidateRejection -> u64` 映射，提供累加与查询接口。
#[derive(Debug, Clone, Default)]
pub struct RejectionCounter {
    counts: HashMap<CandidateRejection, u64>,
}

impl RejectionCounter {
    /// 创建空计数器。
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// 累加一次拒绝。
    pub fn record(&mut self, reason: CandidateRejection) {
        *self.counts.entry(reason).or_insert(0) += 1;
    }

    /// 批量累加。
    pub fn record_batch(&mut self, reasons: &[CandidateRejection]) {
        for r in reasons {
            self.record(*r);
        }
    }

    /// 查询某个原因的计数。
    pub fn count(&self, reason: CandidateRejection) -> u64 {
        self.counts.get(&reason).copied().unwrap_or(0)
    }

    /// 总拒绝数。
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }

    /// 返回按计数降序排列的 (原因, 计数) 列表。
    pub fn sorted_counts(&self) -> Vec<(CandidateRejection, u64)> {
        let mut pairs: Vec<(CandidateRejection, u64)> =
            self.counts.iter().map(|(&k, &v)| (k, v)).collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        pairs
    }

    /// Top-N 拒绝原因（按计数降序）。
    pub fn top_n(&self, n: usize) -> Vec<(CandidateRejection, u64)> {
        let mut all = self.sorted_counts();
        all.truncate(n);
        all
    }

    /// 按类别分组的拒绝原因统计（数据问题 / 市场状态 / 策略过滤）。
    pub fn category_counts(&self) -> RejectionCategoryCounts {
        let mut data_issues = 0u64;
        let mut market_state = 0u64;
        let mut strategy_filter = 0u64;
        for (&reason, &count) in &self.counts {
            if reason.is_data_issue() {
                data_issues += count;
            } else if reason.is_market_state() {
                market_state += count;
            } else if reason.is_strategy_filter() {
                strategy_filter += count;
            }
        }
        RejectionCategoryCounts {
            data_issues,
            market_state,
            strategy_filter,
        }
    }
}

/// 按类别分组的拒绝统计。
#[derive(Debug, Clone, Default)]
pub struct RejectionCategoryCounts {
    pub data_issues: u64,
    pub market_state: u64,
    pub strategy_filter: u64,
}

impl fmt::Display for RejectionCategoryCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  数据问题: {}", self.data_issues)?;
        writeln!(f, "  市场状态: {}", self.market_state)?;
        write!(f, "  策略过滤: {}", self.strategy_filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_count() {
        let mut counter = RejectionCounter::new();
        counter.record(CandidateRejection::SpreadTooSmall);
        counter.record(CandidateRejection::SpreadTooSmall);
        counter.record(CandidateRejection::MarketClosed);
        assert_eq!(counter.count(CandidateRejection::SpreadTooSmall), 2);
        assert_eq!(counter.count(CandidateRejection::MarketClosed), 1);
        assert_eq!(counter.count(CandidateRejection::Inactive), 0);
        assert_eq!(counter.total(), 3);
    }

    #[test]
    fn sorted_counts_returns_descending() {
        let mut counter = RejectionCounter::new();
        counter.record(CandidateRejection::SpreadTooSmall);
        counter.record(CandidateRejection::MarketClosed);
        counter.record(CandidateRejection::MarketClosed);
        counter.record(CandidateRejection::Inactive);
        let sorted = counter.sorted_counts();
        assert_eq!(sorted[0].0, CandidateRejection::MarketClosed);
        assert_eq!(sorted[0].1, 2);
    }

    #[test]
    fn top_n_truncates() {
        let mut counter = RejectionCounter::new();
        counter.record(CandidateRejection::SpreadTooSmall);
        counter.record(CandidateRejection::MarketClosed);
        counter.record(CandidateRejection::Inactive);
        let top = counter.top_n(2);
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn category_counts_groups_correctly() {
        let mut counter = RejectionCounter::new();
        counter.record(CandidateRejection::PriceInvalid); // data
        counter.record(CandidateRejection::MarketClosed); // market
        counter.record(CandidateRejection::SpreadTooSmall); // strategy
        counter.record(CandidateRejection::SpreadTooSmall); // strategy
        let cats = counter.category_counts();
        assert_eq!(cats.data_issues, 1);
        assert_eq!(cats.market_state, 1);
        assert_eq!(cats.strategy_filter, 2);
    }
}
