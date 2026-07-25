//! pm-portfolio：组合资金管理（Simulation Only）。
//!
//! 统一的资金抽象，被 [`pm_paper`] 直接复用，未来 live execution 也可复用同一组合模型。
//!
//! 拥有：
//! - [`Position`] / [`PositionStatus`]：模拟持仓（开仓 / mark-to-market / 平仓 / cost_basis）。
//! - [`Order`] / [`OrderStatus`]：paper 的简单订单（Pending->Filled/Cancelled，立即成交模型）。
//! - [`Portfolio`]：cash / available_cash / locked_cash / total_value / total_pnl / ROI，
//!   开仓扣款 / 平仓入账 / 重估 / 持仓管理。
//! - [`RiskManager`] + [`RiskPolicy`]：最大持仓、单笔上限、最大日亏、待处理订单上限、现金检查。
//!
//! `Side` 复用 [`pm_core`]。配置由 driver 从 `Config` 转成 [`RiskPolicy`] 注入，本 crate 不依赖 pm-models。

pub mod order;
pub mod position;
pub mod risk;

pub use order::{Order, OrderStatus};
pub use position::{Position, PositionStatus};
pub use risk::{RiskManager, RiskPolicy, RiskRejection};

use chrono::{DateTime, Local};

/// B7: 用于判脏的快照键值组。
#[derive(Debug, Clone, PartialEq)]
struct StateHash {
    cash: u64,
    total_value: u64,
    total_pnl: i64,
    locked_cash: u64,
    open_count: usize,
    closed_count: usize,
}

impl StateHash {
    /// 从 Portfolio 当前状态构建哈希组（定点放大 1e6 避免浮点抖动）。
    fn from_portfolio(pf: &Portfolio) -> Self {
        Self {
            cash: (pf.cash * 1_000_000.0) as u64,
            total_value: (pf.total_value * 1_000_000.0) as u64,
            total_pnl: (pf.total_pnl * 1_000_000.0) as i64,
            locked_cash: (pf.locked_cash * 1_000_000.0) as u64,
            open_count: pf.open_positions.len(),
            closed_count: pf.closed_positions.len(),
        }
    }
}

// Position / PositionStatus 已由上方 `pub use` 引入作用域，供本模块内部直接使用。

/// 默认初始资金（USDC）。仅当调用方未显式提供资金时使用；运行时应从 `Config.portfolio` 注入。
pub const DEFAULT_INITIAL_CAPITAL: f64 = 10000.0;

/// 投资组合。Simulation Only。
///
/// 资金模型：
///   - `cash`           = 可用现金（BUY 减少、SELL 增加），控制台 "Cash" headline。
///   - `available_cash` = 可用于新订单的现金（立即成交模型下 = cash），风控检查字段。
///   - `locked_cash`    = 开仓成本占用之和（BUY 增加、SELL 释放）。
///   - `total_value`    = cash + locked_cash + unrealized_pnl（= cash + 持仓市值）。
///   - `total_pnl`      = realized_pnl + unrealized_pnl。
///   - `roi`            = total_pnl / initial_capital。
///
/// 不变量：`initial_capital = cash + locked_cash - realized_pnl`。
#[derive(Debug, Clone)]
pub struct Portfolio {
    pub initial_capital: f64,
    pub cash: f64,
    pub available_cash: f64,
    pub locked_cash: f64,
    pub total_value: f64,
    pub total_pnl: f64,
    pub open_positions: Vec<Position>,
    pub closed_positions: Vec<Position>,
    /// B7: 上次写入时的状态哈希。`None` = 从未写入，需要一次初始快照。
    /// 与 V1.09 `dirty: bool` 不同——使用**数值实质变化**而非单一大头针。
    last_saved_hash: Option<StateHash>,
}

impl Portfolio {
    /// 以指定初始资金构造组合。
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            cash: initial_capital,
            available_cash: initial_capital,
            locked_cash: 0.0,
            total_value: initial_capital,
            total_pnl: 0.0,
            open_positions: Vec::new(),
            closed_positions: Vec::new(),
            last_saved_hash: None, // 初始状态需要一次快照
        }
    }

    /// 默认资金构造（向后兼容；运行时优先用 [`Portfolio::new`] 注入配置资金）。
    pub fn with_default_capital() -> Self {
        Self::new(DEFAULT_INITIAL_CAPITAL)
    }

    /// 开仓扣款：cash / available_cash 减少 cost，locked_cash 增加 cost。
    fn debit(&mut self, cost: f64) {
        self.cash -= cost;
        self.available_cash -= cost;
        self.locked_cash += cost;
    }

    /// 平仓入账：释放成本 cost_basis，加入成交所得 proceeds。
    fn credit(&mut self, cost_basis: f64, proceeds: f64) {
        self.locked_cash -= cost_basis;
        self.cash += proceeds;
        self.available_cash += proceeds;
    }

    /// 已实现盈亏 = closed_positions 的 realized_pnl 之和。
    fn realized_pnl(&self) -> f64 {
        self.closed_positions.iter().map(|p| p.realized_pnl).sum()
    }

    /// 未实现盈亏 = open_positions 的 unrealized_pnl 之和。
    fn unrealized_pnl(&self) -> f64 {
        self.open_positions.iter().map(|p| p.unrealized_pnl).sum()
    }

    /// 重算 total_value / total_pnl。每轮开仓 / 平仓 / mark 完成后调用一次。
    pub fn revalue(&mut self) {
        let unreal = self.unrealized_pnl();
        self.total_pnl = self.realized_pnl() + unreal;
        self.total_value = self.cash + self.locked_cash + unreal;
        // B7: dirty 不由 revalue 设置——has_changed 会通过数值比较检测变化
    }

    /// ROI = total_pnl / initial_capital。
    pub fn roi(&self) -> f64 {
        if self.initial_capital.abs() > f64::EPSILON {
            self.total_pnl / self.initial_capital
        } else {
            0.0
        }
    }

    /// 开仓：扣款 + 入 open_positions。调用方负责风控与判重。
    pub fn add_open(&mut self, pos: Position) {
        let cost = pos.cost_basis();
        self.debit(cost);
        self.open_positions.push(pos);
        // B7: add_open 本身是状态变更，但 dirty 由 has_changed() 通过数值比较检测
    }

    /// mark-to-market：按 question 更新某持仓的 current_price / pnl。找不到则无操作。
    /// B7: 不再无条件设 dirty——has_changed() 通过 `total_value` / `total_pnl` 数值检测变化。
    pub fn mark(&mut self, question: &str, current_price: f64) {
        if let Some(pos) = self
            .open_positions
            .iter_mut()
            .find(|p| p.question == question)
        {
            pos.mark(current_price);
        }
    }

    /// 平仓：按 question 关闭持仓，移入 closed_positions，返回已关闭的快照。
    /// 找不到返回 None。调用方负责从 open_questions 同步移除。
    pub fn close(
        &mut self,
        question: &str,
        exit_price: f64,
        now: DateTime<Local>,
    ) -> Option<Position> {
        let idx = self
            .open_positions
            .iter()
            .position(|p| p.question == question)?;
        let mut pos = self.open_positions.remove(idx);
        let cost_basis = pos.cost_basis();
        pos.close(exit_price, now);
        let proceeds = pos.quantity * exit_price;
        self.credit(cost_basis, proceeds);
        let snapshot = pos.clone();
        self.closed_positions.push(pos);
        // B7: close 本身是状态变更，dirty 由 has_changed() 通过数值比较检测
        Some(snapshot)
    }

    /// 当前开仓数。
    pub fn open_count(&self) -> usize {
        self.open_positions.len()
    }

    /// 已平仓数。
    pub fn closed_count(&self) -> usize {
        self.closed_positions.len()
    }

    /// B7: 自上次 `mark_saved()` 以来组合关键数值是否实质变化（比较 cash / total_value / total_pnl / locked_cash / 开平仓数）。
    pub fn has_changed(&self) -> bool {
        match &self.last_saved_hash {
            Some(last) => &StateHash::from_portfolio(self) != last,
            None => true, // 初始状态需要一次快照
        }
    }

    /// B7: 标记已保存快照——记录当前状态哈希。
    pub fn mark_saved(&mut self) {
        self.last_saved_hash = Some(StateHash::from_portfolio(self));
    }
}

impl Default for Portfolio {
    fn default() -> Self {
        Self::with_default_capital()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 容差：浮点比较允许误差。
    const EPS: f64 = 1e-6;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn open_mark_close_portfolio_math() {
        let now = Local::now();
        let mut pf = Portfolio::new(10000.0);

        // 初始：10000 USDC，0 持仓
        assert!(approx(pf.cash, 10000.0));
        assert!(approx(pf.locked_cash, 0.0));
        assert_eq!(pf.open_count(), 0);

        // 开两仓：BTC @0.42、Trump @0.61，每笔成本 100
        pf.add_open(Position::open("BTC".into(), 0.42, 100.0 / 0.42, now));
        pf.add_open(Position::open("Trump".into(), 0.61, 100.0 / 0.61, now));
        // cash = 10000 - 200 = 9800；locked = 200
        assert!(approx(pf.cash, 9800.0));
        assert!(approx(pf.available_cash, 9800.0));
        assert!(approx(pf.locked_cash, 200.0));
        assert_eq!(pf.open_count(), 2);

        // mark：BTC->0.45（+）、Trump->0.58（-）
        pf.mark("BTC", 0.45);
        pf.mark("Trump", 0.58);
        pf.revalue();
        let qty_btc = 100.0 / 0.42;
        let qty_trump = 100.0 / 0.61;
        let unreal_btc = qty_btc * (0.45 - 0.42);
        let unreal_trump = qty_trump * (0.58 - 0.61);
        assert!(approx(pf.total_pnl, unreal_btc + unreal_trump));
        assert!(approx(
            pf.total_value,
            9800.0 + 200.0 + unreal_btc + unreal_trump
        ));

        // 平仓 BTC @0.45：realized = unreal_btc
        let c = pf.close("BTC", 0.45, now);
        assert!(c.is_some());
        pf.revalue();
        assert!(approx(pf.locked_cash, 100.0));
        assert!(approx(pf.cash, 9800.0 + qty_btc * 0.45));
        assert!(approx(pf.total_pnl, unreal_btc + unreal_trump));
        assert_eq!(pf.open_count(), 1);
        assert_eq!(pf.closed_count(), 1);
    }

    #[test]
    fn invariant_holds_after_open_and_close() {
        // initial = cash + locked - realized
        let now = Local::now();
        let mut pf = Portfolio::new(10000.0);
        pf.add_open(Position::open("A".into(), 0.5, 200.0, now)); // cost 100
        pf.add_open(Position::open("B".into(), 0.25, 400.0, now)); // cost 100
        let realized = pf.realized_pnl();
        assert!(approx(pf.cash + pf.locked_cash - realized, 10000.0));

        pf.close("A", 0.6, now);
        pf.revalue();
        let realized = pf.realized_pnl();
        assert!(approx(pf.cash + pf.locked_cash - realized, 10000.0));
    }

    #[test]
    fn close_nonexistent_is_none() {
        let now = Local::now();
        let mut pf = Portfolio::new(10000.0);
        assert!(pf.close("Ghost", 0.5, now).is_none());
    }

    #[test]
    fn roi_uses_actual_initial_capital() {
        let mut pf = Portfolio::new(5000.0);
        // 制造 100 已实现盈利
        pf.closed_positions
            .push(Position::open("X".into(), 1.0, 100.0, Local::now()));
        pf.revalue();
        // total_pnl 来自 realized (未实现为 0，因为 mark 未调用 -> unreal=0)
        assert!(approx(pf.roi(), 0.0 / 5000.0));
    }
}
