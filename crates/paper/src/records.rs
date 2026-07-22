//! Paper Trading 的 CSV 记录与读写。
//!
//! 三张表：paper_orders.csv / paper_positions.csv / paper_portfolio.csv。
//! 原语（ensure/append/count）复用 [`pm_storage`]；本模块仅定义记录结构 + 表头 + 薄封装。

use std::path::Path;

use chrono::{DateTime, Local};
use pm_portfolio::{Order, Portfolio, Position};

/// paper_orders.csv 表头（列顺序须与 [`OrderRecord`] 字段顺序一致）。
pub const ORDERS_HEADER: &[&str] = &[
    "order_id",
    "question",
    "side",
    "quantity",
    "price",
    "create_time",
    "fill_time",
    "status",
    "simulation_only",
];

/// paper_positions.csv 表头（列顺序须与 [`PositionRecord`] 字段顺序一致）。
pub const POSITIONS_HEADER: &[&str] = &[
    "question",
    "quantity",
    "average_price",
    "current_price",
    "unrealized_pnl",
    "realized_pnl",
    "roi",
    "status",
    "entry_time",
    "exit_time",
    "duration_seconds",
    "simulation_only",
];

/// paper_portfolio.csv 表头（列顺序须与 [`PortfolioRecord`] 字段顺序一致）。
pub const PORTFOLIO_HEADER: &[&str] = &[
    "timestamp",
    "cash",
    "available_cash",
    "locked_cash",
    "total_value",
    "total_pnl",
    "roi",
    "open_positions",
    "closed_positions",
    "simulation_only",
];

/// 单条订单记录，序列化顺序由结构体字段顺序决定，须与 [`ORDERS_HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderRecord {
    pub order_id: String,
    pub question: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub create_time: String,
    pub fill_time: String,
    pub status: String,
    pub simulation_only: bool,
}

impl From<&Order> for OrderRecord {
    fn from(o: &Order) -> Self {
        OrderRecord {
            order_id: o.order_id.clone(),
            question: o.question.clone(),
            side: o.side.as_str().to_string(),
            quantity: o.quantity,
            price: o.price,
            create_time: o.create_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            fill_time: o
                .fill_time
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
            status: o.status.as_str().to_string(),
            simulation_only: o.simulation_only,
        }
    }
}

/// 单条持仓记录（平仓时写入），须与 [`POSITIONS_HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PositionRecord {
    pub question: String,
    pub quantity: f64,
    pub average_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub roi: f64,
    pub status: String,
    pub entry_time: String,
    pub exit_time: String,
    pub duration_seconds: i64,
    pub simulation_only: bool,
}

impl PositionRecord {
    /// 由已关闭的 Position 构造（写入 CSV 时调用）。
    pub fn from_closed(p: &Position) -> Self {
        let duration = p
            .exit_time
            .map(|e| (e - p.entry_time).num_seconds())
            .unwrap_or(0);
        PositionRecord {
            question: p.question.clone(),
            quantity: p.quantity,
            average_price: p.average_price,
            current_price: p.current_price,
            unrealized_pnl: p.unrealized_pnl,
            realized_pnl: p.realized_pnl,
            roi: p.roi,
            status: p.status.as_str().to_string(),
            entry_time: p.entry_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            exit_time: p
                .exit_time
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
            duration_seconds: duration,
            simulation_only: true,
        }
    }
}

/// 组合快照记录（每轮扫描写入一行），须与 [`PORTFOLIO_HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortfolioRecord {
    pub timestamp: String,
    pub cash: f64,
    pub available_cash: f64,
    pub locked_cash: f64,
    pub total_value: f64,
    pub total_pnl: f64,
    pub roi: f64,
    pub open_positions: usize,
    pub closed_positions: usize,
    pub simulation_only: bool,
}

impl PortfolioRecord {
    /// 由当前组合快照构造。
    pub fn from_portfolio(pf: &Portfolio, now: DateTime<Local>) -> Self {
        PortfolioRecord {
            timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            cash: pf.cash,
            available_cash: pf.available_cash,
            locked_cash: pf.locked_cash,
            total_value: pf.total_value,
            total_pnl: pf.total_pnl,
            roi: pf.roi(),
            open_positions: pf.open_count(),
            closed_positions: pf.closed_count(),
            simulation_only: true,
        }
    }
}

/// 确保三张 paper CSV 就绪。任何错误返回 Err，由调用方提示。
pub fn ensure_csv(
    orders_path: impl AsRef<Path>,
    positions_path: impl AsRef<Path>,
    portfolio_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    pm_storage::ensure_csv(orders_path, ORDERS_HEADER)?;
    pm_storage::ensure_csv(positions_path, POSITIONS_HEADER)?;
    pm_storage::ensure_csv(portfolio_path, PORTFOLIO_HEADER)?;
    Ok(())
}

/// 启动时从 paper_orders.csv 读取历史行数，作为 order_id 计数基线。
pub fn load_order_base(orders_path: impl AsRef<Path>) -> u64 {
    pm_storage::count_rows(orders_path)
}

/// 追加订单记录到 paper_orders.csv。
pub fn append_orders(records: &[OrderRecord], path: impl AsRef<Path>) -> usize {
    pm_storage::append_records(path, records)
}

/// 追加已平仓持仓记录到 paper_positions.csv。
pub fn append_positions(records: &[PositionRecord], path: impl AsRef<Path>) -> usize {
    pm_storage::append_records(path, records)
}

/// 追加组合快照到 paper_portfolio.csv。
pub fn append_portfolio(records: &[PortfolioRecord], path: impl AsRef<Path>) -> usize {
    pm_storage::append_records(path, records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_core::Side;

    #[test]
    fn order_record_from_order() {
        let now = Local::now();
        let mut o = Order::new("PO-1".into(), "Q".into(), Side::Buy, 200.0, 0.5, now);
        o.fill(now);
        let r = OrderRecord::from(&o);
        assert_eq!(r.order_id, "PO-1");
        assert_eq!(r.side, "BUY");
        assert_eq!(r.status, "Filled");
        assert!(r.simulation_only);
    }
}
