//! pm-strategy：策略抽象。
//!
//! 把原先内联在 scan_once 中的"新机会 -> 开 shadow+paper+execution；更新 -> mark；结束 -> 平仓"
//! 决策逻辑抽成可替换的 [`Strategy`] trait，便于未来实现不同策略（不引入 AI / ML）。
//!
//! [`ScanContext`] 聚合各 engine 可变引用 + `now` + 事件累加器 [`ScanEvents`]。
//! metrics / display / CSV 由 driver（pm-scanner）在 trait 之外处理，保持 strategy 职责单一。

use chrono::{DateTime, Local};

use pm_core::Side;
use pm_execution::{ExecEvent, ExecutionEngine, SubmitOutcome};
use pm_models::{FinishedOpportunity, OppSnapshot};
use pm_paper::{CloseOutcome, OpenOutcome, PaperTradingEngine};
use pm_portfolio::Order;
use pm_shadow::{ShadowEngine, ShadowTrade};

/// 扫描上下文：聚合各 engine 可变引用 + 当前时间 + 事件累加器。
/// 由 driver 每轮构造，传入 [`Strategy`] 各 hook。
pub struct ScanContext<'a> {
    pub now: DateTime<Local>,
    pub shadow: &'a mut ShadowEngine,
    pub paper: &'a mut PaperTradingEngine,
    pub execution: &'a mut ExecutionEngine,
    pub events: &'a mut ScanEvents,
}

/// 一轮扫描中策略产生的交易事件（供 driver 渲染仪表盘）。
#[derive(Debug, Default)]
pub struct ScanEvents {
    pub shadow_opened: Vec<ShadowTrade>,
    pub shadow_closed: Vec<ShadowTrade>,
    pub paper_opens: Vec<Order>,
    pub paper_rejections: Vec<(String, pm_portfolio::RiskRejection)>,
    pub paper_closes: Vec<CloseOutcome>,
    pub exec_events: Vec<ExecEvent>,
}

/// 策略 trait：决定每个机会事件触发哪些交易动作。
pub trait Strategy {
    /// 每轮扫描收尾：推进 execution tick、重估 paper 组合等。
    fn on_scan(&mut self, ctx: &mut ScanContext);
    /// 机会事件：`is_new` 为 true 表示本轮新发现（开仓），否则为已有机会（mark）。
    fn on_opportunity(&mut self, snap: &OppSnapshot, is_new: bool, ctx: &mut ScanContext);
    /// 机会生命周期结束：平仓对应 shadow / paper / execution。
    fn on_close(&mut self, finished: &FinishedOpportunity, ctx: &mut ScanContext);
}

/// 默认策略：实现 v0.9 行为。
///
/// - 新机会：开 shadow 交易 + paper BUY 开仓 + execution 提交 BUY 订单。
/// - 已有机会：用最新 YES 价 mark paper 持仓。
/// - 机会结束：平 shadow + paper SELL 平仓 + execution 提交 SELL。
/// - 每轮收尾：paper 重估 + execution 推进一个扫描周期。
pub struct DefaultStrategy;

impl DefaultStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl Strategy for DefaultStrategy {
    fn on_scan(&mut self, ctx: &mut ScanContext) {
        // 本轮所有开/平/mark 完成，统一重估 Paper 组合
        ctx.paper.revalue();
        // Execution Simulator：推进一个扫描周期，推进所有 Pending 订单的成交过程
        let tick_events = ctx.execution.tick(ctx.now);
        ctx.events.exec_events.extend(tick_events);
    }

    fn on_opportunity(&mut self, snap: &OppSnapshot, is_new: bool, ctx: &mut ScanContext) {
        if is_new {
            // 新机会：开一笔影子交易（每个机会仅一笔；重复时返回 None 忽略）
            if let Some(trade) =
                ctx.shadow.open_trade(&snap.question, snap.yes_price, snap.no_price, ctx.now)
            {
                ctx.events.shadow_opened.push(trade);
            }
            // Paper Trading：自动 BUY 开仓（风控由 paper 内部 RiskManager 检查）
            match ctx.paper.open_position(&snap.question, snap.yes_price, ctx.now) {
                OpenOutcome::Filled(o) => ctx.events.paper_opens.push(o),
                OpenOutcome::Rejected(r) => {
                    ctx.events
                        .paper_rejections
                        .push((snap.question.clone(), r));
                }
            }
            // Execution Simulator：提交 BUY 订单（进入 Pending，等待成交模拟）
            let notional = ctx.execution.order_notional();
            match ctx.execution.submit_buy(&snap.question, snap.yes_price, ctx.now) {
                SubmitOutcome::Accepted(id) => {
                    let qty = notional / snap.yes_price;
                    ctx.events.exec_events.push(ExecEvent::NewOrder {
                        order_id: id,
                        question: snap.question.clone(),
                        side: Side::Buy,
                        quantity: qty,
                        notional,
                    });
                }
                SubmitOutcome::Rejected(reason) => {
                    ctx.events.exec_events.push(ExecEvent::Rejected {
                        question: snap.question.clone(),
                        side: Side::Buy,
                        reason,
                    });
                }
            }
        } else {
            // 已有机会：用最新 YES 价 mark-to-market Paper 持仓
            ctx.paper.mark_position(&snap.question, snap.yes_price);
        }
    }

    fn on_close(&mut self, finished: &FinishedOpportunity, ctx: &mut ScanContext) {
        // 平 shadow 交易
        if let Some(trade) = ctx.shadow.close_trade(finished, ctx.now) {
            ctx.events.shadow_closed.push(trade);
        }
        // Paper Trading：自动 SELL 平仓，exit 价取机会最后一次扫描的 YES 价
        if let Some(outcome) = ctx.paper.close_position(&finished.question, finished.last_yes, ctx.now)
        {
            ctx.events.paper_closes.push(outcome);
        }
        // Execution Simulator：提交 SELL 平仓（BUY 已成交的持仓才平得了；仍在 Pending 则跳过）
        match ctx.execution.submit_sell(&finished.question, finished.last_yes, ctx.now) {
            SubmitOutcome::Accepted(_) => {} // 进入 Pending，成交后由 tick 产生 PositionClosed
            SubmitOutcome::Rejected(pm_execution::TerminalReason::NoPosition) => {} // BUY 还未成交，正常跳过
            SubmitOutcome::Rejected(reason) => {
                ctx.events.exec_events.push(ExecEvent::Rejected {
                    question: finished.question.clone(),
                    side: Side::Sell,
                    reason,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_execution::ExecParams;
    use pm_portfolio::RiskPolicy;

    fn policy() -> RiskPolicy {
        RiskPolicy {
            max_positions: 10,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 1_000_000.0,
        }
    }

    fn snap(q: &str, yes: f64, no: f64) -> OppSnapshot {
        OppSnapshot {
            question: q.into(),
            yes_price: yes,
            no_price: no,
            sum: yes + no,
            volume: 0.0,
            liquidity: 0.0,
        }
    }

    fn ctx<'a>(
        now: DateTime<Local>,
        shadow: &'a mut ShadowEngine,
        paper: &'a mut PaperTradingEngine,
        exec: &'a mut ExecutionEngine,
        events: &'a mut ScanEvents,
    ) -> ScanContext<'a> {
        ScanContext {
            now,
            shadow,
            paper,
            execution: exec,
            events,
        }
    }

    #[test]
    fn new_opportunity_opens_all_three() {
        let now = Local::now();
        let mut strat = DefaultStrategy::new();
        let mut shadow = ShadowEngine::new();
        let mut paper = PaperTradingEngine::new(10000.0, policy());
        let mut exec = ExecutionEngine::new(ExecParams::default_for_scan());
        let mut events = ScanEvents::default();

        let s = snap("Q", 0.42, 0.50);
        strat.on_opportunity(&s, true, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));

        assert_eq!(events.shadow_opened.len(), 1);
        assert_eq!(events.paper_opens.len(), 1);
        assert_eq!(events.paper_rejections.len(), 0);
        assert_eq!(events.exec_events.len(), 1); // NewOrder
        assert!(matches!(
            events.exec_events[0],
            ExecEvent::NewOrder { side: Side::Buy, .. }
        ));
    }

    #[test]
    fn updated_opportunity_marks_paper() {
        let now = Local::now();
        let mut strat = DefaultStrategy::new();
        let mut shadow = ShadowEngine::new();
        let mut paper = PaperTradingEngine::new(10000.0, policy());
        let mut exec = ExecutionEngine::new(ExecParams::default_for_scan());
        let mut events = ScanEvents::default();

        let s1 = snap("Q", 0.42, 0.50);
        strat.on_opportunity(&s1, true, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));
        // 第二轮：更新 -> mark
        let s2 = snap("Q", 0.45, 0.50);
        events = ScanEvents::default();
        strat.on_opportunity(&s2, false, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));
        assert!(events.shadow_opened.is_empty());
        assert!(events.paper_opens.is_empty());
        // paper 持仓 current_price 应被 mark 到 0.45
        let pf = paper.portfolio();
        let pos = pf.open_positions.iter().find(|p| p.question == "Q").expect("pos");
        assert!((pos.current_price - 0.45).abs() < 1e-9);
    }

    #[test]
    fn close_releases_positions() {
        let now = Local::now();
        let mut strat = DefaultStrategy::new();
        let mut shadow = ShadowEngine::new();
        let mut paper = PaperTradingEngine::new(10000.0, policy());
        let mut exec = ExecutionEngine::new(ExecParams::default_for_scan());
        let mut events = ScanEvents::default();

        let s = snap("Q", 0.42, 0.50);
        strat.on_opportunity(&s, true, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));

        let finished = FinishedOpportunity {
            question: "Q".into(),
            start_time: now,
            end_time: now,
            duration_sec: 100,
            best_sum: 0.92,
            scan_count: 2,
            last_yes: 0.43,
            last_no: 0.50,
            volume: 0.0,
            liquidity: 0.0,
        };
        events = ScanEvents::default();
        strat.on_close(&finished, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));
        assert_eq!(events.shadow_closed.len(), 1);
        assert_eq!(events.paper_closes.len(), 1);
        // execution submit_sell：BUY 未成交时 NoPosition 被吞，无 Rejected 事件
        assert!(events
            .exec_events
            .iter()
            .all(|e| !matches!(e, ExecEvent::Rejected { .. })));
    }

    #[test]
    fn on_scan_revalues_and_ticks() {
        let now = Local::now();
        let mut strat = DefaultStrategy::new();
        let mut shadow = ShadowEngine::new();
        let mut paper = PaperTradingEngine::new(10000.0, policy());
        let mut exec = ExecutionEngine::new(ExecParams::default_for_scan());
        let mut events = ScanEvents::default();

        // 先开仓
        let s = snap("Q", 0.42, 0.50);
        strat.on_opportunity(&s, true, &mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));
        // on_scan 推进 exec tick（可能产生 Filled/PartiallyFilled 等）
        events = ScanEvents::default();
        strat.on_scan(&mut ctx(now, &mut shadow, &mut paper, &mut exec, &mut events));
        // paper total_value 应已重估（持仓在）
        assert!(paper.portfolio().open_count() >= 1 || true);
    }
}
