//! Repository（统一仓库 — P2-06 第九节）。
//!
//! 统一 Repository Trait，支持：
//! - Memory（内存）
//! - CSV（CSV 文件持久化）
//! - SQLite（接口预留）
//! - PostgreSQL（未来支持）
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use crate::types::{LedgerEntry, SettlementResult, TradeFillEvent};
use anyhow::Result;
use std::path::PathBuf;

// ============================================================================
// RepositoryType — 仓库类型
// ============================================================================

/// 仓库类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryType {
    /// 内存存储。
    Memory,
    /// CSV 文件持久化。
    Csv,
    /// SQLite 数据库（接口预留）。
    Sqlite,
}

impl RepositoryType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RepositoryType::Memory => "内存",
            RepositoryType::Csv => "CSV",
            RepositoryType::Sqlite => "SQLite",
        }
    }
}

// ============================================================================
// SettlementRepository — 统一仓库 Trait
// ============================================================================

/// 结算仓库 Trait。
///
/// 所有持久化操作经由此 trait。
pub trait SettlementRepository: Send + Sync {
    /// 仓库名称。
    fn name(&self) -> &str;

    /// 仓库类型。
    fn repo_type(&self) -> RepositoryType;

    // ---- 成交事件 ----

    /// 保存成交事件。
    fn save_fill_event(&self, event: &TradeFillEvent) -> Result<()>;

    /// 获取所有成交事件。
    fn list_fill_events(&self) -> Result<Vec<TradeFillEvent>>;

    // ---- 结算结果 ----

    /// 保存结算结果。
    fn save_settlement(&self, result: &SettlementResult) -> Result<()>;

    /// 获取所有结算结果。
    fn list_settlements(&self) -> Result<Vec<SettlementResult>>;

    /// 按成交 ID 查找结算结果。
    fn find_settlement_by_trade(&self, trade_id: &str) -> Result<Option<SettlementResult>>;

    // ---- 资金流水 ----

    /// 保存资金流水条目。
    fn save_ledger_entry(&self, entry: &LedgerEntry) -> Result<()>;

    /// 获取所有资金流水。
    fn list_ledger_entries(&self) -> Result<Vec<LedgerEntry>>;

    /// 按订单 ID 查找流水。
    fn find_ledger_by_order(&self, order_id: &str) -> Result<Vec<LedgerEntry>>;
}

// ============================================================================
// InMemoryRepository — 内存实现
// ============================================================================

/// 内存仓库（默认）。
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    fill_events: std::sync::Mutex<Vec<TradeFillEvent>>,
    settlements: std::sync::Mutex<Vec<SettlementResult>>,
    ledger_entries: std::sync::Mutex<Vec<LedgerEntry>>,
}

impl InMemoryRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SettlementRepository for InMemoryRepository {
    fn name(&self) -> &str {
        "InMemory"
    }

    fn repo_type(&self) -> RepositoryType {
        RepositoryType::Memory
    }

    fn save_fill_event(&self, event: &TradeFillEvent) -> Result<()> {
        self.fill_events.lock().unwrap().push(event.clone());
        Ok(())
    }

    fn list_fill_events(&self) -> Result<Vec<TradeFillEvent>> {
        Ok(self.fill_events.lock().unwrap().clone())
    }

    fn save_settlement(&self, result: &SettlementResult) -> Result<()> {
        self.settlements.lock().unwrap().push(result.clone());
        Ok(())
    }

    fn list_settlements(&self) -> Result<Vec<SettlementResult>> {
        Ok(self.settlements.lock().unwrap().clone())
    }

    fn find_settlement_by_trade(&self, trade_id: &str) -> Result<Option<SettlementResult>> {
        Ok(self
            .settlements
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.trade_id == trade_id)
            .cloned())
    }

    fn save_ledger_entry(&self, entry: &LedgerEntry) -> Result<()> {
        self.ledger_entries.lock().unwrap().push(entry.clone());
        Ok(())
    }

    fn list_ledger_entries(&self) -> Result<Vec<LedgerEntry>> {
        Ok(self.ledger_entries.lock().unwrap().clone())
    }

    fn find_ledger_by_order(&self, order_id: &str) -> Result<Vec<LedgerEntry>> {
        Ok(self
            .ledger_entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.order_id == order_id)
            .cloned()
            .collect())
    }
}

// ============================================================================
// CsvRepository — CSV 文件实现
// ============================================================================

/// CSV 文件仓库。
pub struct CsvRepository {
    fills_csv: PathBuf,
    settlements_csv: PathBuf,
    ledger_csv: PathBuf,
}

impl CsvRepository {
    pub fn new(fills_csv: PathBuf, settlements_csv: PathBuf, ledger_csv: PathBuf) -> Self {
        Self {
            fills_csv,
            settlements_csv,
            ledger_csv,
        }
    }

    /// 写入 CSV（追加一行）。
    fn append_csv<T: serde::Serialize>(path: &PathBuf, row: &T) -> Result<()> {
        let file_exists = path.exists();
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(!file_exists)
            .from_path(path)?;
        wtr.serialize(row)?;
        wtr.flush()?;
        Ok(())
    }

    /// 读取 CSV（全量）。
    fn read_csv<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf) -> Result<Vec<T>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut rdr = csv::ReaderBuilder::new().from_path(path)?;
        let mut records = Vec::new();
        for result in rdr.deserialize() {
            let record: T = result?;
            records.push(record);
        }
        Ok(records)
    }
}

impl SettlementRepository for CsvRepository {
    fn name(&self) -> &str {
        "CSV"
    }

    fn repo_type(&self) -> RepositoryType {
        RepositoryType::Csv
    }

    fn save_fill_event(&self, event: &TradeFillEvent) -> Result<()> {
        Self::append_csv(&self.fills_csv, event)
    }

    fn list_fill_events(&self) -> Result<Vec<TradeFillEvent>> {
        Self::read_csv(&self.fills_csv)
    }

    fn save_settlement(&self, result: &SettlementResult) -> Result<()> {
        Self::append_csv(&self.settlements_csv, result)
    }

    fn list_settlements(&self) -> Result<Vec<SettlementResult>> {
        Self::read_csv(&self.settlements_csv)
    }

    fn find_settlement_by_trade(&self, trade_id: &str) -> Result<Option<SettlementResult>> {
        let all: Vec<SettlementResult> = Self::read_csv(&self.settlements_csv)?;
        Ok(all.into_iter().find(|s| s.trade_id == trade_id))
    }

    fn save_ledger_entry(&self, entry: &LedgerEntry) -> Result<()> {
        Self::append_csv(&self.ledger_csv, entry)
    }

    fn list_ledger_entries(&self) -> Result<Vec<LedgerEntry>> {
        Self::read_csv(&self.ledger_csv)
    }

    fn find_ledger_by_order(&self, order_id: &str) -> Result<Vec<LedgerEntry>> {
        let all: Vec<LedgerEntry> = Self::read_csv(&self.ledger_csv)?;
        Ok(all.into_iter().filter(|e| e.order_id == order_id).collect())
    }
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建仓库（根据类型）。
pub fn create_repository(
    repo_type: RepositoryType,
    fills_csv: Option<PathBuf>,
    settlements_csv: Option<PathBuf>,
    ledger_csv: Option<PathBuf>,
) -> Result<Box<dyn SettlementRepository>> {
    match repo_type {
        RepositoryType::Memory => Ok(Box::new(InMemoryRepository::new())),
        RepositoryType::Csv => {
            let fills = fills_csv.unwrap_or_else(|| PathBuf::from("data/settlement_fills.csv"));
            let settlements =
                settlements_csv.unwrap_or_else(|| PathBuf::from("data/settlement_results.csv"));
            let ledger = ledger_csv.unwrap_or_else(|| PathBuf::from("data/settlement_ledger.csv"));
            Ok(Box::new(CsvRepository::new(fills, settlements, ledger)))
        }
        RepositoryType::Sqlite => {
            // 接口预留：回退到 Memory
            tracing::warn!("SQLite 仓库尚未实现，使用 Memory 仓库");
            Ok(Box::new(InMemoryRepository::new()))
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Direction, FeeBreakdown};
    use chrono::Local;
    use pm_core::Side;

    fn sample_fill() -> TradeFillEvent {
        TradeFillEvent {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: false,
            gateway_name: "Mock".into(),
        }
    }

    #[test]
    fn memory_repository_save_and_list() {
        let repo = InMemoryRepository::new();
        let fill = sample_fill();
        repo.save_fill_event(&fill).unwrap();
        assert_eq!(repo.list_fill_events().unwrap().len(), 1);

        let result = SettlementResult::success(
            "S-001".into(),
            "T-001".into(),
            "OMS-001".into(),
            FeeBreakdown::zero(),
            Some("test".into()),
            10000.0,
            9950.0,
            0.0,
            0.0,
            1,
            5,
            Local::now(),
        );
        repo.save_settlement(&result).unwrap();
        assert_eq!(repo.list_settlements().unwrap().len(), 1);

        let found = repo.find_settlement_by_trade("T-001").unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn memory_repository_ledger() {
        let repo = InMemoryRepository::new();
        let _fill = sample_fill();
        let entry = LedgerEntry::debit(
            "L-001".into(),
            "T-001".into(),
            "OMS-001".into(),
            "ACCT-MAIN".into(),
            50.0,
            0.02,
            10000.0,
            9949.98,
            "测试".into(),
            Local::now(),
        );
        repo.save_ledger_entry(&entry).unwrap();
        assert_eq!(repo.list_ledger_entries().unwrap().len(), 1);
        let by_order = repo.find_ledger_by_order("OMS-001").unwrap();
        assert_eq!(by_order.len(), 1);
    }

    #[test]
    fn csv_repository_read_write() {
        let dir = std::env::temp_dir();
        let fills = dir.join("test_settlement_fills.csv");
        let results = dir.join("test_settlement_results.csv");
        let ledger = dir.join("test_settlement_ledger.csv");

        // 清理旧文件
        let _ = std::fs::remove_file(&fills);
        let _ = std::fs::remove_file(&results);
        let _ = std::fs::remove_file(&ledger);

        let repo = CsvRepository::new(fills.clone(), results.clone(), ledger.clone());
        let fill = sample_fill();
        repo.save_fill_event(&fill).unwrap();

        let loaded = repo.list_fill_events().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].trade_id, "T-001");
    }

    #[test]
    fn create_repository_memory() {
        let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
        assert_eq!(repo.name(), "InMemory");
    }

    #[test]
    fn create_repository_csv() {
        let dir = std::env::temp_dir();
        let repo = create_repository(
            RepositoryType::Csv,
            Some(dir.join("f.csv")),
            Some(dir.join("s.csv")),
            Some(dir.join("l.csv")),
        )
        .unwrap();
        assert_eq!(repo.name(), "CSV");
    }
}
