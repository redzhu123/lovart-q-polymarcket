# pm-scanner-app

专用持续扫描二进制（`pm-scanner`）。

## 职责
- 瘦入口：加载 `config.toml` -> 初始化 tracing -> 调用 `pm_scanner::driver::run_scan(cfg)`。
- 仅做 scan；其余模式（replay/paper/backtest/execution-test/report）见 `apps/cli`。

## 依赖
`pm-scanner`, `pm-models`, `pm-core`, `pm-utils`, `tokio`, `tracing`, `tracing-subscriber`, `anyhow`。

## 用途 / 运行
```
cargo run -p pm-scanner-app          # 持续扫描
```
统一入口（含 scan）：
```
cargo run -- scan                    # 等价，经 apps/cli 分发
```

## 设计约束
- 保持 main 极薄：配置加载 + tracing 初始化 + run_scan 调用。
- 禁止 `unwrap/expect/panic`。
