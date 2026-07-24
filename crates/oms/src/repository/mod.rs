//! OMS Repository（P2-04 第八节）。
//!
//! 订单持久化层。统一抽象为 [`OrderRepository`] trait。
//!
//! ## 实现
//!
//! - [`memory::InMemoryRepository`]：内存存储（默认，测试 / 实时运行）。
//! - [`csv::CsvRepository`]：CSV 持久化（轻量，便于回放 / 报表）。
//! - [`sqlite::SqliteRepository`]：SQLite 接口预留（P2-04 后阶段实现）。
//!
//! ## 设计原则
//!
//! - **接口优先**：所有持久化逻辑通过 trait 暴露，便于替换。
//! - **CSV 表头固定**：列顺序不可改，便于外部工具读取。
//! - **append-only**：CSV 仅追加新订单，不就地更新；更新通过追加新状态变更记录。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use crate::order::{Order, OrderStatus, StatusChange};
use chrono::{DateTime, Local};
use std::path::PathBuf;

pub mod csv;
pub mod memory;
pub mod sqlite;

// ============================================================================
// OrderRepository Trait
// ============================================================================

/// OMS 订单仓库接口。
///
/// 所有持久化实现必须实现本 trait。
/// OMS 业务层仅依赖本 trait，不直接使用具体实现。
pub trait OrderRepository: Send + Sync {
    // ---- 订单 CRUD ----

    /// 保存 / 插入新订单（若 order_id 已存在则覆盖）。
    fn save(&self, order: &Order) -> anyhow::Result<()>;

    /// 通过 order_id 查找。
    fn find_by_id(&self, order_id: &str) -> anyhow::Result<Option<Order>>;

    /// 通过 client_order_id 查找。
    fn find_by_client_id(&self, client_order_id: &str) -> anyhow::Result<Option<Order>>;

    /// 列出所有订单（按 created_at 升序）。
    fn list_all(&self) -> anyhow::Result<Vec<Order>>;

    /// 按状态过滤。
    fn list_by_status(&self, status: OrderStatus) -> anyhow::Result<Vec<Order>>;

    /// 列出活跃订单（is_active = true）。
    fn list_active(&self) -> anyhow::Result<Vec<Order>>;

    /// 按时间范围过滤（created_at ∈ [from, to]）。
    fn list_in_range(&self, from: DateTime<Local>, to: DateTime<Local>) -> anyhow::Result<Vec<Order>>;

    /// 统计总订单数。
    fn count(&self) -> anyhow::Result<u64>;

    /// 统计各状态订单数。
    fn count_by_status(&self) -> anyhow::Result<Vec<(OrderStatus, u64)>>;

    /// 删除订单（仅用于测试 / 维护）。
    fn delete(&self, order_id: &str) -> anyhow::Result<bool>;

    // ---- 状态变化追加 ----

    /// 追加状态变化记录。
    fn append_status_change(&self, order_id: &str, change: &StatusChange) -> anyhow::Result<()>;

    /// 列出订单的所有状态变化。
    fn list_status_changes(&self, order_id: &str) -> anyhow::Result<Vec<StatusChange>>;

    // ---- 事件追加（仅 CSV / SQLite 实现，InMemory 可忽略）----

    /// 追加 OMS 事件（CSV / SQLite 持久化；InMemory 通常忽略）。
    fn append_event(&self, event: &crate::events::OrderEvent) -> anyhow::Result<()>;

    /// 列出所有 OMS 事件。
    fn list_events(&self) -> anyhow::Result<Vec<crate::events::OrderEvent>>;

    // ---- 元信息 ----

    /// 仓库名称。
    fn name(&self) -> &str;

    /// 当前存储路径（如适用）。
    fn storage_path(&self) -> Option<PathBuf>;

    /// 仓库健康检查。
    fn health(&self) -> RepositoryHealth;
}

/// 仓库健康状态。
#[derive(Debug, Clone)]
pub struct RepositoryHealth {
    pub healthy: bool,
    pub message: String,
    pub order_count: u64,
    pub event_count: u64,
}

impl RepositoryHealth {
    pub fn ok(order_count: u64, event_count: u64) -> Self {
        Self {
            healthy: true,
            message: "健康".to_string(),
            order_count,
            event_count,
        }
    }

    pub fn unhealthy(msg: &str) -> Self {
        Self {
            healthy: false,
            message: msg.to_string(),
            order_count: 0,
            event_count: 0,
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        if self.healthy {
            format!(
                "【{}】{} | 订单 {} | 事件 {}",
                self.message, "健康", self.order_count, self.event_count
            )
        } else {
            format!("【异常】{} | 订单 {} | 事件 {}", self.message, self.order_count, self.event_count)
        }
    }
}

// ============================================================================
// RepositoryType — 选择仓库实现
// ============================================================================

/// 仓库类型枚举（用于配置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RepositoryType {
    /// 内存（默认，测试 / 实时）。
    Memory,
    /// CSV 持久化。
    Csv,
    /// SQLite（接口预留，尚未实现）。
    Sqlite,
}

impl RepositoryType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RepositoryType::Memory => "内存",
            RepositoryType::Csv => "CSV",
            RepositoryType::Sqlite => "SQLite",
        }
    }
}

impl Default for RepositoryType {
    fn default() -> Self {
        RepositoryType::Memory
    }
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 根据类型创建仓库实例。
///
/// - `Memory` → `InMemoryRepository`
/// - `Csv`    → `CsvRepository`（`orders_csv` + `events_csv` 路径）
/// - `Sqlite` → 返回 `SqliteRepository`（待实现）
pub fn create_repository(
    repo_type: RepositoryType,
    orders_csv: Option<PathBuf>,
    events_csv: Option<PathBuf>,
    sqlite_path: Option<PathBuf>,
) -> anyhow::Result<Box<dyn OrderRepository>> {
    match repo_type {
        RepositoryType::Memory => Ok(Box::new(memory::InMemoryRepository::new())),
        RepositoryType::Csv => {
            let orders = orders_csv.unwrap_or_else(|| PathBuf::from("data/oms_orders.csv"));
            let events = events_csv.unwrap_or_else(|| PathBuf::from("data/oms_events.csv"));
            let repo = csv::CsvRepository::new(orders, events)?;
            Ok(Box::new(repo))
        }
        RepositoryType::Sqlite => {
            let path = sqlite_path.unwrap_or_else(|| PathBuf::from("data/oms.sqlite"));
            let repo = sqlite::SqliteRepository::new(path);
            Ok(Box::new(repo))
        }
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
    fn repository_type_zh() {
        assert_eq!(RepositoryType::Memory.as_zh(), "内存");
        assert_eq!(RepositoryType::Csv.as_zh(), "CSV");
        assert_eq!(RepositoryType::Sqlite.as_zh(), "SQLite");
    }

    #[test]
    fn factory_creates_memory_by_default() {
        let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
        assert_eq!(repo.name(), "InMemoryRepository");
        assert_eq!(repo.count().unwrap(), 0);
    }

    #[test]
    fn health_summary_chinese() {
        let h = RepositoryHealth::ok(10, 20);
        assert!(h.summary_zh().contains("10"));
        let h2 = RepositoryHealth::unhealthy("数据库异常");
        assert!(h2.summary_zh().contains("异常"));
    }
}