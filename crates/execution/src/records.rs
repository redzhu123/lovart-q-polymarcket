//! Execution Simulator 的 CSV 记录与读写。
//!
//! 表：execution_orders.csv，记录每笔订单终态（含 Rejected）。
//! 原语（ensure/append/count）复用 [`pm_storage`]。

use std::path::Path;

use crate::engine::ExecutionOrder;

/// execution_orders.csv 表头（列顺序须与 [`ExecutionOrderRecord`] 字段顺序一致）。
pub const ORDERS_HEADER: &[&str] = &[
    "order_id",
    "status",
    "create_time",
    "fill_time",
    "delay",
    "slippage",
    "fill_rate",
    "quantity",
    "filled_quantity",
    "cancel_reason",
];

/// 单条订单终态记录，序列化顺序由结构体字段顺序决定，须与 [`ORDERS_HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionOrderRecord {
    pub order_id: String,
    pub status: String,
    pub create_time: String,
    pub fill_time: String,
    pub delay: u32,
    pub slippage: f64,
    pub fill_rate: f64,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub cancel_reason: String,
}

impl From<&ExecutionOrder> for ExecutionOrderRecord {
    fn from(o: &ExecutionOrder) -> Self {
        ExecutionOrderRecord {
            order_id: o.order_id.clone(),
            status: o.status.as_str().to_string(),
            create_time: o.create_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            fill_time: o
                .fill_time
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
            delay: o.assigned_delay,
            slippage: o.slippage,
            fill_rate: o.fill_rate(),
            quantity: o.quantity,
            filled_quantity: o.filled_quantity,
            cancel_reason: o.cancel_reason.as_str().to_string(),
        }
    }
}

/// 确保 execution_orders.csv 就绪。
pub fn ensure_csv(path: impl AsRef<Path>) -> anyhow::Result<()> {
    pm_storage::ensure_csv(path, ORDERS_HEADER)
}

/// 启动时从 execution_orders.csv 读取历史行数，作为 order_id 计数基线。
pub fn load_order_base(path: impl AsRef<Path>) -> u64 {
    pm_storage::count_rows(path)
}

/// 追加订单终态记录到 execution_orders.csv。
pub fn append_orders(records: &[ExecutionOrderRecord], path: impl AsRef<Path>) -> usize {
    pm_storage::append_records(path, records)
}
