# pm-metrics

统一指标计数器 + 统计辅助。

## 职责
- `Metrics`：会话内计数器，覆盖 Scanner（轮次）、Opportunity（new/updated/finished）、
  Shadow（open/closed）、Paper（opens/closes/rejections）、Execution（submitted/filled/cancelled/expired/rejected）、
  Portfolio（snapshots）。
- 增量方法 + `snapshot`（只读视图，供仪表盘/report）。
- 纯统计数学复用 `pm-utils`（mean/median/ratio/win_rate）。

## 依赖
`pm-core`, `pm-utils`。

## 用途
- 被 `pm-scanner::driver` 在每轮更新。
- 供 `apps/cli` 的 `report` 命令聚合展示（会话指标 + 读 CSV 持久化统计）。

## 设计约束
- 会话内计数器，非持久化（持久化走各 engine CSV）。
- 不做 IO；纯计数 + 快照。
- 禁止 `unwrap/expect/panic`。
