//! CsvRepository：CSV 持久化（P2-04 第八节）。
//!
//! 使用 [`pm_storage`] 提供的 ensure_csv/append_records 工具。
//! 表头固定，禁止修改列顺序。
//!
//! ## 列定义
//!
//! orders.csv: order_id, client_order_id, exchange_order_id, gateway_name,
//!             market_id, provider, direction, side, price, quantity,
//!             order_type, time_in_force, status, filled, remaining,
//!             avg_fill_price, slippage, created_at, updated_at,
//!             strategy_id, risk_id, opportunity_id, version, retry_count,
//!             priority, notes, simulation_only
//!
//! events.csv: timestamp, event_type, event_name_zh, order_id, extra_json
//!
//! status_changes.csv: order_id, from_status, to_status, at, reason, actor

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Local, TimeZone};

use crate::events::{event_to_csv_row, OrderEvent, OMS_EVENTS_HEADER};
use crate::order::{Order, OrderStatus, StatusChange};

use super::{OrderRepository, RepositoryHealth};

// ============================================================================
// 表头常量
// ============================================================================

const ORDERS_HEADER: &[&str] = &[
    "order_id",
    "client_order_id",
    "exchange_order_id",
    "gateway_name",
    "market_id",
    "provider",
    "direction",
    "side",
    "price",
    "quantity",
    "order_type",
    "time_in_force",
    "status",
    "filled",
    "remaining",
    "avg_fill_price",
    "slippage",
    "created_at",
    "updated_at",
    "strategy_id",
    "risk_id",
    "opportunity_id",
    "version",
    "retry_count",
    "priority",
    "notes",
    "simulation_only",
];

const STATUS_CHANGES_HEADER: &[&str] = &[
    "order_id",
    "from_status",
    "to_status",
    "at",
    "reason",
    "actor",
];

// ============================================================================
// CsvRepository
// ============================================================================

/// CSV 持久化仓库。
///
/// 写策略：append-only。
/// 读策略：全量加载到内存（订单量级 < 10K，性能可接受）。
pub struct CsvRepository {
    orders_path: PathBuf,
    events_path: PathBuf,
    status_changes_path: PathBuf,
    /// 内存缓存：order_id → Order
    orders: Mutex<HashMap<String, Order>>,
    /// client_order_id → order_id
    client_index: Mutex<HashMap<String, String>>,
    /// 状态变化：order_id → Vec<StatusChange>
    status_changes: Mutex<HashMap<String, Vec<StatusChange>>>,
}

impl CsvRepository {
    pub fn new(orders_path: PathBuf, events_path: PathBuf) -> anyhow::Result<Self> {
        let status_changes_path = orders_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("oms_status_changes.csv");
        let repo = Self {
            orders_path,
            events_path,
            status_changes_path,
            orders: Mutex::new(HashMap::new()),
            client_index: Mutex::new(HashMap::new()),
            status_changes: Mutex::new(HashMap::new()),
        };
        repo.ensure_files()?;
        repo.load_into_memory()?;
        Ok(repo)
    }

    /// 确保所有 CSV 文件就绪。
    fn ensure_files(&self) -> anyhow::Result<()> {
        pm_storage::ensure_csv(&self.orders_path, ORDERS_HEADER)?;
        pm_storage::ensure_csv(&self.events_path, OMS_EVENTS_HEADER)?;
        pm_storage::ensure_csv(&self.status_changes_path, STATUS_CHANGES_HEADER)?;
        Ok(())
    }

    /// 启动时加载已有 CSV 到内存。
    fn load_into_memory(&self) -> anyhow::Result<()> {
        // 加载 orders
        if self.orders_path.exists() {
            let mut rdr = csv::Reader::from_path(&self.orders_path)?;
            let mut orders = self.orders.lock().unwrap();
            let mut client_idx = self.client_index.lock().unwrap();
            for record in rdr.deserialize() {
                let record: OrderRecord = record?;
                let order = record.into_order();
                client_idx.insert(order.client_order_id.clone(), order.order_id.clone());
                orders.insert(order.order_id.clone(), order);
            }
        }
        // 加载 status_changes
        if self.status_changes_path.exists() {
            let mut rdr = csv::Reader::from_path(&self.status_changes_path)?;
            let mut sc = self.status_changes.lock().unwrap();
            for record in rdr.deserialize() {
                let record: StatusChangeRecord = record?;
                sc.entry(record.order_id.clone())
                    .or_insert_with(Vec::new)
                    .push(record.into_change());
            }
        }
        Ok(())
    }

    /// 追加一行 order 到 CSV。
    fn append_order(&self, order: &Order) -> anyhow::Result<()> {
        let mut wtr = csv::Writer::from_path(&self.orders_path)?;
        wtr.write_record(ORDERS_HEADER)?;
        wtr.write_record(&order_to_csv_row(order))?;
        wtr.flush()?;
        // 注意：上面用 from_path 会覆盖。为 append 行为，改用 OpenOptions
        Ok(())
    }

    /// 真实追加（OpenOptions append）。
    fn append_order_append(&self, order: &Order) -> anyhow::Result<()> {
        use std::fs::OpenOptions;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.orders_path)?;
        let mut wtr = csv::Writer::from_writer(file);
        wtr.write_record(&order_to_csv_row(order))?;
        wtr.flush()?;
        Ok(())
    }
}

impl OrderRepository for CsvRepository {
    fn save(&self, order: &Order) -> anyhow::Result<()> {
        let mut orders = self.orders.lock().unwrap();
        let mut client_idx = self.client_index.lock().unwrap();
        client_idx.insert(order.client_order_id.clone(), order.order_id.clone());
        let existed = orders.contains_key(&order.order_id);
        orders.insert(order.order_id.clone(), order.clone());
        drop(orders);
        drop(client_idx);
        if !existed {
            self.append_order_append(order)?;
        } else {
            // CSV 不支持就地更新：再追加一行（外部分析时按 version 取最大）
            self.append_order_append(order)?;
        }
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
            // CSV 不就地删除；下次 load 时过滤（仅内存删除）
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
        drop(sc);

        use std::fs::OpenOptions;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.status_changes_path)?;
        let mut wtr = csv::Writer::from_writer(file);
        let record = StatusChangeRecord {
            order_id: order_id.to_string(),
            from_status: change.from.as_str().to_string(),
            to_status: change.to.as_str().to_string(),
            at: change.at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            reason: change.reason.clone(),
            actor: change.actor.clone(),
        };
        wtr.serialize(record)?;
        wtr.flush()?;
        Ok(())
    }

    fn list_status_changes(&self, order_id: &str) -> anyhow::Result<Vec<StatusChange>> {
        let sc = self.status_changes.lock().unwrap();
        Ok(sc.get(order_id).cloned().unwrap_or_default())
    }

    fn append_event(&self, event: &OrderEvent) -> anyhow::Result<()> {
        use std::fs::OpenOptions;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.events_path)?;
        let mut wtr = csv::Writer::from_writer(file);
        let row = event_to_csv_row(event);
        wtr.write_record(&row)?;
        wtr.flush()?;
        Ok(())
    }

    fn list_events(&self) -> anyhow::Result<Vec<OrderEvent>> {
        if !self.events_path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::Reader::from_path(&self.events_path)?;
        let mut out = Vec::new();
        for record in rdr.records() {
            let r = record?;
            if r.len() < 5 {
                continue;
            }
            let extra_json = r.get(4).unwrap_or("{}");
            if let Ok(ev) = serde_json::from_str::<OrderEvent>(extra_json) {
                out.push(ev);
            }
        }
        Ok(out)
    }

    fn name(&self) -> &str {
        "CsvRepository"
    }

    fn storage_path(&self) -> Option<PathBuf> {
        Some(self.orders_path.clone())
    }

    fn health(&self) -> RepositoryHealth {
        let orders = self.orders.lock().unwrap().len() as u64;
        let events = self.list_events().map(|v| v.len() as u64).unwrap_or(0);
        RepositoryHealth::ok(orders, events)
    }
}

// ============================================================================
// 内部 CSV 序列化
// ============================================================================

fn order_to_csv_row(o: &Order) -> [String; 27] {
    [
        o.order_id.clone(),
        o.client_order_id.clone(),
        o.exchange_order_id.clone().unwrap_or_default(),
        o.gateway_name.clone(),
        o.market_id.clone(),
        o.provider.clone(),
        o.direction.as_zh().to_string(),
        o.side.as_str().to_string(),
        fmt_f64(o.price),
        fmt_f64(o.quantity),
        o.order_type.as_zh().to_string(),
        o.time_in_force.as_zh().to_string(),
        o.status.as_zh().to_string(),
        fmt_f64(o.filled),
        fmt_f64(o.remaining),
        fmt_f64(o.avg_fill_price),
        fmt_f64(o.slippage),
        o.created_at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        o.updated_at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        o.strategy_id.clone(),
        o.risk_id.clone(),
        o.opportunity_id.clone(),
        o.version.to_string(),
        o.retry_count.to_string(),
        o.priority.to_string(),
        o.notes.clone(),
        if o.simulation_only { "true".into() } else { "false".into() },
    ]
}

fn fmt_f64(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 { "Inf".into() } else { "-Inf".into() }
    } else {
        format!("{:.8}", v)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct OrderRecord {
    order_id: String,
    client_order_id: String,
    exchange_order_id: String,
    gateway_name: String,
    market_id: String,
    provider: String,
    direction: String,
    side: String,
    price: f64,
    quantity: f64,
    order_type: String,
    time_in_force: String,
    status: String,
    filled: f64,
    remaining: f64,
    avg_fill_price: f64,
    slippage: f64,
    created_at: String,
    updated_at: String,
    strategy_id: String,
    risk_id: String,
    opportunity_id: String,
    version: u32,
    retry_count: u32,
    priority: u32,
    notes: String,
    simulation_only: String,
}

impl OrderRecord {
    fn into_order(self) -> Order {
        let direction = match self.direction.as_str() {
            "YES" | "Yes" => pm_execution::order::Direction::Yes,
            _ => pm_execution::order::Direction::No,
        };
        let side = match self.side.as_str() {
            "SELL" | "Sell" => pm_core::Side::Sell,
            _ => pm_core::Side::Buy,
        };
        let order_type = match self.order_type.as_str() {
            "市价" => pm_gateway::OrderType::Market,
            _ => pm_gateway::OrderType::Limit,
        };
        let time_in_force = match self.time_in_force.as_str() {
            "立即成交或取消" => pm_gateway::TimeInForce::Ioc,
            "全部成交或取消" => pm_gateway::TimeInForce::Fok,
            _ => pm_gateway::TimeInForce::Gtc,
        };
        let status = match self.status.as_str() {
            "已创建" => OrderStatus::Created,
            "已校验" => OrderStatus::Validated,
            "待提交" => OrderStatus::PendingSubmit,
            "已提交" => OrderStatus::Submitted,
            "已接受" => OrderStatus::Accepted,
            "部分成交" => OrderStatus::PartiallyFilled,
            "完全成交" => OrderStatus::Filled,
            "已取消" => OrderStatus::Cancelled,
            "已拒绝" => OrderStatus::Rejected,
            "已过期" => OrderStatus::Expired,
            _ => OrderStatus::Completed,
        };
        Order {
            order_id: self.order_id,
            client_order_id: self.client_order_id,
            exchange_order_id: if self.exchange_order_id.is_empty() {
                None
            } else {
                Some(self.exchange_order_id)
            },
            gateway_name: self.gateway_name,
            market_id: self.market_id,
            provider: self.provider,
            direction,
            side,
            price: self.price,
            quantity: self.quantity,
            order_type,
            time_in_force,
            status,
            filled: self.filled,
            remaining: self.remaining,
            avg_fill_price: self.avg_fill_price,
            slippage: self.slippage,
            created_at: parse_dt(&self.created_at),
            updated_at: parse_dt(&self.updated_at),
            strategy_id: self.strategy_id,
            risk_id: self.risk_id,
            opportunity_id: self.opportunity_id,
            status_history: Vec::new(), // 由 status_changes.csv 恢复
            version: self.version,
            retry_count: self.retry_count,
            priority: self.priority,
            notes: self.notes,
            simulation_only: self.simulation_only == "true",
        }
    }
}

fn parse_dt(s: &str) -> DateTime<Local> {
    use chrono::NaiveDateTime;
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f") {
        return Local.from_local_datetime(&naive).unwrap();
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Local.from_local_datetime(&naive).unwrap();
    }
    Local::now()
}

#[derive(serde::Deserialize, serde::Serialize)]
struct StatusChangeRecord {
    order_id: String,
    from_status: String,
    to_status: String,
    at: String,
    reason: String,
    actor: String,
}

impl StatusChangeRecord {
    fn into_change(self) -> StatusChange {
        let from = parse_status(&self.from_status);
        let to = parse_status(&self.to_status);
        let at = parse_dt(&self.at);
        StatusChange {
            from,
            to,
            at,
            reason: self.reason,
            actor: self.actor,
        }
    }
}

fn parse_status(s: &str) -> OrderStatus {
    match s {
        "Created" => OrderStatus::Created,
        "Validated" => OrderStatus::Validated,
        "PendingSubmit" => OrderStatus::PendingSubmit,
        "Submitted" => OrderStatus::Submitted,
        "Accepted" => OrderStatus::Accepted,
        "PartiallyFilled" => OrderStatus::PartiallyFilled,
        "Filled" => OrderStatus::Filled,
        "Cancelled" => OrderStatus::Cancelled,
        "Rejected" => OrderStatus::Rejected,
        "Expired" => OrderStatus::Expired,
        _ => OrderStatus::Completed,
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
    use tempfile::TempDir;

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

    fn make_repo() -> (CsvRepository, TempDir) {
        let dir = TempDir::new().unwrap();
        let orders = dir.path().join("orders.csv");
        let events = dir.path().join("events.csv");
        let repo = CsvRepository::new(orders, events).unwrap();
        (repo, dir)
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (repo, _dir) = make_repo();
        let o = build_order("001");
        repo.save(&o).unwrap();
        let loaded = repo.find_by_id(&o.order_id).unwrap().unwrap();
        assert_eq!(loaded.order_id, o.order_id);
        assert_eq!(loaded.client_order_id, o.client_order_id);
        assert!((loaded.price - o.price).abs() < 1e-9);
    }

    #[test]
    fn reload_persists_across_instances() {
        let (repo1, dir) = make_repo();
        let o = build_order("001");
        repo1.save(&o).unwrap();

        let orders_path = dir.path().join("orders.csv");
        let events_path = dir.path().join("events.csv");
        let repo2 = CsvRepository::new(orders_path, events_path).unwrap();
        assert_eq!(repo2.count().unwrap(), 1);
        assert!(repo2.find_by_id(&o.order_id).unwrap().is_some());
    }

    #[test]
    fn list_active_filters_terminal() {
        let (repo, _dir) = make_repo();
        let mut o = build_order("001");
        o.transition(OrderStatus::Filled, "测试", "oms", Local::now());
        repo.save(&o).unwrap();
        let active = repo.list_active().unwrap();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn append_status_change_persists() {
        let (repo, _dir) = make_repo();
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
    }

    #[test]
    fn append_event_and_list_events() {
        let (repo, _dir) = make_repo();
        let ev = OrderEvent::OrderFilled {
            order_id: "OMS-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: Local::now(),
        };
        repo.append_event(&ev).unwrap();
        let events = repo.list_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name(), "OrderFilled");
    }

    #[test]
    fn count_by_status_grouping() {
        let (repo, _dir) = make_repo();
        for i in 0..3 {
            let mut o = build_order(&format!("{:03}", i));
            if i == 0 {
                o.transition(OrderStatus::Validated, "v", "oms", Local::now());
            }
            repo.save(&o).unwrap();
        }
        let counts = repo.count_by_status().unwrap();
        let validated = counts.iter().find(|(s, _)| *s == OrderStatus::Validated).unwrap();
        assert_eq!(validated.1, 1);
    }

    #[test]
    fn health_returns_ok() {
        let (repo, _dir) = make_repo();
        let h = repo.health();
        assert!(h.healthy);
        assert_eq!(repo.name(), "CsvRepository");
        assert!(repo.storage_path().is_some());
    }
}