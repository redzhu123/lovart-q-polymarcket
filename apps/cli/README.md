# pm-cli-app

统一研究 CLI（`pm-cli`，workspace 默认二进制）。

## 职责
- 分发六个模式（`cargo run -- <mode>`）：
  - `scan` -> `pm_scanner::driver::run_scan`
  - `replay` -> `pm_backtest::run_replay`
  - `paper` -> `pm_paper::run_paper_history`
  - `backtest` -> `pm_backtest::run_backtest`
  - `execution-test` -> `pm_execution::run_execution_test`
  - `report` -> 读各 CSV + `pm_metrics` 聚合展示
- 加载 `config.toml`、初始化 tracing。

## 依赖
`pm-scanner`, `pm-backtest`, `pm-paper`, `pm-execution`, `pm-metrics`, `pm-storage`,
`pm-models`, `pm-core`, `pm-utils`, `tokio`, `tracing`, `tracing-subscriber`, `anyhow`。

## 用途 / 运行
```
cargo run -- scan            # 默认成员即 apps/cli，故 `cargo run -- <mode>` 直接可用
cargo run -- replay
cargo run -- paper
cargo run -- backtest
cargo run -- execution-test
cargo run -- report
```
构建全部 crate：`cargo build --workspace`。

## 设计约束
- main 仅做：配置加载 + tracing 初始化 + 模式分发。
- 无 CLI 框架（std::env::args），保持简单（V1.0 红线：不引入额外框架）。
- 禁止 `unwrap/expect/panic`。
