# OMS Repository（P2-04 第八节）

> 订单持久化层。统一抽象为 [`OrderRepository`](../../crates/oms/src/repository/mod.rs) trait。

---

## 1. 三种实现

| 实现 | 模块 | 适用 | 持久化 | 重启恢复 |
| --- | --- | --- | --- | --- |
| `InMemoryRepository` | `repository::memory` | 测试 / 实时运行 | ❌ | ❌ |
| `CsvRepository` | `repository::csv` | 生产 / 回放 | CSV | ✅ |
| `SqliteRepository` | `repository::sqlite` | P2-05+ | SQLite | ✅（接口预留） |

---

## 2. OrderRepository Trait

```rust
pub trait OrderRepository: Send + Sync {
    // ---- 订单 CRUD ----
    fn save(&self, order: &Order) -> anyhow::Result<()>;
    fn find_by_id(&self, order_id: &str) -> anyhow::Result<Option<Order>>;
    fn find_by_client_id(&self, client_order_id: &str) -> anyhow::Result<Option<Order>>;
    fn list_all(&self) -> anyhow::Result<Vec<Order>>;
    fn list_by_status(&self, status: OrderStatus) -> anyhow::Result<Vec<Order>>;
    fn list_active(&self) -> anyhow::Result<Vec<Order>>;
    fn list_in_range(&self, from: DateTime<Local>, to: DateTime<Local>) -> anyhow::Result<Vec<Order>>;
    fn count(&self) -> anyhow::Result<u64>;
    fn count_by_status(&self) -> anyhow::Result<Vec<(OrderStatus, u64)>>;
    fn delete(&self, order_id: &str) -> anyhow::Result<bool>;

    // ---- 状态变化追加 ----
    fn append_status_change(&self, order_id: &str, change: &StatusChange) -> anyhow::Result<()>;
    fn list_status_changes(&self, order_id: &str) -> anyhow::Result<Vec<StatusChange>>;

    // ---- 事件追加 ----
    fn append_event(&self, event: &OrderEvent) -> anyhow::Result<()>;
    fn list_events(&self) -> anyhow::Result<Vec<OrderEvent>>;

    // ---- 元信息 ----
    fn name(&self) -> &str;
    fn storage_path(&self) -> Option<PathBuf>;
    fn health(&self) -> RepositoryHealth;
}
```

---

## 3. InMemoryRepository

### 3.1 数据结构

```rust
pub struct InMemoryRepository {
    orders: Mutex<HashMap<String, Order>>,        // order_id → Order
    client_index: Mutex<HashMap<String, String>>, // client_order_id → order_id
    events: Mutex<Vec<OrderEvent>>,
    status_changes: Mutex<HashMap<String, Vec<StatusChange>>>,
}
```

### 3.2 特点

- 全部在内存中，读写 O(1) / O(n)
- 适合测试场景
- 程序退出数据丢失（除非搭配快照）
- `clear()` 用于测试重置

---

## 4. CsvRepository

### 4.1 文件布局

```
data/
├── oms_orders.csv            # 27 列订单快照
├── oms_status_changes.csv    # 状态变化记录
└── oms_events.csv            # OMS 事件流
```

### 4.2 orders.csv 表头

```csv
order_id,client_order_id,exchange_order_id,gateway_name,market_id,provider,
direction,side,price,quantity,order_type,time_in_force,status,filled,
remaining,avg_fill_price,slippage,created_at,updated_at,strategy_id,
risk_id,opportunity_id,version,retry_count,priority,notes,simulation_only
```

### 4.3 写策略：append-only

```rust
// 每次 save：
//   1. 更新内存 HashMap
//   2. append 一行到 CSV（OpenOptions::append(true)）
```

不就地更新已存在行 — 通过 `version` 字段实现乐观锁。

### 4.4 读策略：全量加载

启动时一次性加载所有订单到内存：

```rust
fn load_into_memory(&self) -> anyhow::Result<()> {
    let mut rdr = csv::Reader::from_path(&self.orders_path)?;
    for record in rdr.deserialize() {
        let record: OrderRecord = record?;
        let order = record.into_order();
        // 存入 HashMap
    }
    Ok(())
}
```

适用量级：< 10K 订单。大量订单应切换到 SQLite。

### 4.5 状态字段映射

CSV 中存储中文字段（人类可读）：

```rust
fn into_order(self) -> Order {
    let direction = match self.direction.as_str() {
        "YES" | "Yes" => Direction::Yes,
        _ => Direction::No,
    };
    let status = match self.status.as_str() {
        "已创建" => OrderStatus::Created,
        "已校验" => OrderStatus::Validated,
        "已提交" => OrderStatus::Submitted,
        "已接受" => OrderStatus::Accepted,
        "部分成交" => OrderStatus::PartiallyFilled,
        "完全成交" => OrderStatus::Filled,
        "已取消" => OrderStatus::Cancelled,
        "已拒绝" => OrderStatus::Rejected,
        "已过期" => OrderStatus::Expired,
        _ => OrderStatus::Completed,
    };
    // ...
}
```

---

## 5. SqliteRepository（接口预留）

### 5.1 当前状态

- ✅ Trait 方法定义完整
- ✅ 健康检查返回 "未实现"
- ✅ save() 返回错误
- ⏳ 真实实现待 P2-05+ 引入 `rusqlite`

### 5.2 未来表结构

```sql
CREATE TABLE orders (
    order_id TEXT PRIMARY KEY,
    client_order_id TEXT UNIQUE NOT NULL,
    exchange_order_id TEXT,
    gateway_name TEXT NOT NULL,
    market_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    direction TEXT NOT NULL,
    side TEXT NOT NULL,
    price REAL NOT NULL,
    quantity REAL NOT NULL,
    order_type TEXT NOT NULL,
    time_in_force TEXT NOT NULL,
    status TEXT NOT NULL,
    filled REAL NOT NULL DEFAULT 0,
    remaining REAL NOT NULL,
    avg_fill_price REAL NOT NULL DEFAULT 0,
    slippage REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    strategy_id TEXT NOT NULL,
    risk_id TEXT NOT NULL,
    opportunity_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    retry_count INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    simulation_only INTEGER NOT NULL DEFAULT 1,
    INDEX idx_status (status),
    INDEX idx_client_order_id (client_order_id),
    INDEX idx_created_at (created_at)
);

CREATE TABLE status_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id TEXT NOT NULL,
    from_status TEXT NOT NULL,
    to_status TEXT NOT NULL,
    at TEXT NOT NULL,
    reason TEXT,
    actor TEXT,
    FOREIGN KEY (order_id) REFERENCES orders(order_id),
    INDEX idx_order_id (order_id)
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_name_zh TEXT NOT NULL,
    order_id TEXT,
    extra_json TEXT,
    INDEX idx_order_id (order_id),
    INDEX idx_event_type (event_type)
);
```

---

## 6. 工厂函数

```rust
pub enum RepositoryType {
    Memory,
    Csv,
    Sqlite,
}

pub fn create_repository(
    repo_type: RepositoryType,
    orders_csv: Option<PathBuf>,
    events_csv: Option<PathBuf>,
    sqlite_path: Option<PathBuf>,
) -> anyhow::Result<Box<dyn OrderRepository>>
```

### 6.1 默认 CSV 路径

```rust
Memory  → 无文件
Csv     → data/oms_orders.csv + data/oms_events.csv
Sqlite  → data/oms.sqlite
```

### 6.2 自定义路径

```rust
let repo = create_repository(
    RepositoryType::Csv,
    Some(PathBuf::from("/var/log/oms/orders.csv")),
    Some(PathBuf::from("/var/log/oms/events.csv")),
    None,
)?;
```

---

## 7. 健康检查

```rust
pub struct RepositoryHealth {
    pub healthy: bool,
    pub message: String,
    pub order_count: u64,
    pub event_count: u64,
}

impl RepositoryHealth {
    pub fn summary_zh(&self) -> String {
        if self.healthy {
            format!("【{}】{} | 订单 {} | 事件 {}", self.message, "健康", self.order_count, self.event_count)
        } else {
            format!("【异常】{} | 订单 {} | 事件 {}", self.message, self.order_count, self.event_count)
        }
    }
}
```

CLI 输出：

```
Repository: 【健康】健康 | 订单 5 | 事件 5
```

---

## 8. 测试覆盖

参见 [`tests/recovery.rs`](../../crates/oms/tests/recovery.rs) 和 `repository::memory::tests` / `repository::csv::tests`：

- ✅ save / find_by_id / find_by_client_id
- ✅ list_all / list_by_status / list_active / list_in_range
- ✅ count / count_by_status
- ✅ delete
- ✅ append_status_change + persist
- ✅ append_event + list_events
- ✅ reload 跨实例持久化
- ✅ CSV ↔ Order 双向转换
- ✅ SQLite stub 健康检查返回异常

---

## 9. 总结

OMS Repository 具有以下特性：

- ✅ **统一接口**：trait 抽象，便于替换
- ✅ **三种实现**：Memory / CSV / SQLite（接口预留）
- ✅ **append-only**：CSV 不就地更新，靠 version 字段管理
- ✅ **启动加载**：CSV 在构造时一次性加载到内存
- ✅ **健康检查**：所有实现提供 health() 方法
- ✅ **可扩展**：未来可加 PostgreSQL 实现
