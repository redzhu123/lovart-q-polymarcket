//! Paper Trading 引擎：维护组合 + 开仓判重，驱动订单生命周期。
//!
//! Simulation Only -- 不持有任何钱包 / 私钥 / 签名能力。
//! 风控统一委托 [`pm_portfolio::RiskManager`]（持仓上限 / 单笔上限 / 现金 / 当日亏损）。

use std::collections::HashMap;

use chrono::{DateTime, Local};
use pm_core::Side;
use pm_portfolio::{Order, Portfolio, Position, RiskManager, RiskPolicy, RiskRejection};

/// 开仓结果：成交返回 BUY Order，风控拒绝返回原因。
pub enum OpenOutcome {
    Filled(Order),
    Rejected(RiskRejection),
}

/// 平仓结果：SELL Order + 已关闭的 Position 快照。
#[derive(Debug)]
pub struct CloseOutcome {
    pub order: Order,
    pub position: Position,
}

/// Paper Trading 引擎。Simulation Only。
pub struct PaperTradingEngine {
    portfolio: Portfolio,
    risk: RiskManager,
    /// 自增计数器，用于生成 order_id（启动时从 CSV 行数恢复基线，避免重复）。
    counter: u64,
    /// question -> ()，开仓判重集合，与 portfolio.open_positions 保持一致。
    open_questions: HashMap<String, ()>,
}

impl PaperTradingEngine {
    /// 以指定初始资金与风控策略构造。
    pub fn new(capital: f64, policy: RiskPolicy) -> Self {
        Self {
            portfolio: Portfolio::new(capital),
            risk: RiskManager::new(policy),
            counter: 0,
            open_questions: HashMap::new(),
        }
    }

    /// 注入 order_id 计数基线（启动时调用，值 = 历史 paper_orders.csv 数据行数）。
    pub fn load_order_base(&mut self, base: u64) {
        self.counter = base;
    }

    /// 组合快照（只读），供控制台展示。
    pub fn portfolio(&self) -> &Portfolio {
        &self.portfolio
    }

    /// 风控管理器（只读）。
    pub fn risk(&self) -> &RiskManager {
        &self.risk
    }

    /// 组合可变引用（供测试 / 高级用途；普通交易流程请走 open/mark/close）。
    pub fn portfolio_mut(&mut self) -> &mut Portfolio {
        &mut self.portfolio
    }

    /// 生成下一个 order_id。
    fn next_order_id(&mut self) -> String {
        self.counter += 1;
        format!("PO-{:06}", self.counter)
    }

    /// 开仓：新机会出现时调用。Simulation Only。
    /// 风控统一由 RiskManager 检查：价格合法性 -> 持仓数上限 -> 现金充足 -> 当日亏损。
    pub fn open_position(
        &mut self,
        question: &str,
        entry_price: f64,
        now: DateTime<Local>,
    ) -> OpenOutcome {
        // 风控闸
        if let Err(r) = self
            .risk
            .check_open_position(entry_price, self.portfolio.available_cash, self.open_questions.len())
        {
            return OpenOutcome::Rejected(r);
        }
        // 单笔固定成本 = 风控策略的 max_position_size；quantity = size / price
        let cost = self.risk.policy().max_position_size;
        let quantity = cost / entry_price;

        // 创建 BUY 订单并立即模拟成交
        let mut order = Order::new(
            self.next_order_id(),
            question.to_string(),
            Side::Buy,
            quantity,
            entry_price,
            now,
        );
        order.fill(now);

        // 建仓 + 记账
        let pos = Position::open(question.to_string(), entry_price, quantity, now);
        self.portfolio.add_open(pos);
        self.open_questions.insert(question.to_string(), ());
        OpenOutcome::Filled(order)
    }

    /// mark-to-market：用最新价更新某持仓。Simulation Only。找不到则无操作。
    pub fn mark_position(&mut self, question: &str, current_price: f64) {
        self.portfolio.mark(question, current_price);
    }

    /// 重估组合（每轮开仓 / 平仓 / mark 完成后调用一次）。
    pub fn revalue(&mut self) {
        self.portfolio.revalue();
    }

    /// 平仓：机会结束时调用。Simulation Only。返回 SELL Order + 已关闭 Position；无持仓返回 None。
    pub fn close_position(
        &mut self,
        question: &str,
        exit_price: f64,
        now: DateTime<Local>,
    ) -> Option<CloseOutcome> {
        // 先从判重集合移除，避免任何分支下都不会残留陈旧条目
        self.open_questions.remove(question)?;
        // portfolio.close 完成记账 + 移入 closed_positions，返回已关闭快照
        let position = self.portfolio.close(question, exit_price, now)?;
        // 记录已实现盈亏到风控（当日亏损累计）
        self.risk.record_realized_pnl(position.realized_pnl);
        // 用快照里的 quantity 构造 SELL 订单并立即模拟成交
        let mut order = Order::new(
            self.next_order_id(),
            question.to_string(),
            Side::Sell,
            position.quantity,
            exit_price,
            now,
        );
        order.fill(now);
        Some(CloseOutcome { order, position })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_portfolio::RiskPolicy;

    fn policy() -> RiskPolicy {
        RiskPolicy {
            max_positions: 10,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 1000.0,
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn open_mark_close_portfolio_math() {
        let now = Local::now();
        let mut eng = PaperTradingEngine::new(10000.0, policy());

        assert!(approx(eng.portfolio().cash, 10000.0));
        assert_eq!(eng.portfolio().open_count(), 0);

        let o1 = eng.open_position("BTC", 0.42, now);
        let o2 = eng.open_position("Trump", 0.61, now);
        assert!(matches!(o1, OpenOutcome::Filled(_)));
        assert!(matches!(o2, OpenOutcome::Filled(_)));
        assert!(approx(eng.portfolio().cash, 9800.0));
        assert!(approx(eng.portfolio().locked_cash, 200.0));
        assert_eq!(eng.portfolio().open_count(), 2);

        eng.mark_position("BTC", 0.45);
        eng.mark_position("Trump", 0.58);
        eng.revalue();
        let qty_btc = 100.0 / 0.42;
        let qty_trump = 100.0 / 0.61;
        let unreal_btc = qty_btc * (0.45 - 0.42);
        let unreal_trump = qty_trump * (0.58 - 0.61);
        assert!(approx(eng.portfolio().total_pnl, unreal_btc + unreal_trump));

        let c = eng.close_position("BTC", 0.45, now);
        assert!(c.is_some());
        eng.revalue();
        assert!(approx(eng.portfolio().locked_cash, 100.0));
        assert_eq!(eng.portfolio().open_count(), 1);
        assert_eq!(eng.portfolio().closed_count(), 1);
    }

    #[test]
    fn risk_rejections_via_manager() {
        let now = Local::now();
        let p = RiskPolicy {
            max_positions: 2,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 1000.0,
        };
        let mut eng = PaperTradingEngine::new(10000.0, p);

        // 持仓上限
        eng.open_position("A", 0.5, now);
        eng.open_position("B", 0.5, now);
        assert!(matches!(
            eng.open_position("C", 0.5, now),
            OpenOutcome::Rejected(RiskRejection::MaxPositions)
        ));

        // 非法价格
        assert!(matches!(
            eng.open_position("D", 0.0, now),
            OpenOutcome::Rejected(RiskRejection::InvalidPrice)
        ));

        // 现金不足
        let mut eng2 = PaperTradingEngine::new(50.0, p);
        assert!(matches!(
            eng2.open_position("Low", 0.5, now),
            OpenOutcome::Rejected(RiskRejection::InsufficientCash)
        ));
    }

    #[test]
    fn daily_loss_blocks_opening() {
        let now = Local::now();
        let p = RiskPolicy {
            max_positions: 10,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 50.0,
        };
        let mut eng = PaperTradingEngine::new(10000.0, p);
        eng.open_position("A", 0.5, now);
        // 平仓产生大亏损：entry 0.5 -> exit 0.1，qty=200，realized=200*(0.1-0.5)=-80 < -50
        eng.close_position("A", 0.1, now);
        assert!(matches!(
            eng.open_position("B", 0.5, now),
            OpenOutcome::Rejected(RiskRejection::MaxDailyLoss)
        ));
    }

    #[test]
    fn close_nonexistent_is_none() {
        let now = Local::now();
        let mut eng = PaperTradingEngine::new(10000.0, policy());
        assert!(eng.close_position("Ghost", 0.5, now).is_none());
    }
}
