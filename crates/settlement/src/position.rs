//! Position Settlement（持仓结算 — P2-06 第四节）。
//!
//! 所有成交驱动的持仓变化均由本模块统一处理。
//!
//! 职责：
//! - 开仓/加仓/减仓/平仓
//! - 均价调整
//! - 持仓成本重算
//! - 市值更新
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use std::collections::HashMap;

use crate::types::{Direction, PositionState, TradeFillEvent};

// ============================================================================
// PositionManager — Settlement 持仓管理器
// ============================================================================

/// Settlement 持仓管理器。
///
/// 维护所有市场的持仓状态。所有持仓变化必须通过本管理器。
#[derive(Debug, Clone)]
pub struct PositionManager {
    /// 持仓映射：key = "market_id|direction"。
    positions: HashMap<String, PositionState>,
    /// 已平仓持仓列表（历史）。
    closed_positions: Vec<PositionState>,
    /// 持仓 ID 序列号。
    seq: u64,
}

impl Default for PositionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionManager {
    /// 创建新持仓管理器。
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            closed_positions: Vec::new(),
            seq: 0,
        }
    }

    /// 生成持仓 ID。
    pub fn next_position_id(&mut self, now: DateTime<Local>) -> String {
        self.seq += 1;
        format!("SPOS-{}-{:06}", now.format("%Y%m%d"), self.seq)
    }

    /// 持仓 key（market_id + direction）。
    fn key(market_id: &str, direction: Direction) -> String {
        format!("{}|{}", market_id, direction.as_zh())
    }

    /// 查找未平仓持仓。
    pub fn find_open(&self, market_id: &str, direction: Direction) -> Option<&PositionState> {
        self.positions.get(&Self::key(market_id, direction))
    }

    /// 判断是否为开仓方向（买入 YES 或 买入 NO = 开仓）。
    /// 在预测市场中，Buy = 开多仓。
    pub fn is_opening(side: pm_core::Side) -> bool {
        matches!(side, pm_core::Side::Buy)
    }

    /// 判断是否为平仓方向（卖出 = 平仓）。
    pub fn is_closing(side: pm_core::Side) -> bool {
        matches!(side, pm_core::Side::Sell)
    }

    /// 处理成交：根据买卖方向自动开仓/加仓/减仓/平仓。
    ///
    /// # 返回
    ///
    /// - `position_summary`：持仓变化描述（中文）。
    /// - `realized_pnl`：本次成交产生的已实现盈亏。
    /// - `unrealized_pnl`：本次成交后的未实现盈亏。
    pub fn apply_fill(
        &mut self,
        event: &TradeFillEvent,
        now: DateTime<Local>,
    ) -> (String, f64, f64) {
        let side = event.side;

        if Self::is_opening(side) {
            // 开仓 / 加仓
            self.open_or_add(event, now)
        } else {
            // 平仓 / 减仓
            self.close_or_reduce(event, now)
        }
    }

    /// 开仓或加仓。
    fn open_or_add(&mut self, event: &TradeFillEvent, now: DateTime<Local>) -> (String, f64, f64) {
        let key = Self::key(&event.market_id, event.direction);
        let price = event.fill_price;
        let qty = event.fill_quantity;

        if let Some(pos) = self.positions.get_mut(&key) {
            // 加仓
            let old_qty = pos.quantity;
            let old_avg = pos.average_price;
            pos.add_fill(qty, price, &event.order_id, &event.trade_id, now);
            tracing::info!(
                position_id = %pos.position_id,
                market_id = %event.market_id,
                direction = %event.direction.as_zh(),
                old_qty = %old_qty,
                new_qty = %pos.quantity,
                old_avg = %old_avg,
                new_avg = %pos.average_price,
                "持仓加仓完成"
            );
            let summary = format!(
                "加仓 {} {}: {:.2} -> {:.2} @ {:.4} (均价 {:.4} -> {:.4})",
                event.market_id,
                event.direction.as_zh(),
                old_qty,
                pos.quantity,
                price,
                old_avg,
                pos.average_price,
            );
            (summary, 0.0, pos.unrealized_pnl)
        } else {
            // 新开仓
            let pos_id = self.next_position_id(now);
            let pos = PositionState::open(
                pos_id.clone(),
                event.market_id.clone(),
                event.direction,
                event.side,
                qty,
                price,
                event.order_id.clone(),
                event.trade_id.clone(),
                now,
            );
            tracing::info!(
                position_id = %pos_id,
                market_id = %event.market_id,
                direction = %event.direction.as_zh(),
                quantity = %qty,
                price = %price,
                cost = %pos.cost_basis,
                "新持仓创建"
            );
            let summary = format!(
                "新开仓 {} {}: {:.2} @ {:.4} (成本 {:.2})",
                event.market_id,
                event.direction.as_zh(),
                qty,
                price,
                pos.cost_basis,
            );
            self.positions.insert(key, pos);
            (summary, 0.0, 0.0)
        }
    }

    /// 减仓或平仓。
    fn close_or_reduce(
        &mut self,
        event: &TradeFillEvent,
        now: DateTime<Local>,
    ) -> (String, f64, f64) {
        let key = Self::key(&event.market_id, event.direction);
        let price = event.fill_price;
        let qty = event.fill_quantity;

        let pos = match self.positions.get_mut(&key) {
            Some(p) => p,
            None => {
                tracing::warn!(
                    market_id = %event.market_id,
                    direction = %event.direction.as_zh(),
                    "平仓失败：无对应持仓"
                );
                return (
                    format!("平仓失败 {}: 无对应持仓", event.market_id),
                    0.0,
                    0.0,
                );
            }
        };

        let old_qty = pos.quantity;
        let realized = pos.reduce(qty, price, now);

        let summary = if pos.is_closed {
            // 完全平仓
            let pos_id = pos.position_id.clone();
            let total_realized = pos.realized_pnl;
            self.closed_positions.push(pos.clone());
            self.positions.remove(&key);
            tracing::info!(
                position_id = %pos_id,
                market_id = %event.market_id,
                direction = %event.direction.as_zh(),
                exit_price = %price,
                realized_pnl = %total_realized,
                "持仓完全平仓"
            );
            format!(
                "完全平仓 {} {}: {:.2} @ {:.4} (已实现盈亏 {:.2})",
                event.market_id,
                event.direction.as_zh(),
                old_qty,
                price,
                total_realized,
            )
        } else {
            // 部分平仓
            tracing::info!(
                position_id = %pos.position_id,
                market_id = %event.market_id,
                direction = %event.direction.as_zh(),
                closed_qty = %qty,
                remaining_qty = %pos.quantity,
                realized = %realized,
                "持仓部分平仓"
            );
            format!(
                "部分平仓 {} {}: {:.2} -> {:.2} @ {:.4} (已实现盈亏 {:.2})",
                event.market_id,
                event.direction.as_zh(),
                old_qty,
                pos.quantity,
                price,
                realized,
            )
        };

        let unrealized = self
            .positions
            .get(&key)
            .map(|p| p.unrealized_pnl)
            .unwrap_or(0.0);

        (summary, realized, unrealized)
    }

    /// 标记价格（mark-to-market）。
    pub fn mark_position(
        &mut self,
        market_id: &str,
        direction: Direction,
        price: f64,
        now: DateTime<Local>,
    ) {
        let key = Self::key(market_id, direction);
        if let Some(pos) = self.positions.get_mut(&key) {
            pos.mark(price, now);
            tracing::debug!(
                position_id = %pos.position_id,
                market_id = %market_id,
                mark_price = %price,
                unrealized_pnl = %pos.unrealized_pnl,
                "持仓标记价格更新"
            );
        }
    }

    /// 获取所有未平仓持仓。
    pub fn open_positions(&self) -> Vec<&PositionState> {
        self.positions.values().collect()
    }

    /// 获取所有已平仓持仓。
    pub fn closed_positions(&self) -> &[PositionState] {
        &self.closed_positions
    }

    /// 获取所有持仓（未平仓 + 已平仓）。
    pub fn all_positions(&self) -> Vec<&PositionState> {
        let mut all: Vec<&PositionState> = self.positions.values().collect();
        all.extend(self.closed_positions.iter());
        all
    }

    /// 未平仓持仓数量。
    pub fn open_count(&self) -> usize {
        self.positions.len()
    }

    /// 已平仓持仓数量。
    pub fn closed_count(&self) -> usize {
        self.closed_positions.len()
    }

    /// 总未实现盈亏。
    pub fn total_unrealized_pnl(&self) -> f64 {
        self.positions
            .values()
            .map(|p| p.unrealized_pnl)
            .sum::<f64>()
    }

    /// 总已实现盈亏。
    pub fn total_realized_pnl(&self) -> f64 {
        self.closed_positions
            .iter()
            .map(|p| p.realized_pnl)
            .sum::<f64>()
            + self.positions.values().map(|p| p.realized_pnl).sum::<f64>()
    }

    /// 打印全部持仓（中文 CLI 输出）。
    pub fn print_zh(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  Settlement 持仓");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!(
            "  未平仓: {} 个  |  已平仓: {} 个",
            self.open_count(),
            self.closed_count()
        );
        println!(
            "  未实现盈亏: {:.2} USDC  |  已实现盈亏: {:.2} USDC",
            self.total_unrealized_pnl(),
            self.total_realized_pnl()
        );
        println!();

        if self.open_count() > 0 {
            println!("── 未平仓持仓 ──");
            println!();
            for pos in self.positions.values() {
                println!("  {}", pos.summary_zh());
            }
            println!();
        }

        if self.closed_count() > 0 {
            println!("── 已平仓持仓（最近 10 条）──");
            println!();
            let start = self.closed_positions.len().saturating_sub(10);
            for pos in &self.closed_positions[start..] {
                println!("  {}", pos.summary_zh());
            }
            println!();
        }

        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn sample_fill(side: Side, qty: f64, price: f64) -> TradeFillEvent {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        TradeFillEvent {
            trade_id: format!("T-{}", n),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side,
            fill_price: price,
            fill_quantity: qty,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        }
    }

    #[test]
    fn open_position_on_buy() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        let fill = sample_fill(Side::Buy, 100.0, 0.50);
        let (summary, realized, _unrealized) = mgr.apply_fill(&fill, now);
        assert!(summary.contains("新开仓"));
        assert!(approx(realized, 0.0));
        assert_eq!(mgr.open_count(), 1);
        let pos = mgr.find_open("mkt-btc", Direction::Yes).unwrap();
        assert!(approx(pos.quantity, 100.0));
        assert!(approx(pos.average_price, 0.50));
    }

    #[test]
    fn add_to_existing_position() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);
        mgr.apply_fill(&sample_fill(Side::Buy, 50.0, 0.60), now);

        assert_eq!(mgr.open_count(), 1);
        let pos = mgr.find_open("mkt-btc", Direction::Yes).unwrap();
        assert!(approx(pos.quantity, 150.0));
        assert!(approx(pos.average_price, 0.533333333));
        assert!(approx(pos.cost_basis, 80.0));
    }

    #[test]
    fn close_position_on_sell() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);

        let fill_sell = sample_fill(Side::Sell, 100.0, 0.60);
        let (summary, realized, _) = mgr.apply_fill(&fill_sell, now);

        assert!(summary.contains("完全平仓"));
        assert!(approx(realized, 10.0)); // 100 * (0.60 - 0.50)
        assert_eq!(mgr.open_count(), 0);
        assert_eq!(mgr.closed_count(), 1);
    }

    #[test]
    fn partial_close_reduces_position() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 200.0, 0.50), now);

        let fill_sell = sample_fill(Side::Sell, 80.0, 0.55);
        let (summary, realized, _) = mgr.apply_fill(&fill_sell, now);

        assert!(summary.contains("部分平仓"));
        assert!(approx(realized, 4.0)); // 80 * (0.55 - 0.50)
        assert_eq!(mgr.open_count(), 1);
        let pos = mgr.find_open("mkt-btc", Direction::Yes).unwrap();
        assert!(approx(pos.quantity, 120.0));
    }

    #[test]
    fn sell_without_position_warns() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        let fill = sample_fill(Side::Sell, 50.0, 0.55);
        let (summary, realized, _) = mgr.apply_fill(&fill, now);
        assert!(summary.contains("失败"));
        assert!(approx(realized, 0.0));
    }

    #[test]
    fn different_direction_different_position() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);

        // 不同方向的新成交
        let mut fill_no = sample_fill(Side::Buy, 50.0, 0.40);
        fill_no.direction = Direction::No;
        mgr.apply_fill(&fill_no, now);

        assert_eq!(mgr.open_count(), 2);
        assert!(mgr.find_open("mkt-btc", Direction::Yes).is_some());
        assert!(mgr.find_open("mkt-btc", Direction::No).is_some());
    }

    #[test]
    fn mark_position_updates_unrealized_pnl() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);

        mgr.mark_position("mkt-btc", Direction::Yes, 0.65, now);
        let pos = mgr.find_open("mkt-btc", Direction::Yes).unwrap();
        assert!(approx(pos.mark_price, 0.65));
        assert!(approx(pos.unrealized_pnl, 15.0)); // 100 * (0.65 - 0.50)
    }

    #[test]
    fn totals_aggregate_correctly() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);
        mgr.mark_position("mkt-btc", Direction::Yes, 0.55, now);

        assert!(approx(mgr.total_unrealized_pnl(), 5.0));
        assert!(approx(mgr.total_realized_pnl(), 0.0));
    }

    #[test]
    fn print_zh_does_not_panic() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        mgr.apply_fill(&sample_fill(Side::Buy, 100.0, 0.50), now);
        mgr.apply_fill(&sample_fill(Side::Sell, 100.0, 0.55), now);
        mgr.print_zh();
    }

    #[test]
    fn next_position_id_increments() {
        let now = Local::now();
        let mut mgr = PositionManager::new();
        let id1 = mgr.next_position_id(now);
        let id2 = mgr.next_position_id(now);
        assert!(id1 != id2);
        assert!(id1.starts_with("SPOS-"));
    }
}
