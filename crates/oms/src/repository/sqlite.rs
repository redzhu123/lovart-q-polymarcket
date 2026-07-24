//! SqliteRepository：SQLite 持久化（P2-04 第八节 — 接口预留）。
//!
//! 当前阶段仅实现 trait 结构 + 占位实现，不真正操作 SQLite。
//! 后续 P2-05+ 阶段可接入 `rusqlite` 等 crate 完善。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use std::path::PathBuf;

use chrono::{DateTime, Local};

use crate::events::OrderEvent;
use crate::order::{Order, OrderStatus, StatusChange};

use super::{OrderRepository, RepositoryHealth};

/// SQLite 仓库（接口预留）。
///
/// 所有方法当前返回 `unimplemented!`，调用方应在生产环境使用 `Memory` 或 `Csv` 实现。
/// 真正的 SQLite 实现将在 P2-05+ 引入 `rusqlite` crate 后补全。
pub struct SqliteRepository {
    /// SQLite 数据库文件路径（占位）。
    #[allow(dead_code)]
    path: PathBuf,
}

impl SqliteRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl OrderRepository for SqliteRepository {
    fn save(&self, _order: &Order) -> anyhow::Result<()> {
        // P2-05+ 实现 INSERT OR REPLACE
        Err(anyhow::anyhow!(
            "SqliteRepository 尚未实现，请使用 Memory 或 Csv（P2-05+ 将补全）"
        ))
    }

    fn find_by_id(&self, _order_id: &str) -> anyhow::Result<Option<Order>> {
        Err(anyhow::anyhow!(
            "SqliteRepository 尚未实现，请使用 Memory 或 Csv"
        ))
    }

    fn find_by_client_id(&self, _client_order_id: &str) -> anyhow::Result<Option<Order>> {
        Err(anyhow::anyhow!(
            "SqliteRepository 尚未实现，请使用 Memory 或 Csv"
        ))
    }

    fn list_all(&self) -> anyhow::Result<Vec<Order>> {
        Ok(Vec::new())
    }

    fn list_by_status(&self, _status: OrderStatus) -> anyhow::Result<Vec<Order>> {
        Ok(Vec::new())
    }

    fn list_active(&self) -> anyhow::Result<Vec<Order>> {
        Ok(Vec::new())
    }

    fn list_in_range(
        &self,
        _from: DateTime<Local>,
        _to: DateTime<Local>,
    ) -> anyhow::Result<Vec<Order>> {
        Ok(Vec::new())
    }

    fn count(&self) -> anyhow::Result<u64> {
        Ok(0)
    }

    fn count_by_status(&self) -> anyhow::Result<Vec<(OrderStatus, u64)>> {
        Ok(Vec::new())
    }

    fn delete(&self, _order_id: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn append_status_change(&self, _order_id: &str, _change: &StatusChange) -> anyhow::Result<()> {
        Ok(())
    }

    fn list_status_changes(&self, _order_id: &str) -> anyhow::Result<Vec<StatusChange>> {
        Ok(Vec::new())
    }

    fn append_event(&self, _event: &OrderEvent) -> anyhow::Result<()> {
        Ok(())
    }

    fn list_events(&self) -> anyhow::Result<Vec<OrderEvent>> {
        Ok(Vec::new())
    }

    fn name(&self) -> &str {
        "SqliteRepository (stub)"
    }

    fn storage_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }

    fn health(&self) -> RepositoryHealth {
        RepositoryHealth::unhealthy(
            "SqliteRepository 尚未实现（P2-05+ 将补全）。当前请使用 Memory 或 Csv",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_indicates_stub() {
        let r = SqliteRepository::new(PathBuf::from("data/test.sqlite"));
        assert!(r.name().contains("stub"));
        assert!(!r.health().healthy);
    }

    #[test]
    fn save_returns_error() {
        use crate::order::{Direction, Order};
        use chrono::Local;
        use pm_core::Side;
        use pm_gateway::{OrderType, TimeInForce};

        let r = SqliteRepository::new(PathBuf::from("data/test.sqlite"));
        let order = Order::new(
            "C1".into(),
            "mkt".into(),
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
            Local::now(),
        );
        assert!(r.save(&order).is_err());
    }
}
