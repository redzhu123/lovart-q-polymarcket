# pm-models

跨 crate 共享数据模型（DTO）。

## 职责
- 市场与扫描快照：`Market`（Gamma API DTO）、`OppSnapshot`（单轮机会瞬时快照）。
- 机会生命周期：`OpportunityState`、`TrackUpdate`、`FinishedOpportunity`、`ReplayOpportunity`。
- 配置：`Config` 及 `[scanner]/[portfolio]/[execution]/[risk]/[paths]/[replay]/[backtest]` 子段，`Config::load("config.toml")`。

## 依赖
`pm-core`, `serde`, `serde_json`, `chrono`, `toml`, `anyhow`。

## 用途
被 `pm-tracker` / `pm-scanner` / `pm-recorder` / `pm-shadow` / `pm-backtest` / `pm-strategy` 等共享，
作为它们之间的数据契约（data contract）。

## 设计约束
- 只放**行为轻量**的共享 DTO；带行为的 engine struct 与由 engine 类型转换的 CSV record 归各 engine crate（避免循环依赖）。
- 禁止 `unwrap/expect/panic`，`Config::load` 返回 `anyhow::Result<Config>`。
