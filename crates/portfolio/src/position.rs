//! 持仓模型（Paper Trading）。
//!
//! Simulation Only -- 持仓为模拟开仓结果，不持有任何真实份额。
//! 开仓时 average_price = 入场价；每轮扫描用最新价 mark-to-market，
//! 更新 current_price 与 unrealized_pnl；平仓时计算 realized_pnl 并置 Closed。

use chrono::{DateTime, Local};

/// 持仓状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionStatus {
    Open,
    Closed,
}

impl PositionStatus {
    /// 用于 CSV 输出与控制台展示的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionStatus::Open => "Open",
            PositionStatus::Closed => "Closed",
        }
    }
}

/// 模拟持仓。Simulation Only。
#[derive(Debug, Clone)]
pub struct Position {
    pub question: String,
    pub quantity: f64,
    /// 入场均价（开仓价）。
    pub average_price: f64,
    /// 最新标记价。
    pub current_price: f64,
    /// 未实现盈亏（开仓时为 0，每轮 mark 更新）。
    pub unrealized_pnl: f64,
    /// 已实现盈亏（平仓时计算；开仓期间为 0）。
    pub realized_pnl: f64,
    /// 收益率（未实现 / 已实现，视状态而定）。
    pub roi: f64,
    pub status: PositionStatus,
    pub entry_time: DateTime<Local>,
    pub exit_time: Option<DateTime<Local>>,
}

impl Position {
    /// 开仓：以 entry_price 买入 quantity 份额。Simulation Only。
    pub fn open(question: String, entry_price: f64, quantity: f64, now: DateTime<Local>) -> Self {
        Self {
            question,
            quantity,
            average_price: entry_price,
            current_price: entry_price,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            roi: 0.0,
            status: PositionStatus::Open,
            entry_time: now,
            exit_time: None,
        }
    }

    /// 用最新价 mark-to-market：更新 current_price / unrealized_pnl / roi。
    pub fn mark(&mut self, current_price: f64) {
        self.current_price = current_price;
        let cost = self.cost_basis();
        self.unrealized_pnl = self.quantity * (current_price - self.average_price);
        self.roi = if cost.abs() > f64::EPSILON {
            self.unrealized_pnl / cost
        } else {
            0.0
        };
    }

    /// 平仓：以 exit_price 结算，计算 realized_pnl，状态置 Closed。Simulation Only。
    pub fn close(&mut self, exit_price: f64, now: DateTime<Local>) {
        let cost = self.cost_basis();
        self.realized_pnl = self.quantity * (exit_price - self.average_price);
        self.current_price = exit_price;
        self.unrealized_pnl = 0.0;
        self.roi = if cost.abs() > f64::EPSILON {
            self.realized_pnl / cost
        } else {
            0.0
        };
        self.exit_time = Some(now);
        self.status = PositionStatus::Closed;
    }

    /// 持仓成本（占用资金）= quantity * average_price。
    pub fn cost_basis(&self) -> f64 {
        self.quantity * self.average_price
    }

    /// 当前市值 = quantity * current_price。
    pub fn market_value(&self) -> f64 {
        self.quantity * self.current_price
    }

    /// 持仓时长（秒）；未平仓时按 entry_time 到 now 计算。
    pub fn duration_sec(&self, now: DateTime<Local>) -> i64 {
        let end = self.exit_time.unwrap_or(now);
        (end - self.entry_time).num_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn mark_updates_unrealized() {
        let now = Local::now();
        let mut p = Position::open("Q".into(), 0.50, 200.0, now); // cost 100
        p.mark(0.55);
        assert!(approx(p.unrealized_pnl, 200.0 * (0.55 - 0.50))); // +10
        assert!(approx(p.roi, 10.0 / 100.0)); // 10%
        assert_eq!(p.status, PositionStatus::Open);
    }

    #[test]
    fn close_realizes_pnl() {
        let now = Local::now();
        let mut p = Position::open("Q".into(), 0.50, 200.0, now);
        p.close(0.40, now);
        assert!(approx(p.realized_pnl, 200.0 * (0.40 - 0.50))); // -20
        assert!(approx(p.unrealized_pnl, 0.0));
        assert_eq!(p.status, PositionStatus::Closed);
        assert!(p.exit_time.is_some());
    }

    #[test]
    fn cost_and_market_value() {
        let now = Local::now();
        let mut p = Position::open("Q".into(), 0.25, 400.0, now); // cost 100
        p.mark(0.30);
        assert!(approx(p.cost_basis(), 100.0));
        assert!(approx(p.market_value(), 120.0));
    }
}
