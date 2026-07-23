# Polymarket Gateway 对接方案

> 最后更新: 2026-07-23 | 核心文档：真实交易网关的架构设计与迁移方案

---

## 1. 当前 Gateway 架构概览

当前项目有三层 "Gateway" 抽象：

### 1.1 数据源层 (Scanner Data)

```
MarketDataProvider trait
├── GammaProvider   (gamma-api.polymarket.com)     ← 当前活跃
├── ClobProvider    (clob.polymarket.com)           ← 已实现，未启用
└── MockProvider    (离线硬编码数据)                  ← 测试用
```

### 1.2 执行层 (Execution)

```
ExecutionGateway trait
└── MockGateway     (概率模拟成交)
```

### 1.3 交易层 (Trading)

```
TradingProvider trait
└── MockTradingProvider   (模拟交易环境)
```

```
crates/gateway/ ExchangeGateway trait  (空壳，待实现)
```

---

## 2. 三层抽象的统一

当前 6 个 trait 的职责划分及整合方案：

| 层级 | 当前 Trait | 核心职责 | 建议整合方向 |
|------|-----------|----------|-------------|
| 数据 | `MarketDataProvider` | 市场数据获取（markets, orderbooks, prices） | 保留，新增 `WsMarketProvider` 实现 |
| 数据 | `ClobProvider` (impl) | CLOB REST 订单簿 | 合并到 `ClobMarketDataProvider` |
| 执行 | `ExecutionGateway` | 订单提交/取消/状态查询 | 扩展为真实 CLOB 对接，新增 `ClobExecutionGateway` |
| 交易 | `TradingProvider` | 会话/认证/账户/心跳 | 新增 `PolymarketTradingProvider` |
| 交易 | `ExchangeGateway` | 底层交易所操作（空壳） | 合并到 `ExecutionGateway` |
| 工具 | `CredentialManager` | 凭证管理 | 保留并增强 |

**建议**: 减少 trait 层次，合并为**两个核心 trait**：

```rust
// 数据层
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn fetch_markets(&self) -> Result<FetchResult>;
    async fn fetch_orderbooks(&self, ids: &[String]) -> Result<Vec<OrderBook>>;
    async fn fetch_prices(&self, ids: &[String]) -> Result<Vec<PriceQuote>>;
    async fn health_check(&self) -> Result<HealthProbe>;
    fn capability(&self) -> ProviderCapability;
    fn name(&self) -> &str;
}

// 执行+交易层（统一）
#[async_trait]
pub trait ExecutionGateway: Send + Sync {
    // 会话
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    fn is_connected(&self) -> bool;

    // 账户
    async fn get_balance(&self) -> Result<Balance>;
    async fn get_positions(&self) -> Result<Vec<Position>>;

    // 订单
    async fn submit_order(&self, order: SignedOrder) -> Result<GatewayResult>;
    async fn cancel_order(&self, order_id: &str) -> Result<GatewayResult>;
    async fn cancel_all_orders(&self, market_id: Option<&str>) -> Result<GatewayResult>;
    async fn get_order(&self, order_id: &str) -> Result<OrderStatus>;
    async fn get_open_orders(&self, market_id: Option<&str>) -> Result<Vec<Order>>;

    // 元数据
    fn name(&self) -> &str;
    fn capability(&self) -> GatewayCapability;
    fn is_live(&self) -> bool;
    async fn health_check(&self) -> Result<HealthProbe>;
}
```

---

## 3. Mock Gateway vs 真实 Gateway 差异分析

### 3.1 功能差异矩阵

| 功能 | MockGateway | 真实 Gateway (CLOB) | 差异级别 |
|------|------------|---------------------|----------|
| **网络** | 无 HTTP 调用 | REST + WebSocket | 🔴 重大 |
| **认证** | 无需认证 | L1 EIP-712 + L2 HMAC | 🔴 重大 |
| **订单提交** | 直接返回结果 | POST /order → 异步状态 | 🔴 重大 |
| **订单状态** | 立即确定 | 异步更新（WS 或轮询） | 🔴 重大 |
| **EIP-712 签名** | 不签名 | 每个订单必须签名 | 🔴 重大 |
| **取消订单** | 模拟状态切换 | DELETE /order | 🟡 中等 |
| **余额查询** | 返回 10,000 固定值 | GET /balances + Data API | 🟡 中等 |
| **订单簿** | 无 | GET /book (不预排序) | 🟡 中等 |
| **Tick Size** | 无验证 | 必须对齐 tick_size | 🟡 中等 |
| **价格精度** | f64 直接使用 | 10⁶ 整数精度 + tick 对齐 | 🟡 中等 |
| **生命周期** | 瞬时 | 跨数秒（匹配+链上确认） | 🔴 重大 |
| **错误处理** | 本地模拟错误 | 真实 HTTP 错误码 | 🟡 中等 |
| **WebSocket** | 无 | Market + User 频道 | 🟡 中等 |
| **速率限制** | 无限制 | Token Bucket 限流 | 🟡 中等 |
| **链上交易** | 无 | Polygon 交易 → 需 MATIC Gas | 🔴 重大 |
| **pUSD 授权** | 跳过 | 必须 approve | 🔴 重大 |

### 3.2 数据流差异

**MockGateway 数据流** (完全同步):
```
submit_order(request) → 立即返回 (accepted | filled | rejected)
                         ↑ 概率决定 + 随机延迟
```

**真实 Gateway 数据流** (异步):
```
submit_order(signed_order)
  → POST /order (L2 认证)
    → CLOB 返回 accepted
      → 后台 WS 监听 order/trade 事件
        → 状态更新: PARTIAL → FILLED
          → 等待链上 CONFIRMED
            → 完成
```

### 3.3 核心差异总结

MockGateway 的核心假设（**必须打破的**）：
1. ❌ "提交订单 = 立即知道最终结果"
2. ❌ "不需要签名"
3. ❌ "不需要认证"
4. ❌ "不区分链下匹配和链上结算"
5. ❌ "不需要代币授权"
6. ❌ "不需要关心 tick size"
7. ❌ "余额不会变化"

---

## 4. 真实 Gateway 设计

### 4.1 建议模块结构

```
crates/gateway/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块注册
    ├── traits.rs           # ExecutionGateway trait (统一接口) [待重新设计]
    ├── types.rs            # 共享类型 (SignedOrder, Balance, Position, GatewayCapability)
    ├── mock.rs             # MockGateway (保留，用于测试和 dry-run)
    ├── clob/
    │   ├── mod.rs
    │   ├── client.rs       # HTTP 客户端（L2 签名、请求重试）
    │   ├── auth.rs         # L1 凭证引导 + L2 HMAC 构造
    │   ├── order.rs        # 订单提交/取消/查询
    │   ├── account.rs      # 余额/授权查询
    │   ├── signing.rs      # EIP-712 订单签名
    │   └── types.rs        # CLOB 特定 DTO (request/response 结构)
    ├── ws/
    │   ├── mod.rs
    │   ├── market.rs       # Market Channel 客户端
    │   └── user.rs         # User Channel 客户端
    └── data_api/
        ├── mod.rs
        └── client.rs       # Data API 客户端（positions, trades, activity, value）
```

### 4.2 RealGateway 设计

```rust
pub struct RealGateway {
    config: GatewayConfig,
    credential: Credential,
    http_client: Client,           // reqwest
    clob_base_url: String,         // https://clob.polymarket.com
    data_api_base_url: String,     // https://data-api.polymarket.com
    signer: Box<dyn OrderSigner>,  // EIP-712 签名器
    rate_limiter: RateLimiter,     // Token bucket
    ws_market: Option<WsMarketClient>,
    ws_user: Option<WsUserClient>,
    state: Arc<RwLock<GatewayState>>,
}

pub struct GatewayConfig {
    pub clob_base_url: String,
    pub chain_id: u64,               // 137
    pub ctf_exchange_address: String,
    pub neg_risk_address: String,
    pub signature_type: u8,          // 0-3
    pub live_enabled: bool,          // 安全开关
    pub max_retries: u32,            // 请求重试次数
    pub request_timeout_ms: u64,
    pub ws_enabled: bool,
    pub cache_tick_size: bool,
    pub cache_neg_risk: bool,
}

impl ExecutionGateway for RealGateway {
    // 每个方法实际调用 CLOB API
    async fn submit_order(&self, order: SignedOrder) -> Result<GatewayResult> {
        // 1. 检查 live_enabled 安全开关
        // 2. 速率限制器 try_acquire
        // 3. 构造 L2 认证头
        // 4. POST /order
        // 5. 解析响应
        // 6. 返回 GatewayResult (Accepted / Rejected)
    }
}
```

### 4.3 安全开关

```rust
impl RealGateway {
    fn guard_live(&self) -> Result<()> {
        if !self.config.live_enabled {
            return Err(anyhow!(
                "LIVE_GATEWAY_DISABLED: 真实交易未启用。设置 gateway.live_enabled=true 以启用。"
            ));
        }
        Ok(())
    }
}
```

所有状态变更方法（submit_order, cancel_order 等）的第一行调用 `self.guard_live()?`。

---

## 5. 在当前项目中的接入位置

### 5.1 需要修改的文件

| 文件 | 改动 | 优先级 |
|------|------|--------|
| `crates/execution/src/gateway.rs` | `ExecutionGateway` trait 扩展 | P0 |
| `crates/execution/src/gateway.rs` | 新增 `ClobGateway` 或 `RealGateway` | P0 |
| `crates/execution/src/order.rs` | Order 增加 `salt`, `signature`, `signature_type`, `token_id` 字段 | P0 |
| `crates/execution/src/builder.rs` | OrderBuilder 增加精度转换、tick 对齐 | P0 |
| `crates/execution/src/validator.rs` | 新增 `TickSizeRule`, `AllowanceRule`, `BalanceRule` | P1 |
| `crates/execution/src/pipeline.rs` | Pipeline 支持异步状态更新（WS 驱动） | P1 |
| `crates/execution/src/scheduler.rs` | 适配真实 CLOB 限流参数 | P2 |
| `crates/trading/src/credential.rs` | 集成 L1 API Key 创建流程 | P0 |
| `crates/trading/src/session.rs` | L2 HMAC 签名中间件 | P0 |
| `crates/trading/src/provider.rs` | `PolymarketTradingProvider` 实现 | P1 |
| `crates/scanner/src/datasource/` | 新增 `ClobMarketDataProvider`（完整 CLOB 数据） | P2 |
| `apps/cli/src/main.rs` | 新增 `live` 或 `trade` CLI 模式 | P2 |

### 5.2 依赖新增

```toml
# Cargo.toml (workspace)
ethers = { version = "2", features = ["ethers-solc"] }  # EIP-712 签名
hmac = "0.12"        # L2 认证
sha2 = "0.10"        # L2 认证
hex = "0.4"          # HMAC hex 编码
base64 = "0.22"      # secret 解码
rand = "0.8"         # salt 生成
tokio-tungstenite = "0.24"  # WebSocket
```

---

## 6. 迁移策略：Mock → 真实

### 6.1 渐进式迁移路径

```
Phase A: 影子模式 (Shadow Mode)
  ├── 真实 Gateway 接收请求但不发送到 CLOB
  ├── 日志记录 "如果启用真实交易，将提交订单: {...}"
  ├── 目的: 验证数据格式、签名逻辑、认证流程
  └── 风险: 零（不上链）

Phase B: 只读模式 (Read-Only Mode)
  ├── 真实 Gateway 执行所有查询操作
  │   ├── GET /balances  → 真实余额
  │   ├── GET /tick-size  → 真实 tick size
  │   ├── GET /book       → 真实订单簿
  │   └── GET /orders     → 真实订单状态
  ├── 下单/撤单继续使用 MockGateway
  ├── 目的: 验证数据流、缓存策略、错误处理
  └── 风险: 极低（只读操作）

Phase C: 沙盒模式 (Sandbox Mode)
  ├── 真实下单但极小额（如 1 pUSD）
  ├── 即时撤单（或使用 FAK 类型）
  ├── 目的: 端到端验证签名+提交流程
  └── 风险: 极低（金额微不足道）

Phase D: 影子交易 (Paper Trading)
  ├── 真实 Gateway 全量运行
  ├── 但 portfolio 使用虚拟资金
  ├── 目的: 在真实市场环境中验证策略
  └── 风险: 低（资金虚拟，但市场数据真实）

Phase E: 小额真金 (Small Live)
  ├── 极小的真实资金（如 100 pUSD）
  ├── 严格的仓位和亏损限制
  ├── 目的: 验证完整交易循环
  └── 风险: 可控（小额 + 严格风控）

Phase F: 全量交易 (Full Live)
  ├── 全部功能启用
  ├── 持续监控 + 熔断机制
  └── 风险: 标准交易风险
```

### 6.2 配置驱动的切换

```toml
# config.toml (后续版本)
[gateway]
mode = "mock"            # "mock" | "shadow" | "readonly" | "paper" | "live"
live_enabled = false     # 总安全开关（必须显式设为 true 才能真实交易）

[gateway.mock]
fill_probability = 0.7
partial_fill_probability = 0.3

[gateway.clob]
base_url = "https://clob.polymarket.com"
chain_id = 137
signature_type = 3      # POLY_1271
request_timeout_ms = 10000
max_retries = 3

[gateway.ws]
enabled = false
market_url = "wss://ws-subscriptions-clob.polymarket.com/ws/market"
user_url = "wss://ws-subscriptions-clob.polymarket.com/ws/user"
```

---

## 7. 未来扩展建议

1. **Gateway 中间件层**: 统一的日志、指标、限流、重试中间件
2. **多交易所支持**: `ExecutionGateway` trait 设计足够通用，未来可支持其他预测市场
3. **Order SOR (Smart Order Router)**: 自动选择最佳执行路径
4. **回测一致性**: 确保 MockGateway 和 RealGateway 对相同的输入产生可比较的输出
5. **熔断器**: 连续失败 N 次后自动降级到 MockGateway
