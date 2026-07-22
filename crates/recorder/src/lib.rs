//! pm-recorder：机会生命周期 CSV 记录器。
//!
//! 一个机会生命周期结束才写一行（非每轮每机会一行）。
//! 列：`question,start_time,end_time,duration_seconds,best_sum,scan_count,
//!     last_yes,last_no,volume,liquidity`。
//!
//! CSV 原语（ensure/append/count）复用 [`pm_storage`]；本 crate 仅定义 [`LifecycleRecord`]
//! 与 `From<&FinishedOpportunity>` 转换 + 表头常量。路径由调用方从 `Config.paths` 传入。

use pm_models::FinishedOpportunity;

/// V1.0 表头（列顺序固定，须与 [`LifecycleRecord`] 字段顺序一致）。
pub const HEADER: &[&str] = &[
    "question",
    "start_time",
    "end_time",
    "duration_seconds",
    "best_sum",
    "scan_count",
    "last_yes",
    "last_no",
    "volume",
    "liquidity",
];

/// 单条生命周期记录，序列化顺序由结构体字段顺序决定，须与 [`HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LifecycleRecord {
    pub question: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_seconds: i64,
    pub best_sum: f64,
    pub scan_count: u64,
    pub last_yes: f64,
    pub last_no: f64,
    pub volume: f64,
    pub liquidity: f64,
}

impl From<&FinishedOpportunity> for LifecycleRecord {
    fn from(f: &FinishedOpportunity) -> Self {
        LifecycleRecord {
            question: f.question.clone(),
            start_time: f.start_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            end_time: f.end_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            duration_seconds: f.duration_sec,
            best_sum: f.best_sum,
            scan_count: f.scan_count,
            last_yes: f.last_yes,
            last_no: f.last_no,
            volume: f.volume,
            liquidity: f.liquidity,
        }
    }
}

/// 确保 `opportunities.csv` 就绪（委托 [`pm_storage::ensure_csv`]）。
pub fn ensure_csv(path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
    pm_storage::ensure_csv(path, HEADER)
}

/// 追加一批已结束的生命周期记录（委托 [`pm_storage::append_records`]），返回写入条数。
pub fn append_records(records: &[LifecycleRecord], path: impl AsRef<std::path::Path>) -> usize {
    pm_storage::append_records(path, records)
}

/// 统计已有数据行数（委托 [`pm_storage::count_rows`]）。
pub fn count_records(path: impl AsRef<std::path::Path>) -> u64 {
    pm_storage::count_rows(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn finished() -> FinishedOpportunity {
        let now = Local::now();
        FinishedOpportunity {
            question: "Q".into(),
            start_time: now,
            end_time: now,
            duration_sec: 120,
            best_sum: 0.95,
            scan_count: 3,
            last_yes: 0.43,
            last_no: 0.57,
            volume: 1000.0,
            liquidity: 500.0,
        }
    }

    #[test]
    fn record_from_finished_preserves_fields() {
        let f = finished();
        let r = LifecycleRecord::from(&f);
        assert_eq!(r.question, "Q");
        assert_eq!(r.duration_seconds, 120);
        assert_eq!(r.scan_count, 3);
        assert!((r.best_sum - 0.95).abs() < 1e-9);
    }

    #[test]
    fn ensure_append_count_roundtrip() {
        let dir = std::env::temp_dir().join("pm_recorder_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("opportunities.csv");

        ensure_csv(&path).expect("ensure");
        assert_eq!(count_records(&path), 0);

        let f = finished();
        let rec = LifecycleRecord::from(&f);
        let n = append_records(&[rec], &path);
        assert_eq!(n, 1);
        assert_eq!(count_records(&path), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
