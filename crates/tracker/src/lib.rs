//! pm-tracker：机会生命周期跟踪器。
//!
//! 维护 `HashMap<Question, OpportunityState>`：
//! - [`OpportunityTracker::observe`]：本轮发现的机会，不存在则创建、存在则更新，返回 [`TrackUpdate`]。
//! - [`OpportunityTracker::reap`]：本轮未再出现的机会视为生命周期结束，移出并返回 `Vec<FinishedOpportunity>`。
//!
//! Key 暂用 Question（题目文本），后续可换成 conditionId 等更稳定的标识。
//! DTO（`OpportunityState` / `TrackUpdate` / `FinishedOpportunity`）归 pm-models。

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local};

use pm_models::{FinishedOpportunity, OppSnapshot, OpportunityState, TrackUpdate};

/// 跟踪器：以 Question 为 Key 持有所有活跃机会的状态。
pub struct OpportunityTracker {
    map: HashMap<String, OpportunityState>,
}

impl OpportunityTracker {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// 当前仍在跟踪中（本轮还活着）的机会数。
    pub fn active_count(&self) -> usize {
        self.map.len()
    }

    /// 观察到一个本轮发现的机会：不存在则创建，存在则更新。返回事件信息。
    pub fn observe(&mut self, snap: &OppSnapshot, now: DateTime<Local>) -> TrackUpdate {
        match self.map.get_mut(&snap.question) {
            Some(state) => {
                // 已存在：更新本轮字段。best_sum 取历史最低（套利越大越好）。
                state.last_seen = now;
                state.scan_count += 1;
                if snap.sum < state.best_sum {
                    state.best_sum = snap.sum;
                }
                state.last_yes = snap.yes_price;
                state.last_no = snap.no_price;
                state.volume = snap.volume;
                state.liquidity = snap.liquidity;
                let duration_sec = (now - state.start_time).num_seconds();
                TrackUpdate {
                    is_new: false,
                    question: state.question.clone(),
                    duration_sec,
                    best_sum: state.best_sum,
                    scan_count: state.scan_count,
                    sum: snap.sum,
                }
            }
            None => {
                // 新机会：创建状态
                let state = OpportunityState {
                    question: snap.question.clone(),
                    start_time: now,
                    last_seen: now,
                    best_sum: snap.sum,
                    scan_count: 1,
                    last_yes: snap.yes_price,
                    last_no: snap.no_price,
                    volume: snap.volume,
                    liquidity: snap.liquidity,
                };
                self.map.insert(snap.question.clone(), state);
                TrackUpdate {
                    is_new: true,
                    question: snap.question.clone(),
                    duration_sec: 0,
                    best_sum: snap.sum,
                    scan_count: 1,
                    sum: snap.sum,
                }
            }
        }
    }

    /// 清理本轮未再出现的机会 + 到期强制平仓（B6）。
    /// `seen_keys` 为本轮实际观测到的机会 Key 集合。
    /// `max_age_secs` > 0 时：跟踪时长超过此值的也视为生命周期结束。
    /// 返回已结束的机会列表。
    pub fn reap(
        &mut self,
        seen_keys: &HashSet<String>,
        now: DateTime<Local>,
        max_age_secs: u64,
    ) -> Vec<FinishedOpportunity> {
        let mut finished: Vec<FinishedOpportunity> = Vec::new();
        let mut remove_keys: Vec<String> = Vec::new();

        for (key, state) in self.map.iter() {
            let should_close = if !seen_keys.contains(key) {
                true // 本轮未出现 → 自然结束
            } else if max_age_secs > 0 {
                (now - state.start_time).num_seconds() > max_age_secs as i64 // B6: 超时到期
            } else {
                false
            };

            if should_close {
                let duration_sec = (now - state.start_time).num_seconds();
                finished.push(FinishedOpportunity {
                    question: state.question.clone(),
                    start_time: state.start_time,
                    end_time: now,
                    duration_sec,
                    best_sum: state.best_sum,
                    scan_count: state.scan_count,
                    last_yes: state.last_yes,
                    last_no: state.last_no,
                    volume: state.volume,
                    liquidity: state.liquidity,
                });
                remove_keys.push(key.clone());
            }
        }
        for key in remove_keys {
            self.map.remove(&key);
        }
        finished
    }
}

impl Default for OpportunityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(q: &str, yes: f64, no: f64) -> OppSnapshot {
        OppSnapshot {
            question: q.into(),
            yes_price: yes,
            no_price: no,
            sum: yes + no,
            volume: 0.0,
            liquidity: 0.0,
        }
    }

    #[test]
    fn observe_new_then_update() {
        let now = Local::now();
        let mut t = OpportunityTracker::new();
        let ev1 = t.observe(&snap("Q1", 0.40, 0.55), now);
        assert!(ev1.is_new);
        assert_eq!(ev1.scan_count, 1);
        assert_eq!(t.active_count(), 1);

        // 第二轮：SUM 更低 -> best_sum 更新
        let ev2 = t.observe(&snap("Q1", 0.38, 0.55), now);
        assert!(!ev2.is_new);
        assert_eq!(ev2.scan_count, 2);
        assert!((ev2.best_sum - 0.93).abs() < 1e-9);
    }

    #[test]
    fn reap_removes_unseen() {
        let now = Local::now();
        let mut t = OpportunityTracker::new();
        t.observe(&snap("A", 0.4, 0.5), now);
        t.observe(&snap("B", 0.4, 0.5), now);

        // 本轮只看到 A
        let mut seen = HashSet::new();
        seen.insert("A".to_string());
        let finished = t.reap(&seen, now, 0);
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].question, "B");
        assert_eq!(t.active_count(), 1);
    }

    #[test]
    fn reap_empty_when_all_seen() {
        let now = Local::now();
        let mut t = OpportunityTracker::new();
        t.observe(&snap("A", 0.4, 0.5), now);
        let mut seen = HashSet::new();
        seen.insert("A".to_string());
        assert!(t.reap(&seen, now, 0).is_empty());
        assert_eq!(t.active_count(), 1);
    }

    #[test]
    fn reap_stale_opportunity_by_age() {
        let now = Local::now();
        let mut t = OpportunityTracker::new();
        // 插入一个旧机会（把 start_time 手工设为远早于 now）
        t.observe(&snap("Old", 0.4, 0.5), now);
        let stale_age_secs = 1i64; // 1 秒后即视为到期

        let mut seen = HashSet::new();
        seen.insert("Old".to_string());
        // 先确认年龄小于阈值时不回收
        assert!(t.reap(&seen, now, stale_age_secs as u64).is_empty());
        assert_eq!(t.active_count(), 1);

        // 假装过了很久再 reap → 强制到期
        let later = now + chrono::Duration::seconds(2);
        let finished = t.reap(&seen, later, stale_age_secs as u64);
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].question, "Old");
        assert_eq!(t.active_count(), 0);
    }
}
