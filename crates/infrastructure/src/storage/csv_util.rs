//! CSV 文件工具函数。
//!
//! 从 `pm-storage::lib.rs` 提取并统一。

use serde::Serialize;
use std::io::BufRead;
use std::path::Path;

/// 确保 CSV 文件存在且有正确的表头。
///
/// 如果文件不存在，创建目录和文件并写入表头。
/// 如果文件存在但表头不匹配，备份旧文件并写入新表头。
pub fn ensure_csv(path: impl AsRef<Path>, header: &[&str]) -> anyhow::Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if !path.exists() {
        let mut wtr = csv::Writer::from_path(path)?;
        wtr.write_record(header.iter().map(|h| h.to_string()))?;
        wtr.flush()?;
        return Ok(());
    }

    // 验证表头
    let first_line = read_first_nonempty_line(path);
    if let Some(existing) = first_line {
        let expected = header.join(",");
        if existing != expected {
            // 备份旧文件
            let backup = path.with_extension("csv.bak");
            std::fs::copy(path, &backup)?;
            // 写入新表头
            let mut wtr = csv::Writer::from_path(path)?;
            wtr.write_record(header.iter().map(|h| h.to_string()))?;
            wtr.flush()?;
        }
    }

    Ok(())
}

/// 追加记录到 CSV 文件（使用标准文件追加模式）
///
/// 返回实际写入的行数。
pub fn append_records<T: Serialize>(
    path: impl AsRef<Path>,
    records: &[T],
) -> anyhow::Result<usize> {
    let path = path.as_ref();
    if records.is_empty() {
        return Ok(0);
    }

    // 使用标准文件追加模式打开，避免 csv::Writer 截断文件
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for record in records {
        wtr.serialize(record)?;
    }
    wtr.flush()?;

    Ok(records.len())
}

/// 统计 CSV 文件行数（不含表头）。
///
/// 使用 chunked byte scanning：只计数 `\n` 字节，不分配 String、不做 UTF-8 校验，
/// 大文件（数百 MB / 百万行）也能在数十毫秒内完成。
pub fn count_rows(path: impl AsRef<Path>) -> u64 {
    let path = path.as_ref();
    if !path.exists() {
        return 0;
    }

    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    let mut reader = std::io::BufReader::with_capacity(256 * 1024, file);
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
    newlines.saturating_sub(1) // 去掉表头行
}

/// 读取 CSV 文件的第一行非空内容。
///
/// 使用 streaming read：只读取到第一个非空行即停止，不加载整个文件。
pub fn read_first_nonempty_line(path: impl AsRef<Path>) -> Option<String> {
    let path = path.as_ref();
    if !path.exists() {
        return None;
    }

    let Ok(file) = std::fs::File::open(path) else {
        return None;
    };
    let reader = std::io::BufReader::with_capacity(64 * 1024, file);
    for line in reader.lines() {
        if let Ok(s) = line {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_csv_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.csv");
        let header = &["col1", "col2", "col3"];
        ensure_csv(&path, header).unwrap();
        assert!(path.exists());
        let first = read_first_nonempty_line(&path);
        assert_eq!(first.unwrap(), "col1,col2,col3");
    }

    #[test]
    fn append_records_adds_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        let header = &["name", "value"];
        ensure_csv(&path, header).unwrap();

        let records = vec![vec!["a", "1"], vec!["b", "2"]];
        let count = append_records(&path, &records).unwrap();
        assert_eq!(count, 2);
        assert_eq!(count_rows(&path), 2);
    }

    #[test]
    fn count_rows_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.csv");
        ensure_csv(&path, &["col"]).unwrap();
        assert_eq!(count_rows(&path), 0);
    }

    #[test]
    fn count_rows_nonexistent_file() {
        assert_eq!(count_rows("/nonexistent/file.csv"), 0);
    }

    #[test]
    fn read_first_nonempty_line_nonexistent() {
        assert!(read_first_nonempty_line("/nonexistent/file.csv").is_none());
    }
}
