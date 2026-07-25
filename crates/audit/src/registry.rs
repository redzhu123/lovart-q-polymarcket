//! 通用内存注册表（DataRegistry）。
//!
//! 提供带序号的 HashMap 包装，支持插入、查询、按条件计数。
//! 用于存储 Candidate / Opportunity 等生命周期间的实体。

use std::collections::HashMap;
use std::fmt::Debug;

/// 可识别实体 trait —— 注册表中的元素必须实现。
pub trait Identifiable {
    fn id(&self) -> &str;
}

/// 通用内存注册表。
///
/// 维护 `HashMap<Id, T>` 与插入顺序列表，支持：
/// - 插入（重复 ID 返回旧值）
/// - 按 ID 查询
/// - 总数
/// - 按谓词计数
/// - 全量迭代
#[derive(Debug, Clone)]
pub struct DataRegistry<T: Identifiable> {
    items: HashMap<String, T>,
    insertion_order: Vec<String>,
}

impl<T: Identifiable> DataRegistry<T> {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    /// 插入一项。若 ID 已存在，返回旧值。
    pub fn insert(&mut self, item: T) -> Option<T> {
        let id = item.id().to_string();
        let old = self.items.insert(id.clone(), item);
        if old.is_none() {
            self.insertion_order.push(id);
        }
        old
    }

    /// 按 ID 查询。
    pub fn get(&self, id: &str) -> Option<&T> {
        self.items.get(id)
    }

    /// 按 ID 移除。
    pub fn remove(&mut self, id: &str) -> Option<T> {
        let old = self.items.remove(id);
        if old.is_some() {
            self.insertion_order.retain(|k| k != id);
        }
        old
    }

    /// 注册表中元素总数。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 按谓词统计数量。
    pub fn count_by<P: Fn(&T) -> bool>(&self, predicate: P) -> u64 {
        self.items.values().filter(|v| predicate(v)).count() as u64
    }

    /// 返回所有元素（按插入顺序）。
    pub fn all(&self) -> Vec<&T> {
        self.insertion_order
            .iter()
            .filter_map(|id| self.items.get(id))
            .collect()
    }

    /// 取出所有元素，清空注册表（按插入顺序）。
    pub fn drain_all(&mut self) -> Vec<T> {
        let mut result: Vec<T> = Vec::new();
        for id in &self.insertion_order {
            if let Some(item) = self.items.remove(id) {
                result.push(item);
            }
        }
        self.insertion_order.clear();
        result
    }

    /// 清空注册表。
    pub fn clear(&mut self) {
        self.items.clear();
        self.insertion_order.clear();
    }

    /// 返回插入顺序中的 ID 列表。
    pub fn ids(&self) -> &[String] {
        &self.insertion_order
    }
}

impl<T: Identifiable> Default for DataRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestItem {
        id: String,
        value: i32,
    }

    impl Identifiable for TestItem {
        fn id(&self) -> &str {
            &self.id
        }
    }

    fn item(id: &str, value: i32) -> TestItem {
        TestItem {
            id: id.to_string(),
            value,
        }
    }

    #[test]
    fn insert_and_get() {
        let mut reg = DataRegistry::new();
        reg.insert(item("a", 1));
        assert_eq!(reg.get("a").unwrap().value, 1);
        assert!(reg.get("b").is_none());
    }

    #[test]
    fn duplicate_id_replaces() {
        let mut reg = DataRegistry::new();
        assert!(reg.insert(item("a", 1)).is_none());
        let old = reg.insert(item("a", 2));
        assert_eq!(old.unwrap().value, 1);
        assert_eq!(reg.get("a").unwrap().value, 2);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn count_by_predicate() {
        let mut reg = DataRegistry::new();
        reg.insert(item("a", 10));
        reg.insert(item("b", 20));
        reg.insert(item("c", 30));
        assert_eq!(reg.count_by(|t| t.value > 15), 2);
        assert_eq!(reg.count_by(|t| t.value < 5), 0);
        assert_eq!(reg.count_by(|_| true), 3);
    }

    #[test]
    fn all_returns_insertion_order() {
        let mut reg = DataRegistry::new();
        reg.insert(item("c", 3));
        reg.insert(item("a", 1));
        reg.insert(item("b", 2));
        let all = reg.all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "c");
        assert_eq!(all[1].id, "a");
        assert_eq!(all[2].id, "b");
    }

    #[test]
    fn remove_works() {
        let mut reg = DataRegistry::new();
        reg.insert(item("a", 1));
        reg.insert(item("b", 2));
        assert_eq!(reg.remove("a").unwrap().value, 1);
        assert_eq!(reg.len(), 1);
        assert!(reg.get("a").is_none());
        assert!(reg.get("b").is_some());
    }

    #[test]
    fn drain_all_clears() {
        let mut reg = DataRegistry::new();
        reg.insert(item("a", 1));
        reg.insert(item("b", 2));
        let drained = reg.drain_all();
        assert_eq!(drained.len(), 2);
        assert!(reg.is_empty());
    }
}
