# pm-paper

Paper Trading 引擎 + 历史回放（Simulation Only）。

## 职责
- `PaperTradingEngine`：维护 `Portfolio` + 开仓判重，驱动订单生命周期（立即成交模型）。
- `OpenRejection` / `OpenOutcome` / `CloseOutcome`：开平仓结果。
- `OrderRecord` / `PositionRecord` / `PortfolioRecord`：CSV 记录（复用 `pm-storage`）。
- `run_paper_history`：回放 `opportunities.csv` 走 PaperTradingEngine，产出 paper 报告（供 `cargo run -- paper`）。

## 依赖
`pm-core`, `pm-models`（`ReplayOpportunity`）, `pm-portfolio`（`Portfolio`/`Position`/`Order`/`RiskManager`）, `pm-storage`, `serde`, `anyhow`, `chrono`。

## 用途
- 被 `pm-scanner::driver`（实盘扫描）调用，在组合层做资金管理 + 持仓 + 风控。
- 被 `pm-strategy::DefaultStrategy` 编排。
- `run_paper_history` 供 `apps/cli` 的 `paper` 命令调用。

## 设计约束
- Simulation Only：所有订单 `simulation_only=true`，日志显式打印 "Simulation"。
- 与 `pm-shadow` 并存：shadow 估算单机会理论套利，paper 在 10000 USDC 组合层做资金管理。
- 禁止 `unwrap/expect/panic`。
