//! 机会历史追踪器（V1.04 第八节）。
//!
//! 为每个 Opportunity 维护完整历史记录：
//! - 首次发现时间
//! - 最高 ROI
//! - 持续时长
//! - 更新次数
//! 方便后续统计。

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::model::{Opportunity, OpportunityStatus};

/// 单个机会的历史记录。
#[derive(Debug, Clone)]
pub struct OpportunityHistory {
    /// 机会 ID。
    pub id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 首次发现时间。
    pub first_seen: DateTime<Utc>,
    /// 最后出现时间。
    pub last_seen: DateTime<Utc>,
    /// 最高评分。
    pub best_score: f64,
    /// 最高 ROI。
    pub best_roi: f64,
    /// 最高置信度。
    pub best_confidence: f64,
    /// 最低 SUM（套利越大越好）。
    pub best_sum: f64,
    /// 累计更新次数。
    pub update_count: u64,
    /// 当前状态。
    pub status: OpportunityStatus,
    /// 分数历史（最近 N 次）。
    pub score_history: Vec<f64>,
}

impl OpportunityHistory {
    /// 从 Opportunity 创建新的历史记录。
    pub fn new(opp: &Opportunity, now: DateTime<Utc>) -> Self {
        Self {
            id: opp.id.clone(),
            market_id: opp.market_id.clone(),
            first_seen: now,
            last_seen: now,
            best_score: opp.score,
            best_roi: opp.expected_roi,
            best_confidence: opp.confidence,
            best_sum: opp.sum,
            update_count: 0,
            status: opp.status,
            score_history: vec![opp.score],
        }
    }

    /// 更新历史记录（同一机会再次出现时调用）。
    pub fn update(&mut self, opp: &Opportunity, now: DateTime<Utc>) {
        self.last_seen = now;
        self.update_count += 1;
        self.status = opp.status;

        if opp.score > self.best_score {
            self.best_score = opp.score;
        }
        if opp.expected_roi > self.best_roi {
            self.best_roi = opp.expected_roi;
        }
        if opp.confidence > self.best_confidence {
            self.best_confidence = opp.confidence;
        }
        if opp.sum < self.best_sum {
            self.best_sum = opp.sum;
        }

        // 保留最近 10 次评分
        self.score_history.push(opp.score);
        if self.score_history.len() > 10 {
            self.score_history.remove(0);
        }
    }

    /// 存活时长（秒）。
    pub fn duration_secs(&self) -> i64 {
        (self.last_seen - self.first_seen).num_seconds()
    }

    /// 评分趋势（正数=上升，负数=下降）。
    pub fn score_trend(&self) -> f64 {
        if self.score_history.len() < 2 {
            return 0.0;
        }
        let first = self.score_history.first().unwrap();
        let last = self.score_history.last().unwrap();
        last - first
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let trend = self.score_trend();
        let trend_str = if trend > 1.0 {
            "↑ 上升"
        } else if trend < -1.0 {
            "↓ 下降"
        } else {
            "→ 稳定"
        };
        format!(
            "首次={} | 持续={}秒 | 更新={}次 | 最高分={:.1} | 最高ROI={:.2}% | 趋势={}",
            self.first_seen.format("%H:%M:%S"),
            self.duration_secs(),
            self.update_count,
            self.best_score,
            self.best_roi * 100.0,
            trend_str,
        )
    }
}

/// 机会历史追踪器：维护所有机会的完整历史。
#[derive(Debug, Clone, Default)]
pub struct HistoryTracker {
    /// key = market_id → 历史记录。
    records: HashMap<String, OpportunityHistory>,
}

impl HistoryTracker {
    /// 创建新的追踪器。
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// 记录一个新机会或更新已有记录。
    /// 返回是否为新记录。
    pub fn record(&mut self, opp: &Opportunity, now: DateTime<Utc>) -> bool {
        match self.records.get_mut(&opp.market_id) {
            Some(history) => {
                history.update(opp, now);
                false
            }
            None => {
                let history = OpportunityHistory::new(opp, now);
                self.records.insert(opp.market_id.clone(), history);
                true
            }
        }
    }

    /// 获取某个市场的历史记录。
    pub fn get(&self, market_id: &str) -> Option<&OpportunityHistory> {
        self.records.get(market_id)
    }

    /// 记录总数。
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// 获取所有历史记录的不可变引用。
    pub fn all(&self) -> Vec<&OpportunityHistory> {
        let mut v: Vec<_> = self.records.values().collect();
        v.sort_by_key(|h| std::cmp::Reverse(h.update_count));
        v
    }

    /// 清理终态记录（Expired / Removed）。
    pub fn cleanup_terminal(&mut self) -> usize {
        let before = self.records.len();
        self.records.retain(|_, h| !h.status.is_terminal());
        before - self.records.len()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OpportunityType;
    use chrono::Utc;

    fn make_opp(market_id: &str, score: f64, roi: f64) -> Opportunity {
        Opportunity::new(
            market_id.into(),
            format!("Q_{market_id}"),
            "test".into(),
            Utc::now(),
            OpportunityType::Unknown,
            score,
            0.8,
            (score * 0.8) as u8,
            score * 0.25,
            score * 0.20,
            0.0,
            0.0,
            0.0,
            score * 0.1,
            roi,
            1.0,
            0.5,
            0.5,
            1.0,
            None,
            1000.0,
            2000.0,
            None,
            None,
        )
    }

    #[test]
    fn new_record_then_update() {
        let now = Utc::now();
        let mut tracker = HistoryTracker::new();

        let opp1 = make_opp("m1", 70.0, 0.02);
        assert!(tracker.record(&opp1, now)); // 新记录

        let opp2 = make_opp("m1", 85.0, 0.03);
        assert!(!tracker.record(&opp2, now)); // 更新

        let history = tracker.get("m1").unwrap();
        assert_eq!(history.update_count, 1);
        assert_eq!(history.best_score, 85.0);
        assert!((history.best_roi - 0.03).abs() < 0.001);
    }

    #[test]
    fn score_trend_up_down_stable() {
        let now = Utc::now();
        let mut tracker = HistoryTracker::new();

        // 逐步提升
        tracker.record(&make_opp("m1", 50.0, 0.01), now);
        tracker.record(&make_opp("m1", 60.0, 0.01), now);
        tracker.record(&make_opp("m1", 70.0, 0.01), now);

        let history = tracker.get("m1").unwrap();
        assert!(history.score_trend() > 0.0, "评分趋势应上升");
    }

    #[test]
    fn history_duration_is_positive() {
        let now = Utc::now();
        let mut tracker = HistoryTracker::new();
        tracker.record(&make_opp("m1", 70.0, 0.02), now);

        let later = now + chrono::Duration::seconds(60);
        tracker.record(&make_opp("m1", 75.0, 0.02), later);

        let history = tracker.get("m1").unwrap();
        assert!(history.duration_secs() >= 60);
    }

    #[test]
    fn cleanup_removes_terminal() {
        let now = Utc::now();
        let mut tracker = HistoryTracker::new();

        let mut opp = make_opp("m1", 70.0, 0.02);
        opp.status = OpportunityStatus::Expired;
        tracker.record(&opp, now);

        let opp2 = make_opp("m2", 80.0, 0.03);
        tracker.record(&opp2, now);

        assert_eq!(tracker.len(), 2);
        let removed = tracker.cleanup_terminal();
        assert_eq!(removed, 1);
        assert_eq!(tracker.len(), 1);
        assert!(tracker.get("m1").is_none());
        assert!(tracker.get("m2").is_some());
    }

    #[test]
    fn summary_zh_contains_key_info() {
        let now = Utc::now();
        let mut tracker = HistoryTracker::new();
        tracker.record(&make_opp("m1", 70.0, 0.02), now);
        tracker.record(&make_opp("m1", 85.0, 0.03), now);

        let history = tracker.get("m1").unwrap();
        let s = history.summary_zh();
        assert!(s.contains("最高分"));
        assert!(s.contains("最高ROI"));
        assert!(s.contains("更新"));
    }
}
