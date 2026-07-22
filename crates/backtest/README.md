# pm-backtest

历史回放 + 回测 + 回测报告。

## 职责
- `run_replay`：读 `opportunities.csv` 按时间步回放历史扫描（仅展示开/闭事件，不做交易）。供 `cargo run -- replay`。
- `run_backtest`：重放历史机会，用 `pm-shadow::ShadowEngine` 重新执行开/平仓并累计统计。供 `cargo run -- backtest`。
- `BacktestReport`：聚合已平仓交易（win_rate / avg / median / best / worst ROI / avg / longest duration），终端打印 + 追加 `backtest_report.csv`。

## 依赖
`pm-core`, `pm-models`（`ReplayOpportunity`/`Config`）, `pm-storage`（`load_sorted_opportunities`）,
`pm-shadow`（`ShadowEngine`/`ShadowTrade`）, `pm-utils`（格式化/数学）, `serde`, `anyhow`, `chrono`, `tokio`（replay sleep）。

## 用途
- `run_replay` / `run_backtest` 供 `apps/cli` 的 `replay` / `backtest` 命令调用。
- `BacktestReport` 被 `run_backtest` 产出。

## 设计约束
- Simulation Only：回测对开仓价做 `entry_slippage` 策略假设，结果偏乐观。
- 不读 `shadow_trades.csv` 旧结果，只读 `opportunities.csv` 原始数据，确保策略可重复回测。
- 禁止 `unwrap/expect/panic`。
