# pm-strategy

策略抽象：`Strategy` trait + `DefaultStrategy`。

## 职责
- `Strategy` trait：`on_scan`（每轮）/ `on_opportunity`（新或更新机会）/ `on_close`（机会结束）。
- `ScanContext`：聚合 `shadow` / `paper` / `execution` / `risk` 可变引用 + `now` + `cfg` + 事件累加器。
- `DefaultStrategy`：实现 v0.9 行为——新机会开 shadow+paper+execution，更新 mark，结束平仓。
- `ScanEvents`：累积本轮展示事件（开/平/mark/exec），交 driver 渲染。

## 依赖
`pm-core`, `pm-models`（`OppSnapshot`/`FinishedOpportunity`/`Config`）, `pm-portfolio`（`RiskManager`）,
`pm-shadow`, `pm-paper`, `pm-execution`, `chrono`。
不依赖 `pm-tracker`（由 driver 调用 tracker 后把结果传给 strategy）；不依赖 `pm-metrics`（metrics 由 driver 处理）。

## 用途
- 被 `pm-scanner::driver` 持有并按轮调用其三个 hook。
- 未来策略（如仅 paper、仅 shadow、自定义风控）实现同一 trait 即可替换，无需改 driver。

## 设计约束
- 不实现 AI / 机器学习 / 复杂算法（V1.0 红线）。
- 策略只做决策与触发 engine 动作；不做 IO、不打印仪表盘、不写 CSV（归 driver）。
- 禁止 `unwrap/expect/panic`。
