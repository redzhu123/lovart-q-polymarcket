//! PositionManager — 持仓管理器（P2-05 第四节）。
//!
//! 统一持仓模型，支持 Prediction / Spot / Perpetual / AMM。
//! 提供持仓的增删改查 + mark-to-market + 平仓。

use crate::domain::{Direction, Position, PositionStatus};
use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicU64, Ordering};

static POS_SEQ: AtomicU64 = AtomicU64::new(0);

/// 持仓管理器。
pub struct PositionManager {
    positions: Vec<Position>,
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
        }
    }

    /// 生成下一个持仓 ID（格式 `POS-YYYYMMDD-NNNNNN`）。
    pub fn next_position_id(&self, now: DateTime<Local>) -> String {
        let n = POS_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        format!("POS-{}-{:06}", now.format("%Y%m%d"), n)
    }

    /// 当前全部持仓（含已平仓）。
    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    /// 当前活跃持仓数。
    pub fn open_count(&self) -> usize {
        self.positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .count()
    }

    /// 已平仓数。
    pub fn closed_count(&self) -> usize {
        self.positions.len() - self.open_count()
    }

    /// 添加新持仓。
    pub fn add_position(&mut self, pos: Position) {
        tracing::info!(
            position_id = %pos.position_id,
            market_id = %pos.market_id,
            direction = %pos.direction.as_zh(),
            quantity = %pos.quantity,
            price = %pos.average_price,
            "新持仓添加"
        );
        self.positions.push(pos);
    }

    /// 按 market_id + direction 查找活跃持仓，返回索引。
    pub fn find_open_by_market(&self, market_id: &str, direction: Direction) -> Option<usize> {
        self.positions.iter().position(|p| {
            p.market_id == market_id && p.direction == direction && p.status == PositionStatus::Open
        })
    }

    /// 加仓（均价调整）。
    pub fn add_to_position(
        &mut self,
        idx: usize,
        qty: f64,
        price: f64,
        order_id: &str,
        now: DateTime<Local>,
    ) {
        if let Some(pos) = self.positions.get_mut(idx) {
            pos.add_quantity(qty, price, order_id, now);
            tracing::info!(
                position_id = %pos.position_id,
                new_quantity = %pos.quantity,
                new_avg_price = %pos.average_price,
                "持仓加仓完成"
            );
        }
    }

    /// mark-to-market：按 market_id + direction 更新标记价。
    pub fn mark_position(
        &mut self,
        market_id: &str,
        direction: Direction,
        current_price: f64,
        now: DateTime<Local>,
    ) -> bool {
        if let Some(pos) = self.positions.iter_mut().find(|p| {
            p.market_id == market_id && p.direction == direction && p.status == PositionStatus::Open
        }) {
            pos.mark(current_price, now);
            true
        } else {
            false
        }
    }

    /// 平仓：按 market_id + direction 完全平仓，返回已实现盈亏。
    pub fn close_position(
        &mut self,
        market_id: &str,
        direction: Direction,
        exit_price: f64,
        now: DateTime<Local>,
    ) -> Option<f64> {
        if let Some(pos) = self.positions.iter_mut().find(|p| {
            p.market_id == market_id && p.direction == direction && p.status == PositionStatus::Open
        }) {
            let realized = pos.close(exit_price, now);
            tracing::info!(
                position_id = %pos.position_id,
                market_id = %market_id,
                exit_price = %exit_price,
                realized_pnl = %realized,
                "持仓平仓完成"
            );
            Some(realized)
        } else {
            None
        }
    }

    /// 批量 mark-to-market：传入 (market_id, current_price) 列表。
    pub fn mark_all(&mut self, prices: &[(String, f64)], now: DateTime<Local>) {
        for (market_id, price) in prices {
            self.mark_position(market_id, Direction::Yes, *price, now);
            self.mark_position(market_id, Direction::No, *price, now);
        }
    }

    /// 中文打印全部持仓。
    pub fn print_zh(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  持仓列表");
        println!("═══════════════════════════════════════════════════════════");
        println!();

        let open: Vec<_> = self
            .positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .collect();
        let closed: Vec<_> = self
            .positions
            .iter()
            .filter(|p| p.status == PositionStatus::Closed)
            .collect();

        if open.is_empty() {
            println!("  （无活跃持仓）");
        } else {
            println!("  活跃持仓 ({} 个)：", open.len());
            println!();
            println!(
                "  {:<20} {:<12} {:<8} {:<10} {:<10} {:<10} {:<12} {:<10}",
                "持仓 ID", "市场", "方向", "数量", "均价", "现价", "未实现盈亏", "收益率"
            );
            println!("  {}", "─".repeat(105));
            for pos in &open {
                println!(
                    "  {:<20} {:<12} {:<8} {:<10.2} {:<10.4} {:<10.4} {:<12.2} {:<10.2}%",
                    pos.position_id,
                    truncate(&pos.market_id, 12),
                    pos.direction.as_zh(),
                    pos.quantity,
                    pos.average_price,
                    pos.current_price,
                    pos.unrealized_pnl,
                    pos.roi * 100.0,
                );
            }
            println!();
        }

        if !closed.is_empty() {
            println!("  已平仓 ({} 个)：", closed.len());
            println!();
            for pos in &closed {
                println!(
                    "  {} | {} | {} | 已实现盈亏: {:.2} | ROI: {:.2}%",
                    pos.position_id,
                    truncate(&pos.market_id, 20),
                    pos.direction.as_zh(),
                    pos.realized_pnl,
                    pos.roi * 100.0,
                );
            }
            println!();
        }

        println!(
            "  总计: {} 持仓 ({} 活跃, {} 已平仓)",
            self.positions.len(),
            open.len(),
            closed.len()
        );
        println!();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

impl Default for PositionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AssetType;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn test_position(now: DateTime<Local>) -> Position {
        Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        )
    }

    #[test]
    fn add_and_find_position() {
        let mut mgr = PositionManager::new();
        let now = Local::now();
        mgr.add_position(test_position(now));
        assert_eq!(mgr.open_count(), 1);
        assert!(mgr.find_open_by_market("mkt-btc", Direction::Yes).is_some());
        assert!(mgr.find_open_by_market("mkt-eth", Direction::Yes).is_none());
    }

    #[test]
    fn mark_position_updates_price() {
        let mut mgr = PositionManager::new();
        let now = Local::now();
        mgr.add_position(test_position(now));
        mgr.mark_position("mkt-btc", Direction::Yes, 0.60, now);
        let pos = &mgr.positions()[0];
        assert!(approx(pos.current_price, 0.60));
        assert!(approx(pos.unrealized_pnl, 20.0));
    }

    #[test]
    fn add_to_existing_position() {
        let mut mgr = PositionManager::new();
        let now = Local::now();
        mgr.add_position(test_position(now));
        let idx = mgr.find_open_by_market("mkt-btc", Direction::Yes).unwrap();
        mgr.add_to_position(idx, 100.0, 0.60, "OMS-002", now);
        let pos = &mgr.positions()[0];
        assert!(approx(pos.quantity, 300.0));
        assert!(approx(pos.average_price, (100.0 + 60.0) / 300.0));
    }

    #[test]
    fn next_position_id_monotonic() {
        let mgr = PositionManager::new();
        let now = Local::now();
        let id1 = mgr.next_position_id(now);
        let id2 = mgr.next_position_id(now);
        assert!(id1.starts_with("POS-"));
        assert_ne!(id1, id2);
    }
}
