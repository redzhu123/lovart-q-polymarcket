//! pm-execution：Execution Simulator（Simulation Only）。
//!
//! 绝不连接钱包 / 发送订单 / 签名 / 上链 / Polygon。
//! 在 paper "立即成交" 基础上模拟真实成交过程：
//!   提交 -> Pending ->（随机延迟后）部分成交 / 完全成交 / 超时取消 / 过期。
//!
//! 模块：
//! - [`state`]：`OrderStatus`（6 态）/ `TerminalReason`。
//! - [`fill`]：`FillEngine`（随机延迟 / 分批 / 滑点 / 流动性失败）。
//! - [`engine`]：`ExecutionEngine` / `ExecParams` / `ExecEvent` / `ExecutionStats` / `ExecutionOrder` / `ExecPosition`。
//! - [`records`]：`ExecutionOrderRecord` + CSV（复用 `pm-storage`）。
//! - [`stress`]：`run_execution_test` 压测（`cargo run -- execution-test`）。
//!
//! `Side` 复用 `pm-core`。不依赖 `pm-models`（接口用 String + f64，保持低耦合）。

pub mod engine;
pub mod fill;
pub mod records;
pub mod state;
pub mod stress;

pub use engine::{
    ExecEvent, ExecParams, ExecPosition, ExecutionEngine, ExecutionOrder, ExecutionStats,
    PortfolioSummary, SubmitOutcome,
};
pub use records::{ensure_csv, load_order_base, append_orders, ExecutionOrderRecord};
pub use state::{OrderStatus, TerminalReason};
pub use stress::{run_execution_test, run_execution_test_with_count};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn reject_invalid_price() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_scan());
        assert!(matches!(
            eng.submit_buy("Q", 0.0, now),
            SubmitOutcome::Rejected(TerminalReason::InvalidPrice)
        ));
        assert!(matches!(
            eng.submit_buy("Q", f64::NAN, now),
            SubmitOutcome::Rejected(TerminalReason::InvalidPrice)
        ));
        assert_eq!(eng.stats().total, 2);
        assert_eq!(eng.stats().rejected, 2);
        assert_eq!(eng.pending_count(), 0);
    }

    #[test]
    fn reject_max_pending() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_stress());
        for i in 0..ExecParams::default_for_stress().max_pending_orders {
            let q = format!("Q{}", i);
            assert!(matches!(
                eng.submit_buy(&q, 0.5, now),
                SubmitOutcome::Accepted(_)
            ));
        }
        assert!(matches!(
            eng.submit_buy("Qx", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::MaxPending)
        ));
        assert_eq!(
            eng.pending_count(),
            ExecParams::default_for_stress().max_pending_orders
        );
    }

    #[test]
    fn reject_insufficient_cash() {
        let now = Local::now();
        let p = ExecParams {
            capital: 50.0,
            ..ExecParams::default_for_scan()
        };
        let mut eng = ExecutionEngine::new(p);
        assert!(matches!(
            eng.submit_buy("Q", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::InsufficientCash)
        ));
    }

    #[test]
    fn sell_no_position_rejected() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_scan());
        assert!(matches!(
            eng.submit_sell("Ghost", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::NoPosition)
        ));
    }

    #[test]
    fn buy_cash_invariant() {
        // 提交 30 笔 BUY -> 推进到全部终态 -> 验证现金不变式：
        //   available + pending + sum(open.cost) == initial_capital
        let now = Local::now();
        let p = ExecParams::default_for_scan();
        let initial = p.capital;
        let mut eng = ExecutionEngine::new(p);
        for i in 0..30 {
            let q = format!("Q{}", i);
            let price = 0.2 + (i as f64 % 50.0) / 100.0;
            let _ = eng.submit_buy(&q, price, now);
        }
        // pending 上限 20，故前 20 进入 pending，后 10 被拒（MaxPending）
        assert_eq!(eng.pending_count(), ExecParams::default_for_scan().max_pending_orders);
        assert_eq!(eng.stats().rejected, 10);

        // 推进直到全部终态
        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        assert_eq!(eng.pending_count(), 0);

        let inv = eng.available_cash() + eng.pending_cash() + eng.open_positions_cost();
        assert!(
            (inv - initial).abs() < 1e-6,
            "buy invariant broken: {} vs {}",
            inv,
            initial
        );
        // 30 笔提交尝试全部计入 total
        assert_eq!(eng.stats().total, 30);
        let settled =
            eng.stats().filled + eng.stats().cancelled + eng.stats().expired + eng.stats().rejected;
        assert_eq!(settled, eng.stats().total);
    }

    #[test]
    fn sell_closes_position_invariant() {
        // BUY -> 全部终态 -> 对每个 open position SELL -> 全部终态 ->
        // 验证 SELL 后不变式：available + pending + open.cost == initial + sum(realized)
        let now = Local::now();
        let p = ExecParams::default_for_scan();
        let initial = p.capital;
        let mut eng = ExecutionEngine::new(p);
        for i in 0..15 {
            let q = format!("P{}", i);
            let price = 0.2 + (i as f64 % 50.0) / 100.0;
            let _ = eng.submit_buy(&q, price, now);
        }
        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        // 对每个潜在 open position 平仓（未成交的会 NoPosition 拒绝，正常）
        for i in 0..15 {
            let q = format!("P{}", i);
            let _ = eng.submit_sell(&q, 0.5, now);
        }
        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        assert_eq!(eng.pending_count(), 0);
        // SELL 单批模型下成交的 BUY 持仓应全部平仓
        assert_eq!(eng.open_position_count(), 0);
        assert!(eng.closed_position_count() > 0);

        let inv = eng.available_cash() + eng.pending_cash() + eng.open_positions_cost();
        let realized = eng.closed_realized_pnl();
        assert!(
            (inv - initial - realized).abs() < 1e-6,
            "sell invariant broken: {} vs {} + {}",
            inv,
            initial,
            realized
        );
    }
}
