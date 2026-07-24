//! 存储框架：统一的存储 trait 及多种后端实现。
//!
//! 从 `pm-oms::repository` 和 `pm-storage` 提取并统一。
//!
//! # 核心能力
//!
//! - [`Storage`] trait：统一的持久化接口
//! - [`MemoryStorage`]：内存存储
//! - [`CsvStorage`]：CSV 文件存储
//! - [`SqliteStorage`]：SQLite 存储（接口预留）
//!
//! # 未来扩展
//!
//! - PostgreSQL、ClickHouse（接口预留）

pub mod csv_util;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 存储健康状态
#[derive(Debug, Clone)]
pub struct StorageHealth {
    /// 是否健康
    pub healthy: bool,
    /// 健康信息
    pub message: String,
    /// 记录数
    pub record_count: u64,
}

impl StorageHealth {
    /// 中文摘要
    pub fn summary_zh(&self) -> String {
        format!(
            "存储状态: {}, 记录数: {}, 详情: {}",
            if self.healthy { "健康" } else { "异常" },
            self.record_count,
            self.message,
        )
    }
}

/// 存储后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageKind {
    /// 内存
    Memory,
    /// CSV 文件
    Csv,
    /// SQLite
    Sqlite,
    /// PostgreSQL（预留）
    Postgres,
    /// ClickHouse（预留）
    Clickhouse,
}

impl StorageKind {
    pub fn as_zh(&self) -> &'static str {
        match self {
            StorageKind::Memory => "内存",
            StorageKind::Csv => "CSV",
            StorageKind::Sqlite => "SQLite",
            StorageKind::Postgres => "PostgreSQL",
            StorageKind::Clickhouse => "ClickHouse",
        }
    }
}

/// 统一的存储 trait
///
/// 所有 Repository 统一依赖此接口，不得直接操作文件或数据库。
#[async_trait]
pub trait Storage: Send + Sync {
    /// 存储名称
    fn name(&self) -> &str;

    /// 存储路径（仅文件/数据库后端有值）
    fn storage_path(&self) -> Option<PathBuf>;

    /// 保存数据
    async fn save(&self, key: &str, data: &Value) -> anyhow::Result<()>;

    /// 加载数据
    async fn load(&self, key: &str) -> anyhow::Result<Option<Value>>;

    /// 删除数据
    async fn delete(&self, key: &str) -> anyhow::Result<bool>;

    /// 列出所有键
    async fn list_keys(&self) -> anyhow::Result<Vec<String>>;

    /// 记录总数
    async fn count(&self) -> anyhow::Result<u64>;

    /// 健康检查
    fn health(&self) -> StorageHealth;
}

/// 内存存储实现
pub struct MemoryStorage {
    name: String,
    data: Arc<Mutex<HashMap<String, Value>>>,
}

impl MemoryStorage {
    /// 创建新的内存存储
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    fn name(&self) -> &str {
        &self.name
    }

    fn storage_path(&self) -> Option<PathBuf> {
        None
    }

    async fn save(&self, key: &str, data: &Value) -> anyhow::Result<()> {
        let mut store = self.data.lock().unwrap_or_else(|e| e.into_inner());
        store.insert(key.to_string(), data.clone());
        Ok(())
    }

    async fn load(&self, key: &str) -> anyhow::Result<Option<Value>> {
        let store = self.data.lock().unwrap_or_else(|e| e.into_inner());
        Ok(store.get(key).cloned())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<bool> {
        let mut store = self.data.lock().unwrap_or_else(|e| e.into_inner());
        Ok(store.remove(key).is_some())
    }

    async fn list_keys(&self) -> anyhow::Result<Vec<String>> {
        let store = self.data.lock().unwrap_or_else(|e| e.into_inner());
        Ok(store.keys().cloned().collect())
    }

    async fn count(&self) -> anyhow::Result<u64> {
        let store = self.data.lock().unwrap_or_else(|e| e.into_inner());
        Ok(store.len() as u64)
    }

    fn health(&self) -> StorageHealth {
        let count = self.data.lock().unwrap_or_else(|e| e.into_inner()).len();
        StorageHealth {
            healthy: true,
            message: "内存存储正常运行".to_string(),
            record_count: count as u64,
        }
    }
}

/// CSV 文件存储实现
pub struct CsvStorage {
    name: String,
    path: PathBuf,
}

impl CsvStorage {
    /// 创建新的 CSV 存储
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            name: name.into(),
            path,
        })
    }
}

#[async_trait]
impl Storage for CsvStorage {
    fn name(&self) -> &str {
        &self.name
    }

    fn storage_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }

    async fn save(&self, key: &str, data: &Value) -> anyhow::Result<()> {
        let json = serde_json::to_string(data)?;
        let header = &["key", "data"];
        csv_util::ensure_csv(&self.path, header)?;
        let record = vec![key.to_string(), json];
        csv_util::append_records(&self.path, &[record])?;
        Ok(())
    }

    async fn load(&self, key: &str) -> anyhow::Result<Option<Value>> {
        // 读取整个 CSV 查找匹配的 key
        let content = std::fs::read_to_string(&self.path)?;
        let mut reader = csv::Reader::from_reader(content.as_bytes());
        for result in reader.records() {
            let record = result?;
            if record.get(0).map(|s| s == key).unwrap_or(false) {
                if let Some(data_str) = record.get(1) {
                    let value: Value = serde_json::from_str(data_str)?;
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }

    async fn delete(&self, _key: &str) -> anyhow::Result<bool> {
        // CSV 删除需要重写整个文件，简化实现为不支持
        Ok(false)
    }

    async fn list_keys(&self) -> anyhow::Result<Vec<String>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&self.path)?;
        let mut reader = csv::Reader::from_reader(content.as_bytes());
        let mut keys = Vec::new();
        for result in reader.records() {
            let record = result?;
            if let Some(key) = record.get(0) {
                keys.push(key.to_string());
            }
        }
        Ok(keys)
    }

    async fn count(&self) -> anyhow::Result<u64> {
        if !self.path.exists() {
            return Ok(0);
        }
        Ok(csv_util::count_rows(&self.path))
    }

    fn health(&self) -> StorageHealth {
        let dir_exists = self.path.parent().map(|p| p.exists()).unwrap_or(false);
        let file_exists = self.path.exists();
        let healthy = dir_exists; // 目录存在即可写入，文件可延迟创建

        StorageHealth {
            healthy,
            message: if file_exists {
                "CSV 存储文件存在".to_string()
            } else if dir_exists {
                "CSV 存储就绪（文件将在首次写入时创建）".to_string()
            } else {
                "CSV 存储目录不存在".to_string()
            },
            record_count: if file_exists {
                csv_util::count_rows(&self.path)
            } else {
                0
            },
        }
    }
}

/// SQLite 存储（接口预留，当前为桩实现）
pub struct SqliteStorage {
    name: String,
    path: PathBuf,
}

impl SqliteStorage {
    /// 创建 SQLite 存储桩
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
        }
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    fn name(&self) -> &str {
        &self.name
    }

    fn storage_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }

    async fn save(&self, _key: &str, _data: &Value) -> anyhow::Result<()> {
        anyhow::bail!("SQLite 存储尚未实现（接口预留）")
    }

    async fn load(&self, _key: &str) -> anyhow::Result<Option<Value>> {
        anyhow::bail!("SQLite 存储尚未实现（接口预留）")
    }

    async fn delete(&self, _key: &str) -> anyhow::Result<bool> {
        anyhow::bail!("SQLite 存储尚未实现（接口预留）")
    }

    async fn list_keys(&self) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("SQLite 存储尚未实现（接口预留）")
    }

    async fn count(&self) -> anyhow::Result<u64> {
        anyhow::bail!("SQLite 存储尚未实现（接口预留）")
    }

    fn health(&self) -> StorageHealth {
        StorageHealth {
            healthy: false,
            message: "SQLite 存储尚未实现（接口预留）".to_string(),
            record_count: 0,
        }
    }
}

/// 创建存储实例的工厂函数
pub fn create_storage(
    kind: StorageKind,
    path: Option<PathBuf>,
) -> anyhow::Result<Box<dyn Storage>> {
    match kind {
        StorageKind::Memory => Ok(Box::new(MemoryStorage::new("default"))),
        StorageKind::Csv => {
            let p = path.unwrap_or_else(|| PathBuf::from("data/infrastructure_storage.csv"));
            Ok(Box::new(CsvStorage::new("default-csv", p)?))
        }
        StorageKind::Sqlite => {
            let p = path.unwrap_or_else(|| PathBuf::from("data/infrastructure_storage.db"));
            Ok(Box::new(SqliteStorage::new("default-sqlite", p)))
        }
        _ => anyhow::bail!("存储类型 {:?} 尚未实现", kind.as_zh()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_storage_crud() {
        let store = MemoryStorage::new("test");
        let data = serde_json::json!({"price": 1.5, "qty": 100});
        store.save("order-1", &data).await.unwrap();

        let loaded = store.load("order-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap()["price"], 1.5);

        let count = store.count().await.unwrap();
        assert_eq!(count, 1);

        let keys = store.list_keys().await.unwrap();
        assert_eq!(keys, vec!["order-1"]);
    }

    #[tokio::test]
    async fn memory_storage_delete() {
        let store = MemoryStorage::new("test");
        store.save("k", &serde_json::json!("v")).await.unwrap();
        assert!(store.delete("k").await.unwrap());
        assert!(!store.delete("k").await.unwrap());
    }

    #[tokio::test]
    async fn memory_storage_health() {
        let store = MemoryStorage::new("test");
        let health = store.health();
        assert!(health.healthy);
        assert!(health.summary_zh().contains("健康"));
    }

    #[tokio::test]
    async fn csv_storage_create() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.csv");
        let store = CsvStorage::new("test-csv", &path).unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
        assert!(store.health().healthy);
    }

    #[test]
    fn storage_kind_zh() {
        assert_eq!(StorageKind::Memory.as_zh(), "内存");
        assert_eq!(StorageKind::Csv.as_zh(), "CSV");
        assert_eq!(StorageKind::Postgres.as_zh(), "PostgreSQL");
    }

    #[tokio::test]
    async fn factory_creates_memory_storage() {
        let store = create_storage(StorageKind::Memory, None).unwrap();
        assert_eq!(store.name(), "default");
        assert!(store.storage_path().is_none());
    }
}
