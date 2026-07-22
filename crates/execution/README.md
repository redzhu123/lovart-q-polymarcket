# pm-execution

Execution Simulator（Simulation Only）。

## 职责
- `FillEngine`：随机成交延迟（0..=max_fill_delay 扫描周期）、分批计划（1~3 批）、size-based 滑点、流动性失败。
- `OrderStatus`（Pending/PartiallyFilled/Filled/Cancelled/Expired/Rejected）/ `TerminalReason`。
- `ExecutionOrder` / `ExecPosition`：模拟订单与持仓（含成交进度内部字段）。
- `ExecutionEngine`：维护 pending/terminal 订单、开/平仓持仓、available/pending 现金，`tick` 推进成交；风控（max_pending / cash / price / NoPosition）。
- `ExecutionStats`：Fill Rate / Execution Success Rate / Avg Fill Time / Avg Delay / Avg Slippage / Partial Fill Rate。
- `ExecEvent` / `ExecutionOrderRecord` + CSV（复用 `pm-storage`）。
- `run_execution_test`：1000 笔 BUY 压测。

## 依赖
`pm-core`（`Side`）, `pm-storage`, `serde`, `anyhow`, `chrono`, `rand`。不依赖 `pm-models`（接口用 `String`+`f64`）。

## 用途
- 被 `pm-scanner::driver`（实盘扫描）与 `pm-strategy::DefaultStrategy` 编排。
- `run_execution_test` 供 `apps/cli` 的 `execution-test` 命令调用。
- 与 `pm-paper` 并存：paper 为立即成交基线，execution 为真实成交模拟。

## 设计约束
- Simulation Only：所有订单 `simulation_only=true`。
- 现金不变式：`available + pending + sum(open.cost) == initial + sum(realized)`。
- 禁止 `unwrap/expect/panic`；FillEngine 带随机性，测试断言不变式而非具体终态。
