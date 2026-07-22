# pm-scanner

扫描子系统（库）。

## 职责（三个模块）
- `market`：Gamma API 分页拉取活跃市场（`fetch_active_markets`）+ 识别潜在套利机会（`find_opportunities`，SUM<阈值）。
- `driver`：扫描循环 `run_scan(cfg)` -- 拉取 -> `OpportunityTracker` -> 调用 `Strategy` 的 `on_scan`/`on_opportunity`/`on_close` ->
  写 CSV（recorder/shadow/paper/execution）-> 更新 `Metrics` -> 渲染仪表盘。Ctrl+C 可中断。
- `display`：仪表盘与明细渲染（New/Updated/Finished、Shadow/Paper/Execution 事件、Portfolio、累计统计）。

## 依赖
`pm-core`, `pm-models`, `pm-utils`, `pm-storage`, `pm-tracker`, `pm-recorder`, `pm-shadow`,
`pm-portfolio`, `pm-paper`, `pm-execution`, `pm-strategy`, `pm-metrics`, `tokio`, `reqwest`, `tracing`, `chrono`, `anyhow`。

## 用途
- `run_scan` 被 `apps/scanner`（专用二进制）与 `apps/cli::scan`（统一 CLI）共用 -- 无 app->app 依赖。
- `market` 的价格来源未来替换为 CLOB API 即可启用真实套利检测，driver/tracker/recorder 无需改动。

## 设计约束
- Simulation Only：所有交易 `simulation_only=true`，日志显式打印 "Simulation"。
- 单轮失败不退出，打印错误后继续下一轮。
- 禁止 `unwrap/expect/panic`；网络错误用 `anyhow` + `?`。
