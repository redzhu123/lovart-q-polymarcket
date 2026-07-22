//! pm-backtest：历史回放 + 回测 + 回测报告。
//!
//! - [`replay::run_replay`]：读 `opportunities.csv`，按 start_time 排序后逐时间步回放历史扫描过程（仅展示）。
//! - [`backtest::run_backtest`]：重放全部历史机会，对每个机会重新执行 Shadow 开/平仓并累计统计。
//! - [`report::BacktestReport`]：聚合已平仓交易（含 median / longest 等），终端打印 + 追加 `backtest_report.csv`。
//!
//! Simulation Only -- opportunities.csv 未保存逐轮快照与开仓瞬时价，回测对开仓价做策略假设
//! （entry_sum = best_sum * (1 + entry_slippage)），结果偏乐观，不代表真实收益。
//!
//! 机会文件读取复用 `pm-storage::load_sorted_opportunities`，避免重复实现。

pub mod backtest;
pub mod report;
pub mod replay;

pub use backtest::run_backtest;
pub use report::BacktestReport;
pub use replay::run_replay;
