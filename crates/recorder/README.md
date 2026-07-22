# pm-recorder

机会生命周期 CSV 记录器。

## 职责
- `LifecycleRecord`：序列化记录（字段顺序与表头对齐）。
- `From<&FinishedOpportunity>`：从结束机会转换。
- `ensure_csv` / `append_records` / `count_records`：委托 `pm-storage` 通用原语 + 机会文件路径常量。
- 写入时机：机会生命周期结束（`reap` 产出）才写一行。

## 依赖
`pm-core`, `pm-models`（`FinishedOpportunity`）, `pm-storage`, `serde`, `anyhow`, `chrono`。
不依赖 `pm-tracker`（DTO 已下沉到 models，保持低耦合）。

## 用途
被 `pm-scanner::driver` 在每轮 `reap` 后调用，持久化结束的机会。

## 设计约束
- 写失败只返回 0，不 `panic`、不退出。
- 不缓冲、不批量事务、不数据库（V1.0 红线）。
