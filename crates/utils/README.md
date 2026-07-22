# pm-utils

纯工具函数。

## 职责
- 数值格式化：`fmt_money` / `fmt_sum` / `fmt_pnl` / `fmt_roi` / `fmt_pct` / `fmt_qty` / `fmt_scans`（NaN 兜底）。
- 统计数学：`mean` / `median` / `ratio`（空集合安全）。

## 依赖
无外部依赖、无内部 crate 依赖（叶子 crate）。

## 用途
被 `pm-scanner::display`（仪表盘）、`pm-backtest`（报告）、`pm-metrics`（统计）共用，避免格式化/数学逻辑重复。

## 设计约束
- 纯函数，无 IO、无全局状态。
- 禁止 `unwrap/expect/panic`。
