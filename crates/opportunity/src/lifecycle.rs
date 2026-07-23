//! 机会生命周期管理（V1.04 第七节）。
//!
//! 状态机：
//! ```text
//! Created → Updated → Stable → Weak → Expired → Removed
//!              ↑          |        |
//!              +----------+--------+
//! ```
//!
//! 每个状态记录进入时间、持续时长、变更次数。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::OpportunityStatus;

/// 单次状态变更记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusTransition {
    /// 变更前状态。
    pub from: OpportunityStatus,
    /// 变更后状态。
    pub to: OpportunityStatus,
    /// 变更时间（UTC）。
    pub timestamp: DateTime<Utc>,
    /// 在前一状态停留的秒数。
    pub duration_secs: i64,
}

/// 生命周期管理器。
///
/// 维护一个 Opportunity 从创建到移除的完整状态历史。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifecycle {
    /// 当前状态。
    pub current: OpportunityStatus,
    /// 状态变更历史（按时间顺序）。
    pub history: Vec<StatusTransition>,
    /// 状态变更总次数。
    pub transition_count: u32,
    /// 首次创建时间。
    pub created_at: DateTime<Utc>,
    /// 总存活时长（秒）。
    pub total_duration_secs: i64,
}

impl Lifecycle {
    /// 创建新的生命周期（初始状态 = Created）。
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            current: OpportunityStatus::Created,
            history: Vec::new(),
            transition_count: 0,
            created_at: now,
            total_duration_secs: 0,
        }
    }

    /// 尝试转换到目标状态。
    ///
    /// 允许的转换：
    /// - Created → Updated, Stable, Weak, Expired
    /// - Updated → Stable, Weak, Expired
    /// - Stable → Weak, Expired
    /// - Weak → Updated（恢复）, Expired
    /// - Expired → Removed
    /// 相同状态不变（幂等）。
    pub fn transition(&mut self, target: OpportunityStatus, now: DateTime<Utc>) -> bool {
        if self.current == target {
            return false; // 幂等：同状态不重复记录
        }

        // 检查转换是否合法
        if !Self::is_valid_transition(self.current, target) {
            return false;
        }

        let duration_secs = (now - self.created_at).num_seconds();
        let transition = StatusTransition {
            from: self.current,
            to: target,
            timestamp: now,
            duration_secs,
        };

        self.history.push(transition);
        self.current = target;
        self.transition_count += 1;
        self.total_duration_secs = duration_secs;

        true
    }

    /// 根据当前状态和新的机会数据，自动判定下一状态。
    ///
    /// 规则：
    /// - 首次出现 → Created（已在 new() 中设置）
    /// - 持续轮数 ≥ 3，评分稳定 → Stable
    /// - 持续轮数 < 3 → Updated
    /// - 评分 < 30 或置信度 < 0.3 → Weak
    /// - 超过 TTL 未出现 → Expired
    pub fn auto_transition(
        &mut self,
        scan_count: u64,
        score: f64,
        confidence: f64,
        is_new: bool,
        now: DateTime<Utc>,
    ) -> OpportunityStatus {
        if is_new {
            // 不会发生——仅在首次创建时使用 new()
            return self.current;
        }

        let target = if score < 20.0 || confidence < 0.2 {
            OpportunityStatus::Weak
        } else if scan_count >= 3 && score >= 50.0 && confidence >= 0.5 {
            OpportunityStatus::Stable
        } else {
            OpportunityStatus::Updated
        };

        self.transition(target, now);
        self.current
    }

    /// 标记为过期。
    pub fn expire(&mut self, now: DateTime<Utc>) -> bool {
        self.transition(OpportunityStatus::Expired, now)
    }

    /// 标记为已移除。
    pub fn remove(&mut self, now: DateTime<Utc>) -> bool {
        self.transition(OpportunityStatus::Removed, now)
    }

    /// 检查状态转换是否合法。
    fn is_valid_transition(from: OpportunityStatus, to: OpportunityStatus) -> bool {
        use OpportunityStatus::*;
        matches!(
            (from, to),
            (Created, Updated)
                | (Created, Stable)
                | (Created, Weak)
                | (Created, Expired)
                | (Updated, Stable)
                | (Updated, Weak)
                | (Updated, Expired)
                | (Stable, Updated)
                | (Stable, Weak)
                | (Stable, Expired)
                | (Weak, Updated)
                | (Weak, Stable)
                | (Weak, Expired)
                | (Expired, Removed)
        )
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "状态={} | 变更次数={} | 存活={}秒",
            self.current.as_zh(),
            self.transition_count,
            self.total_duration_secs,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn new_lifecycle_is_created() {
        let lc = Lifecycle::new(now());
        assert_eq!(lc.current, OpportunityStatus::Created);
        assert_eq!(lc.transition_count, 0);
    }

    #[test]
    fn valid_transition_created_to_updated() {
        let mut lc = Lifecycle::new(now());
        assert!(lc.transition(OpportunityStatus::Updated, now()));
        assert_eq!(lc.current, OpportunityStatus::Updated);
        assert_eq!(lc.transition_count, 1);
    }

    #[test]
    fn idempotent_same_state() {
        let mut lc = Lifecycle::new(now());
        assert!(!lc.transition(OpportunityStatus::Created, now()));
        assert_eq!(lc.transition_count, 0);
    }

    #[test]
    fn invalid_transition_skipped() {
        let mut lc = Lifecycle::new(now());
        // Expired → Created 不合法
        lc.transition(OpportunityStatus::Expired, now());
        assert!(!lc.transition(OpportunityStatus::Created, now()));
        assert_eq!(lc.current, OpportunityStatus::Expired);
    }

    #[test]
    fn auto_transition_weak_when_low_score() {
        let mut lc = Lifecycle::new(now());
        lc.auto_transition(5, 10.0, 0.5, false, now());
        assert_eq!(lc.current, OpportunityStatus::Weak);
    }

    #[test]
    fn auto_transition_stable_when_good() {
        let mut lc = Lifecycle::new(now());
        // 先转为 Updated
        lc.transition(OpportunityStatus::Updated, now());
        // 再自动判定
        lc.auto_transition(5, 80.0, 0.9, false, now());
        assert_eq!(lc.current, OpportunityStatus::Stable);
    }

    #[test]
    fn expire_then_remove_is_valid() {
        let mut lc = Lifecycle::new(now());
        assert!(lc.expire(now()));
        assert_eq!(lc.current, OpportunityStatus::Expired);
        assert!(lc.remove(now()));
        assert_eq!(lc.current, OpportunityStatus::Removed);
        assert_eq!(lc.transition_count, 2);
    }

    #[test]
    fn summary_zh_contains_state() {
        let lc = Lifecycle::new(now());
        let s = lc.summary_zh();
        assert!(s.contains("新建"));
        assert!(s.contains("存活"));
    }
}
