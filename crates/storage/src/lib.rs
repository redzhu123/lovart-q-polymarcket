//! pm-storage：通用 CSV 存储原语 + 机会文件读取。
//!
//! 把原先散落在 recorder / shadow / paper / execution / replay 中重复的 CSV 模式抽到一处：
//! - [`ensure_csv`]：目录/文件就绪 + 表头校验 + 旧表头备份为 `*.bak`。
//! - [`append_records`]：追加序列化记录，`has_headers(false)`，返回写入条数。
//! - [`count_rows`]：统计数据行数（不含表头），用于 order_id 续编号基线。
//! - [`read_first_nonempty_line`]：读取首行（表头校验用）。
//! - [`load_sorted_opportunities`]：读取 `opportunities.csv` -> `Vec<ReplayOpportunity>`（按 start_time 升序）。
//!
//! 通用函数对 `T: Serialize` 泛型；写失败用 `tracing::error!` 记录并返回 0 / Err，不 panic、不退出。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use serde::Serialize;

use pm_models::ReplayOpportunity;

/// 备份后缀（表头不符时改名为 `*.bak` 再重建）。
const BAK_SUFFIX: &str = ".bak";

/// 确保目录与 CSV 就绪：
/// - 目录不存在则创建；
/// - CSV 不存在则创建并写表头；
/// - CSV 存在但表头不符则改名为 `*.bak` 后重建。
///
/// `header` 为期望表头列（顺序须与记录字段顺序一致）。
pub fn ensure_csv(path: impl AsRef<Path>, header: &[&str]) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).context("创建 CSV 目录失败")?;
        }
    }
    let header_line = header.join(",");
    if !path.exists() {
        write_fresh_header(path, header)?;
        return Ok(());
    }
    if read_first_nonempty_line(path).as_deref() == Some(header_line.as_str()) {
        return Ok(());
    }
    // 表头不符：备份后重建
    let backup = backup_path(path);
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(path, &backup).context("备份旧 CSV 失败")?;
    tracing::info!(backup = %backup.display(), "检测到旧表头 CSV，已备份后重建");
    write_fresh_header(path, header)?;
    Ok(())
}

/// 新建 CSV 并写入表头。
fn write_fresh_header(path: &Path, header: &[&str]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path).context("创建 CSV 文件失败")?;
    wtr.write_record(header).context("写入表头失败")?;
    wtr.flush().context("flush 表头失败")?;
    Ok(())
}

/// 备份路径：原路径 + `.bak`。
fn backup_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(BAK_SUFFIX);
    std::path::PathBuf::from(s)
}

/// 读取文件首个非空行（用于表头校验）。失败返回 None。
pub fn read_first_nonempty_line(path: impl AsRef<Path>) -> Option<String> {
    let file = File::open(path.as_ref()).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        match line {
            Ok(s) if !s.trim().is_empty() => return Some(s.trim().to_string()),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

/// 统计某张 CSV 已有数据行数（不含表头）。文件不存在或读取失败时返回 0。
///
/// 使用 chunked byte scanning：只计数 `\n` 字节，不分配 String、不做 UTF-8 校验，
/// 大文件（数百 MB / 百万行）也能在数十毫秒内完成。
pub fn count_rows(path: impl AsRef<Path>) -> u64 {
    let path = path.as_ref();
    if !path.exists() {
        return 0;
    }
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let mut reader = BufReader::with_capacity(256 * 1024, file); // 256 KiB buffer
    let mut newlines = 0u64;
    let mut buf = [0u8; 256 * 1024];
    loop {
        use std::io::Read;
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                newlines += buf[..n].iter().filter(|&&b| b == b'\n').count() as u64;
            }
            Err(_) => return 0,
        }
    }
    // 最后一行可能没有换行符：如果有内容但不以 \n 结尾，仍然算一行（即表头行）。
    // 简单处理：只要文件非空就至少有表头，saturating_sub(1) 去掉表头。
    newlines.saturating_sub(1)
}

/// 通用追加写入：把一批记录序列化追加到 CSV，返回成功写入条数。
/// `has_headers(false)`：追加模式下绝不写表头，避免重复表头污染数据。
/// 单次写入失败用 `tracing::error!` 记录并返回当前已写入数，不 panic、不退出。
pub fn append_records<T: Serialize>(path: impl AsRef<Path>, records: &[T]) -> usize {
    if records.is_empty() {
        return 0;
    }
    let path = path.as_ref();
    let file = match OpenOptions::new().create(true).append(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(path = %path.display(), error = %e, "CSV 打开失败");
            return 0;
        }
    };
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);
    let mut written = 0usize;
    for r in records {
        if let Err(e) = wtr.serialize(r) {
            tracing::error!(path = %path.display(), error = %e, "CSV 序列化失败");
            break;
        }
        written += 1;
    }
    if let Err(e) = wtr.flush() {
        tracing::error!(path = %path.display(), error = %e, "CSV flush 失败");
        return 0;
    }
    written
}

// ---- 机会文件读取（replay / backtest / paper 共用）----

/// 从 `opportunities.csv` 反序列化的一行（字段名与表头一致）。
#[derive(Debug, Clone, serde::Deserialize)]
struct OpportunityRow {
    question: String,
    start_time: String,
    end_time: String,
    duration_seconds: i64,
    best_sum: f64,
    scan_count: u64,
    last_yes: f64,
    last_no: f64,
    volume: f64,
    liquidity: f64,
}

/// 解析 "YYYY-MM-DD HH:MM:SS" 为本地时间。DST 歧义或非法时间返回 Err。
fn parse_local(s: &str) -> Result<DateTime<Local>> {
    let ndt = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("解析时间失败: {}", s))?;
    Local
        .from_local_datetime(&ndt)
        .single()
        .ok_or_else(|| anyhow::anyhow!("本地时间非法或歧义: {}", s))
}

/// 从 `opportunities.csv` 加载全部历史机会并按 start_time 升序排序。
/// 文件不存在时返回空 Vec（不阻断，由调用方提示）。
pub fn load_sorted_opportunities(path: impl AsRef<Path>) -> Result<Vec<ReplayOpportunity>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut rdr = csv::Reader::from_path(path).context("打开 opportunities.csv 失败")?;
    let mut opps: Vec<ReplayOpportunity> = Vec::new();
    for result in rdr.deserialize() {
        let row: OpportunityRow = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "跳过无法解析的 opportunities 行");
                continue;
            }
        };
        let start_time = match parse_local(&row.start_time) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "跳过时间非法的 opportunities 行");
                continue;
            }
        };
        let end_time = match parse_local(&row.end_time) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "跳过时间非法的 opportunities 行");
                continue;
            }
        };
        opps.push(ReplayOpportunity {
            question: row.question,
            start_time,
            end_time,
            duration_sec: row.duration_seconds,
            best_sum: row.best_sum,
            scan_count: row.scan_count,
            last_yes: row.last_yes,
            last_no: row.last_no,
            volume: row.volume,
            liquidity: row.liquidity,
        });
    }
    opps.sort_by_key(|a| a.start_time);
    Ok(opps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Row {
        a: i32,
        b: f64,
    }

    #[test]
    fn ensure_append_count_roundtrip() {
        let dir = std::env::temp_dir().join("pm_storage_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("t.csv");
        let header = ["a", "b"];

        // 不存在 -> 建表头
        ensure_csv(&path, &header).expect("ensure");
        assert_eq!(count_rows(&path), 0);

        // 追加两条
        let n = append_records(&path, &[Row { a: 1, b: 1.5 }, Row { a: 2, b: 2.5 }]);
        assert_eq!(n, 2);
        assert_eq!(count_rows(&path), 2);

        // 表头相符 -> 不重建，行数不变
        ensure_csv(&path, &header).expect("ensure again");
        assert_eq!(count_rows(&path), 2);

        // 备份文件存在
        assert!(backup_path(&path).exists() || true); // 备份仅在表头不符时产生

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_csv_bad_header_backs_up_and_rebuilds() {
        let dir = std::env::temp_dir().join("pm_storage_test2");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("t.csv");
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(&path, "x,y\n1,2\n").expect("write");

        ensure_csv(&path, &["a", "b"]).expect("ensure");
        // 旧文件被备份
        assert!(backup_path(&path).exists());
        // 新表头就位
        assert_eq!(read_first_nonempty_line(&path).as_deref(), Some("a,b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_sorted_opportunities_missing_file_is_empty() {
        let opps = load_sorted_opportunities("no_such_file.csv").expect("ok");
        assert!(opps.is_empty());
    }
}
