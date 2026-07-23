//! 优先队列（V1.04 第六节）。
//!
//! 按 Score 排序的优先队列，支持 Top-N 容量限制。
//! Score 高的排前面，队列满时淘汰最低分。

use crate::model::Opportunity;

/// 机会优先队列：按 Score 降序排列，容量满时自动淘汰最低分。
///
/// 内部使用 Vec + sort（N 很小，不需要 BinaryHeap 的复杂度）。
#[derive(Debug, Clone)]
pub struct OpportunityQueue {
    /// 按 Score 降序排列（最高分在前）。
    items: Vec<Opportunity>,
    /// 最大容量（0 表示无限制）。
    capacity: usize,
}

impl OpportunityQueue {
    /// 创建新的优先队列。
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity.min(256)),
            capacity,
        }
    }

    /// 默认容量（100）。
    pub fn default_capacity() -> usize {
        100
    }

    /// 当前队列中机会数量。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 队列是否为空。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 插入一个机会（自动按 Score 排序，超容量时淘汰最低分）。
    ///
    /// 返回：被淘汰的机会（如果有）。
    pub fn push(&mut self, opp: Opportunity) -> Option<Opportunity> {
        self.items.push(opp);
        // 按 Score 降序排列（NaN 排最后）
        self.items.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if self.capacity > 0 && self.items.len() > self.capacity {
            Some(self.items.pop().unwrap()) // 移除最低分（在末尾）
        } else {
            None
        }
    }

    /// 批量插入机会。
    pub fn extend(&mut self, opps: Vec<Opportunity>) {
        for opp in opps {
            self.push(opp);
        }
    }

    /// 获取 Top-N 机会（按 Score 降序）。
    pub fn top_n(&self, n: usize) -> &[Opportunity] {
        let end = n.min(self.items.len());
        &self.items[..end]
    }

    /// 获取所有机会的不可变引用（已按 Score 降序）。
    pub fn all(&self) -> &[Opportunity] {
        &self.items
    }

    /// 获取最高分机会。
    pub fn best(&self) -> Option<&Opportunity> {
        self.items.first()
    }

    /// 按优先级筛选（priority ≥ min_priority）。
    pub fn filter_by_priority(&self, min_priority: u8) -> Vec<&Opportunity> {
        self.items
            .iter()
            .filter(|o| o.priority >= min_priority)
            .collect()
    }

    /// 按类型筛选。
    pub fn filter_by_type(&self, t: crate::model::OpportunityType) -> Vec<&Opportunity> {
        self.items
            .iter()
            .filter(|o| o.opportunity_type == t)
            .collect()
    }

    /// 清空队列。
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// 消耗队列，返回按 Score 降序的 Vec。
    pub fn into_sorted(self) -> Vec<Opportunity> {
        self.items
    }
}

impl Default for OpportunityQueue {
    fn default() -> Self {
        Self::new(Self::default_capacity())
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

    fn make_opp(id: &str, score: f64, priority: u8) -> Opportunity {
        let now = Utc::now();
        Opportunity::new(
            id.into(),
            format!("Q_{id}"),
            "test".into(),
            now,
            OpportunityType::Unknown,
            score,
            0.8,
            priority,
            score * 0.25, score * 0.20, score * 0.20, score * 0.15, score * 0.10, score * 0.10,
            0.01, 1.0,
            0.5, 0.5, 1.0,
            None, 1000.0, 2000.0,
            None, None,
        )
    }

    #[test]
    fn push_sorts_by_score_desc() {
        let mut q = OpportunityQueue::new(10);
        q.push(make_opp("A", 50.0, 50));
        q.push(make_opp("B", 90.0, 90));
        q.push(make_opp("C", 30.0, 30));
        assert_eq!(q.all()[0].score, 90.0);
        assert_eq!(q.all()[1].score, 50.0);
        assert_eq!(q.all()[2].score, 30.0);
    }

    #[test]
    fn capacity_evicts_lowest() {
        let mut q = OpportunityQueue::new(3);
        q.push(make_opp("A", 90.0, 90));
        q.push(make_opp("B", 50.0, 50));
        q.push(make_opp("C", 70.0, 70));
        assert_eq!(q.len(), 3);
        // 插入第四个（最高分），应淘汰最低分 B(50)
        let evicted = q.push(make_opp("D", 95.0, 95));
        assert_eq!(q.len(), 3);
        assert!(evicted.is_some());
        assert!((evicted.unwrap().score - 50.0).abs() < 0.01);
    }

    #[test]
    fn top_n_returns_correct_count() {
        let mut q = OpportunityQueue::new(10);
        q.push(make_opp("A", 90.0, 90));
        q.push(make_opp("B", 80.0, 80));
        q.push(make_opp("C", 70.0, 70));
        assert_eq!(q.top_n(2).len(), 2);
        assert_eq!(q.top_n(5).len(), 3); // 只有 3 个
    }

    #[test]
    fn best_returns_highest_score() {
        let mut q = OpportunityQueue::new(10);
        q.push(make_opp("A", 50.0, 50));
        q.push(make_opp("B", 95.0, 95));
        assert_eq!(q.best().unwrap().score, 95.0);
    }

    #[test]
    fn filter_by_priority() {
        let mut q = OpportunityQueue::new(10);
        q.push(make_opp("A", 90.0, 90));
        q.push(make_opp("B", 70.0, 70));
        q.push(make_opp("C", 50.0, 50));
        let high = q.filter_by_priority(80);
        assert_eq!(high.len(), 1);
        assert_eq!(high[0].market_id, "A");
    }

    #[test]
    fn empty_queue_is_empty() {
        let q = OpportunityQueue::new(10);
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert!(q.best().is_none());
    }

    #[test]
    fn clear_empties_queue() {
        let mut q = OpportunityQueue::new(10);
        q.push(make_opp("A", 90.0, 90));
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn unlimited_capacity() {
        let mut q = OpportunityQueue::new(0); // 0 = 无限制
        for i in 0..200 {
            q.push(make_opp(&format!("X{i}"), (i as f64) % 100.0, (i as u8) % 100));
        }
        assert_eq!(q.len(), 200);
    }
}
