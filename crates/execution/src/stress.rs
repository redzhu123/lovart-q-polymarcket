//! execution-test 模式：生成大量模拟 BUY 订单，驱动成交，输出统计。
//!
//! Simulation Only -- 不联网、不拉行情，仅用 FillEngine 的随机模型。
//! 对应 `cargo run -- execution-test`。

use anyhow::Result;
use chrono::Local;
use pm_utils::{fmt_money, fmt_pct, fmt_scans};

use crate::engine::{ExecParams, ExecutionEngine, SubmitOutcome};
use crate::records::{ExecutionOrderRecord, append_orders, ensure_csv, load_order_base};

/// 压测订单数。对应需求："自动：生成：1000 模拟订单。"
const EXECUTION_TEST_ORDERS: u64 = 1000;

/// 压测安全阀：tick 上限，防止极端随机情况下死循环。
const SAFETY_TICK_CAP: u32 = 5000;

/// 控制台主分隔线。
const SEP: &str = "======================================";
/// 控制台段内分隔线。
const DASH: &str = "---";

/// execution-test 入口。
///
/// `params` 应使用大额资金（如 `ExecParams::default_for_stress()`，capital=1_000_000），
/// 避免现金耗尽干扰成交统计。`csv_path` 为 execution_orders.csv 路径。
pub fn run_execution_test(params: ExecParams, csv_path: &str) -> Result<()> {
    run_execution_test_with_count(params, csv_path, EXECUTION_TEST_ORDERS)
}

/// 同 [`run_execution_test`]，但可指定订单数（便于测试用小数量快速跑）。
pub fn run_execution_test_with_count(
    params: ExecParams,
    csv_path: &str,
    orders: u64,
) -> Result<()> {
    println!("Mode: Execution Test");
    println!();
    println!("Execution Simulator -- Simulation Only");
    println!();
    println!("Capital: {} USDC (Simulation)", fmt_money(params.capital));
    println!("Max Pending Orders: {}", params.max_pending_orders);
    println!("Order Notional: {} USDC", fmt_money(params.order_notional));
    println!();
    println!("Generating {} simulated BUY orders...", orders);
    println!();

    // 确保 CSV 就绪；失败只提示，不退出。
    if let Err(e) = ensure_csv(csv_path) {
        println!("Execution CSV init failed: {:#}", e);
    }

    let mut eng = ExecutionEngine::new(params);
    eng.load_order_base(load_order_base(csv_path));

    let now0 = Local::now();
    let mut submitted: u64 = 0;
    let mut tick_idx: u32 = 0;

    // 循环：每个 tick 尽量填满 pending 槽位 -> 提交 -> 推进 -> 写终态。
    // 退出条件：orders 笔提交完且无 pending。
    loop {
        let now = now0 + chrono::Duration::seconds(10 * tick_idx as i64);

        while eng.pending_count() < params.max_pending_orders && submitted < orders {
            let i = submitted;
            // 价格在 0.10..0.89 之间循环，保证合法且有差异
            let price = 0.10 + (i as f64 % 80.0) / 100.0;
            let q = format!("SIM-{}", i);
            match eng.submit_buy(&q, price, now) {
                SubmitOutcome::Accepted(_) => submitted += 1,
                // 现金充足理论不会触发；跳出本次提交，等 tick 推进后再试
                SubmitOutcome::Rejected(_) => break,
            }
        }

        let _events = eng.tick(now);

        // 写入本周期产生的终态订单
        let drained = eng.drain_terminal();
        if !drained.is_empty() {
            let recs: Vec<ExecutionOrderRecord> =
                drained.iter().map(ExecutionOrderRecord::from).collect();
            let _ = append_orders(&recs, csv_path);
        }

        tick_idx += 1;

        if submitted >= orders && eng.pending_count() == 0 {
            break;
        }
        if tick_idx > SAFETY_TICK_CAP {
            println!("Safety tick cap reached, stopping early.");
            break;
        }
    }

    println!("Submitted: {}", submitted);
    println!("Ticks: {}", tick_idx);
    println!();

    print_execution_stats(eng.stats(), &eng.portfolio_summary());
    Ok(())
}

/// 打印执行统计与组合概览。
fn print_execution_stats(
    stats: &crate::engine::ExecutionStats,
    summary: &crate::engine::PortfolioSummary,
) {
    println!("{}", SEP);
    println!();
    println!("Execution Simulator");
    println!();
    println!("{}", DASH);
    println!();
    println!("Execution Statistics -- Simulation Only");
    println!();
    println!("{}", DASH);
    println!();
    println!("Orders");
    println!();
    println!("{}", stats.total);
    println!();
    println!("{}", DASH);
    println!();
    println!("Filled");
    println!();
    println!("{}", stats.filled);
    println!();
    println!("Cancelled");
    println!();
    println!("{}", stats.cancelled);
    println!();
    println!("Expired");
    println!();
    println!("{}", stats.expired);
    println!();
    println!("Rejected");
    println!();
    println!("{}", stats.rejected);
    println!();
    println!("{}", DASH);
    println!();
    println!("Fill Rate");
    println!();
    println!("{}", fmt_pct(stats.fill_rate()));
    println!();
    println!("Execution Success Rate");
    println!();
    println!("{}", fmt_pct(stats.execution_success_rate()));
    println!();
    println!("Average Fill Time");
    println!();
    println!("{} scans", fmt_scans(stats.average_fill_time()));
    println!();
    println!("Average Delay");
    println!();
    println!("{} scans", fmt_scans(stats.average_delay()));
    println!();
    println!("Average Slippage");
    println!();
    println!("{}", fmt_pct(stats.average_slippage()));
    println!();
    println!("Partial Fill Rate");
    println!();
    println!("{}", fmt_pct(stats.partial_fill_rate()));
    println!();
    println!("{}", DASH);
    println!();
    println!("Portfolio -- Simulation Only");
    println!();
    println!("{}", DASH);
    println!();
    println!("Available Cash");
    println!();
    println!("{} USDC", fmt_money(summary.available_cash));
    println!();
    println!("Pending Cash");
    println!();
    println!("{} USDC", fmt_money(summary.pending_cash));
    println!();
    println!("Pending Orders");
    println!();
    println!("{}", summary.pending_orders);
    println!();
    println!("Open Positions");
    println!();
    println!("{}", summary.open_positions);
    println!();
    println!("Closed Positions");
    println!();
    println!("{}", summary.closed_positions);
    println!();
    println!("{}", SEP);
    println!();
    println!("Simulation Only -- 模拟成交，非真实交易");
}
