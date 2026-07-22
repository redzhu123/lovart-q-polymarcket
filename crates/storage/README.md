# pm-storage

通用 CSV 读写原语 + 机会文件读取。

## 职责
- `ensure_csv(path, header)`：目录创建 / 文件创建 / 表头校验 / 旧表头备份 `*.bak`。
- `append_records(path, records)`：追加序列化记录（`has_headers(false)`），返回写入条数。
- `count_rows(path)`：统计数据行数（用于 order_id 续编号基线）。
- `load_sorted_opportunities(path)`：读 `opportunities.csv` -> `Vec<ReplayOpportunity>`（按 start_time 升序）。

## 依赖
`pm-core`, `pm-models`, `serde`, `csv`, `anyhow`, `chrono`。

## 用途
被 `pm-recorder` / `pm-shadow` / `pm-paper` / `pm-execution` / `pm-backtest` 复用，
消除原先五处重复的 CSV ensure/append/count/load 逻辑。`pm-tracker` 不依赖本 crate（保持轻）。

## 设计约束
- 写失败只返回 0 / `Err`，不 `panic`、不退出程序。
- 通用函数对 `T: Serialize` 泛型；类型化读取（opportunities）依赖 `pm-models` 的 DTO。
