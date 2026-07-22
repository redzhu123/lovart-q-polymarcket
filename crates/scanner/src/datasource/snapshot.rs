//! Market Snapshot（V1.02 第九节）。
//!
//! 每轮扫描保存一份市场快照：更新时间 / 市场数 / Provider / 内容哈希。
//! 追加写入 `<data_dir>/market_snapshots.csv`，便于以后 Replay。
//!
//! 哈希对 sorted market_id 序列计算，内容不变则哈希不变（可检测市场集合是否变化）。

use std::collections::hash_map::DefaultHasher;
use std::fs::OpenOptions;
use std::hash::Hasher;
use std::io::Write;

use anyhow::Result;
use chrono::{DateTime, Local};

use pm_models::UnifiedMarket;

/// 单轮市场快照。
#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    /// 快照时间（本地，"YYYY-MM-DD HH:MM:SS"）。
    pub timestamp: String,
    /// 市场数。
    pub market_count: usize,
    /// 数据源 Provider 名称。
    pub provider: String,
    /// 市场集合内容哈希。
    pub hash: u64,
}

impl MarketSnapshot {
    /// 从一轮市场列表构造快照。
    pub fn from_markets(markets: &[UnifiedMarket], provider: &str, now: DateTime<Local>) -> Self {
        let mut ids: Vec<&str> = markets.iter().map(|m| m.market_id.as_str()).collect();
        ids.sort_unstable();
        let mut hasher = DefaultHasher::new();
        for id in &ids {
            hasher.write(id.as_bytes());
            // 分隔，避免 "ab"+"c" 与 "a"+"bc" 撞哈希
            hasher.write_u8(0xFF);
        }
        Self {
            timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            market_count: markets.len(),
            provider: provider.into(),
            hash: hasher.finish(),
        }
    }

    /// 追加写入 CSV（无表头则先写表头）。字段无逗号，简单行写即可。
    pub fn save_to_csv(&self, path: &str) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        // 文件为空时写表头
        let meta = std::fs::metadata(path)?;
        if meta.len() == 0 {
            writeln!(file, "timestamp,market_count,provider,hash")?;
        }
        writeln!(file, "{},{},{},{}", self.timestamp, self.market_count, self.provider, self.hash)?;
        Ok(())
    }

    /// 打印快照区块（数据源诊断用）。
    pub fn print_block(&self) {
        println!("{}", crate::display::DASH);
        println!();
        println!("市场快照");
        println!();
        println!("更新时间: {}", self.timestamp);
        println!("市场数  : {}", self.market_count);
        println!("数据源  : {}", self.provider);
        println!("哈希    : {:016x}", self.hash);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn um(id: &str) -> UnifiedMarket {
        UnifiedMarket {
            market_id: id.into(),
            question: id.into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: Some(0.4),
            no_price: Some(0.5),
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 2,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn hash_stable_regardless_of_order() {
        let now = Local::now();
        let a = MarketSnapshot::from_markets(&[um("x"), um("y"), um("z")], "gamma", now);
        let b = MarketSnapshot::from_markets(&[um("z"), um("x"), um("y")], "gamma", now);
        // 顺序不同但集合相同 -> 哈希相同
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.market_count, 3);
    }

    #[test]
    fn hash_changes_when_set_changes() {
        let now = Local::now();
        let a = MarketSnapshot::from_markets(&[um("x"), um("y")], "gamma", now);
        let b = MarketSnapshot::from_markets(&[um("x"), um("w")], "gamma", now);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn save_to_csv_writes_header_once() {
        let dir = std::env::temp_dir();
        let path = dir.join("pm_snapshot_test.csv");
        let _ = std::fs::remove_file(&path);
        let path = path.to_str().unwrap();
        let now = Local::now();
        MarketSnapshot::from_markets(&[um("a")], "gamma", now).save_to_csv(path).unwrap();
        MarketSnapshot::from_markets(&[um("b")], "gamma", now).save_to_csv(path).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        // 表头只出现一次
        assert_eq!(content.matches("timestamp,market_count,provider,hash").count(), 1);
        // 两行数据
        assert_eq!(content.trim_end().lines().count(), 3);
        let _ = std::fs::remove_file(path);
    }
}
