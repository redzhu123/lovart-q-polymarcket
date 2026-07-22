# pm-tracker

机会生命周期跟踪器。

## 职责
- `OpportunityTracker`：以 question 为 key 维护活跃机会状态。
  - `observe(snap, now) -> TrackUpdate`：新建或更新。
  - `reap(seen_keys, now) -> Vec<FinishedOpportunity>`：清理本轮消失的机会。
  - `active_count()`。

## 依赖
`pm-core`, `pm-models`, `chrono`。

## 用途
被 `pm-scanner::driver`（scan 循环）调用，驱动机会的开/更新/结束事件，是 shadow/paper/execution 平仓的触发源。

## 设计约束
- 仅持有跟踪逻辑；DTO（`OpportunityState`/`TrackUpdate`/`FinishedOpportunity`）归 `pm-models`。
- 不做 IO、不写 CSV（归 `pm-recorder`）。
- 禁止 `unwrap/expect/panic`。
