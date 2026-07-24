//! InMemoryRepository：内存仓库实现（P2-04 第八节）。
//!
//! 适合测试 / 实时运行（不持久化到磁盘）。所有数据存储在 Mutex<HashMap> 中。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Local};

use crate::events::OrderEvent;
use crate::order::{Order, OrderStatus, StatusChange};

use super::{OrderRepository, RepositoryHealth};

// ============================================================================
// InMemoryRepository
// ============================================================================

/// 内存订单仓库。
///
/// 内部使用 `Mutex<HashMap<String, Order>>` 存储订单。
/// 事件追加会被忽略（事件由 EventBus 同步分发，无持久化）。
pub struct InMemoryRepository {
    /// order_id → Order
    orders: Mutex<HashMap<String, Order>>,
    /// client_order_id → order_id
    client_index: Mutex<HashMap<String, String>>,
    /// 事件列表（仅测试使用，实时运行通常不需要）
    events: Mutex<Vec<OrderEvent>>,
    /// 状态变化：order_id → Vec<StatusChange>
    status_changes: Mutex<HashMap<String, Vec<StatusChange>>>,
}

impl Default for InMemoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRepository {
    pub fn new() -> Self {
        Self {
            orders: Mutex::new(HashMap::new()),
            client_index: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            status_changes: Mutex::new(HashMap::new()),
        }
    }

    /// 清空所有数据（仅测试）。
    pub fn clear(&self) {
        self.orders.lock().unwrap().clear();
        self.client_index.lock().unwrap().clear();
        self.events.lock().unwrap().clear();
        self.status_changes.lock().unwrap().clear();
    }
}

impl OrderRepository for InMemoryRepository {
    fn save(&self, order: &Order) -> anyhow::Result<()> {
        let mut orders = self.orders.lock().unwrap();
        let mut client_idx = self.client_index.lock().unwrap();
        client_idx.insert(order.client_order_id.clone(), order.order_id.clone());
        orders.insert(order.order_id.clone(), order.clone());
        Ok(())
    }

    fn find_by_id(&self, order_id: &str) -> anyhow::Result<Option<Order>> {
        let orders = self.orders.lock().unwrap();
        Ok(orders.get(order_id).cloned())
    }

    fn find_by_client_id(&self, client_order_id: &str) -> anyhow::Result<Option<Order>> {
        let client_idx = self.client_index.lock().unwrap();
        let orders = self.orders.lock().unwrap();
        let id = client_idx.get(client_order_id);
        Ok(id.and_then(|oid| orders.get(oid).cloned()))
    }

    fn list_all(&self) -> anyhow::Result<Vec<Order>> {
        let orders = self.orders.lock().unwrap();
        let mut v: Vec<Order> = orders.values().cloned().collect();
        v.sort_by_key(|o| o.created_at);
        Ok(v)
    }

    fn list_by_status(&self, status: OrderStatus) -> anyhow::Result<Vec<Order>> {
        let orders = self.orders.lock().unwrap();
        let mut v: Vec<Order> = orders
            .values()
            .filter(|o| o.status == status)
            .cloned()
            .collect();
        v.sort_by_key(|o| o.created_at);
        Ok(v)
    }

    fn list_active(&self) -> anyhow::Result<Vec<Order>> {
        let orders = self.orders.lock().unwrap();
        let mut v: Vec<Order> = orders
            .values()
            .filter(|o| o.status.is_active())
            .cloned()
            .collect();
        v.sort_by_key(|o| o.created_at);
        Ok(v)
    }

    fn list_in_range(
        &self,
        from: DateTime<Local>,
        to: DateTime<Local>,
    ) -> anyhow::Result<Vec<Order>> {
        let orders = self.orders.lock().unwrap();
        let mut v: Vec<Order> = orders
            .values()
            .filter(|o| o.created_at >= from && o.created_at <= to)
            .cloned()
            .collect();
        v.sort_by_key(|o| o.created_at);
        Ok(v)
    }

    fn count(&self) -> anyhow::Result<u64> {
        Ok(self.orders.lock().unwrap().len() as u64)
    }

    fn count_by_status(&self) -> anyhow::Result<Vec<(OrderStatus, u64)>> {
        let orders = self.orders.lock().unwrap();
        let mut counts: HashMap<OrderStatus, u64> = HashMap::new();
        for o in orders.values() {
            *counts.entry(o.status).or_insert(0) += 1;
        }
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by_key(|(s, _)| *s);
        Ok(v)
    }

    fn delete(&self, order_id: &str) -> anyhow::Result<bool> {
        let mut orders = self.orders.lock().unwrap();
        let removed = orders.remove(order_id).is_some();
        if removed {
            let mut client_idx = self.client_index.lock().unwrap();
            client_idx.retain(|_, v| v != order_id);
        }
        Ok(removed)
    }

    fn append_status_change(
        &self,
        order_id: &str,
        change: &StatusChange,
    ) -> anyhow::Result<()> {
        let mut sc = self.status_changes.lock().unwrap();
        sc.entry(order_id.to_string())
            .or_insert_with(Vec::new)
            .push(change.clone());
        Ok(())
    }

    fn list_status_changes(&self, order_id: &str) -> anyhow::Result<Vec<StatusChange>> {
        let sc = self.status_changes.lock().unwrap();
        Ok(sc.get(order_id).cloned().unwrap_or_default())
    }

    fn append_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    fn list_events(&self) -> anyhow::Result<Vec<OrderEvent>> {
        Ok(self.events.lock().unwrap().clone())
    }

    fn name(&self) -> &str {
        "InMemoryRepository"
    }

    fn storage_path(&self) -> Option<PathBuf> {
        None
    }

    fn health(&self) -> RepositoryHealth {
        let orders = self.orders.lock().unwrap().len() as u64;
        let events = self.events.lock().unwrap().len() as u64;
        RepositoryHealth::ok(orders, events)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::{Direction, Order};
    use chrono::Local;
    use pm_core::Side;
    use pm_gateway::{OrderType, TimeInForce};

    fn build_order(id: &str) -> Order {
        let now = Local::now();
        Order::new(
            format!("CLI-{}", id),
            "mkt-test".into(),
            "mock".into(),
            "MockGateway".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            OrderType::Limit,
            TimeInForce::Gtc,
            "S1".into(),
            "R1".into(),
            "O1".into(),
            now,
        )
    }

    #[test]
    fn save_and_find_by_id() {
        let repo = InMemoryRepository::new();
        let o = build_order("001");
        repo.save(&o).unwrap();
        let found = repo.find_by_id(&o.order_id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().order_id, o.order_id);
    }

    #[test]
    fn find_by_client_id() {
        let repo = InMemoryRepository::new();
        let o = build_order("001");
        let cid = o.client_order_id.clone();
        repo.save(&o).unwrap();
        let found = repo.find_by_client_id(&cid).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().order_id, o.order_id);
    }

    #[test]
    fn list_all_sorted_by_created() {
        let repo = InMemoryRepository::new();
        for i in 0..3 {
            let mut o = build_order(&format!("{:03}", i));
            o.created_at = Local::now() + chrono::Duration::seconds(i);
            repo.save(&o).unwrap();
        }
        let all = repo.list_all().unwrap();
        assert_eq!(all.len(), 3);
        assert!(all[0].created_at <= all[1].created_at);
    }

    #[test]
    fn list_by_status_filter() {
        let repo = InMemoryRepository::new();
        let mut o1 = build_order("001");
        let o2 = build_order("002");
        o1.transition(OrderStatus::Validated, "测试", "oms", Local::now());
        repo.save(&o1).unwrap();
        repo.save(&o2).unwrap();
        let validated = repo.list_by_status(OrderStatus::Validated).unwrap();
        assert_eq!(validated.len(), 1);
        let created = repo.list_by_status(OrderStatus::Created).unwrap();
        assert_eq!(created.len(), 1);
    }

    #[test]
    fn list_active_excludes_terminal() {
        let repo = InMemoryRepository::new();
        let mut o = build_order("001");
        o.transition(OrderStatus::Validated, "v", "oms", Local::now());
        o.transition(OrderStatus::Filled, "f", "oms", Local::now());
        repo.save(&o).unwrap();
        let active = repo.list_active().unwrap();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn list_in_range() {
        let repo = InMemoryRepository::new();
        let now = Local::now();
        let mut o = build_order("001");
        o.created_at = now;
        repo.save(&o).unwrap();
        let from = now - chrono::Duration::hours(1);
        let to = now + chrono::Duration::hours(1);
        let in_range = repo.list_in_range(from, to).unwrap();
        assert_eq!(in_range.len(), 1);
    }

    #[test]
    fn count_and_count_by_status() {
        let repo = InMemoryRepository::new();
        for i in 0..5 {
            repo.save(&build_order(&format!("{:03}", i))).unwrap();
        }
        assert_eq!(repo.count().unwrap(), 5);
        let counts = repo.count_by_status().unwrap();
        assert!(counts.iter().any(|(s, c)| *s == OrderStatus::Created && *c == 5));
    }

    #[test]
    fn delete_removes_order() {
        let repo = InMemoryRepository::new();
        let o = build_order("001");
        let id = o.order_id.clone();
        let cid = o.client_order_id.clone();
        repo.save(&o).unwrap();
        assert!(repo.delete(&id).unwrap());
        assert!(repo.find_by_id(&id).unwrap().is_none());
        assert!(repo.find_by_client_id(&cid).unwrap().is_none());
    }

    #[test]
    fn status_changes_append_and_list() {
        let repo = InMemoryRepository::new();
        let id = "OMS-001";
        let change = StatusChange::new(
            OrderStatus::Created,
            OrderStatus::Validated,
            "校验通过",
            "validator",
            Local::now(),
        );
        repo.append_status_change(id, &change).unwrap();
        let list = repo.list_status_changes(id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].to, OrderStatus::Validated);
    }

    #[test]
    fn events_append_and_list() {
        let repo = InMemoryRepository::new();
        let event = OrderEvent::OrderCreated {
            order_id: "OMS-001".into(),
            client_order_id: "C1".into(),
            market_id: "mkt-1".into(),
            timestamp: Local::now(),
        };
        repo.append_event(&event).unwrap();
        let events = repo.list_events().unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn health_and_metadata() {
        let repo = InMemoryRepository::new();
        repo.save(&build_order("001")).unwrap();
        let h = repo.health();
        assert!(h.healthy);
        assert_eq!(h.order_count, 1);
        assert_eq!(repo.name(), "InMemoryRepository");
        assert!(repo.storage_path().is_none());
    }

    #[test]
    fn clear_resets() {
        let repo = InMemoryRepository::new();
        repo.save(&build_order("001")).unwrap();
        repo.clear();
        assert_eq!(repo.count().unwrap(), 0);
    }
}