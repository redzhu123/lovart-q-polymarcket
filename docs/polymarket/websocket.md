# Polymarket WebSocket API

> 最后更新: 2026-07-23 | Base URL: `wss://ws-subscriptions-clob.polymarket.com`

---

## 1. 概述

Polymarket 通过 WebSocket 提供实时数据流，支持两个主要频道和一个辅助频道：

| 频道 | 端点 | 认证 | 用途 |
|------|------|------|------|
| **Market** | `/ws/market` | 无（公开） | 实时订单簿、价格变动、最新成交价、tick size 变更 |
| **User** | `/ws/user` | L2 API 凭证 | 用户订单和成交的实时状态更新 |
| RTDS | `/ws/live` | 可选 | 实时数据流（体育赛事等） |

---

## 2. Market Channel（公开频道）

### 2.1 连接

```
wss://ws-subscriptions-clob.polymarket.com/ws/market
```

无需认证。直接建立 WebSocket 连接后，发送订阅消息。

### 2.2 订阅消息

```json
{
  "assets_ids": ["TOKEN_ID_1", "TOKEN_ID_2"],
  "type": "market",
  "custom_feature_enabled": true
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `assets_ids` | `string[]` | 要订阅的 **Asset ID** 列表（ERC-1155 token ID，不是 condition ID） |
| `type` | `string` | 固定为 `"market"` |
| `custom_feature_enabled` | `bool` | 设为 `true` 启用额外事件：`best_bid_ask`, `new_market`, `market_resolved` |

> **重要**: Market channel 使用 **asset_id / token_id** 订阅，而 User channel 使用 **condition_id** 订阅。两者不同。

### 2.3 事件类型

#### `book` — 订单簿快照

**触发时机**: 初次订阅时 + 每次成交影响订单簿时

```json
{
  "event_type": "book",
  "asset_id": "12345",
  "market": "0xCONDITION_ID",
  "bids": [
    {"price": "0.43", "size": "100.5"},
    {"price": "0.42", "size": "200.0"}
  ],
  "asks": [
    {"price": "0.57", "size": "150.0"},
    {"price": "0.58", "size": "300.0"}
  ],
  "hash": "0x...",
  "timestamp": "1712345678000"
}
```

| 字段 | 说明 |
|------|------|
| `bids[]` | 买单列表（`price`, `size`），**需自行按 price 降序排序** |
| `asks[]` | 卖单列表（`price`, `size`），**需自行按 price 升序排序** |
| `hash` | 订单簿哈希值（用于校验数据完整性） |
| `timestamp` | 毫秒级 Unix 时间戳 |

#### `price_change` — 价格变动

**触发时机**: 订单被挂单或取消时

```json
{
  "event_type": "price_change",
  "asset_id": "12345",
  "market": "0xCONDITION_ID",
  "price_changes": [
    {
      "price": "0.44",
      "size": "50.0",
      "side": "BUY",
      "best_bid": "0.43",
      "best_ask": "0.57"
    }
  ],
  "timestamp": "1712345678000"
}
```

| 字段 | 说明 |
|------|------|
| `price_changes[].size` | 该价格水平的**当前总数量**。`"0"` 表示该价格水平**已被移除** |
| `price_changes[].side` | `"BUY"` 或 `"SELL"` |
| `best_bid` / `best_ask` | 更新后的最优买卖价 |

#### `last_trade_price` — 最新成交价

**触发时机**: Maker 和 Taker 订单匹配成交时

```json
{
  "event_type": "last_trade_price",
  "asset_id": "12345",
  "market": "0xCONDITION_ID",
  "price": "0.45",
  "side": "BUY",
  "size": "10.0",
  "fee_rate_bps": "0",
  "timestamp": "1712345678000"
}
```

#### `tick_size_change` — Tick Size 变更

**触发时机**: 订单簿价格触及 >0.96 或 <0.04 时

```json
{
  "event_type": "tick_size_change",
  "asset_id": "12345",
  "old_tick_size": "0.01",
  "new_tick_size": "0.001"
}
```

> **关键**: Bot 程序**必须**监听此事件。如果使用旧的 tick size 下订单，订单会被拒绝。

#### `best_bid_ask` — 最优报价变化 *(custom feature)*

```json
{
  "event_type": "best_bid_ask",
  "asset_id": "12345",
  "best_bid": "0.44",
  "best_ask": "0.56",
  "spread": "0.12"
}
```

#### `new_market` — 新市场创建 *(custom feature)*

```json
{
  "event_type": "new_market",
  "question": "Will BTC exceed $100K in 2025?",
  "assets_ids": ["12345", "12346"],
  "outcomes": ["Yes", "No"],
  "condition_id": "0x...",
  "clob_token_ids": ["12345", "12346"],
  "tags": ["crypto", "bitcoin"],
  "fee_schedule": {}
}
```

#### `market_resolved` — 市场结算 *(custom feature)*

```json
{
  "event_type": "market_resolved",
  "condition_id": "0x...",
  "winning_asset_id": "12345",
  "winning_outcome": "Yes"
}
```

### 2.4 动态订阅/取消订阅

无需断开重连即可修改订阅列表：

**订阅更多**:
```json
{
  "assets_ids": ["NEW_TOKEN_ID"],
  "operation": "subscribe",
  "custom_feature_enabled": true
}
```

**取消订阅**:
```json
{
  "assets_ids": ["OLD_TOKEN_ID"],
  "operation": "unsubscribe"
}
```

---

## 3. User Channel（认证频道）

### 3.1 连接

```
wss://ws-subscriptions-clob.polymarket.com/ws/user
```

### 3.2 订阅消息

```json
{
  "auth": {
    "apiKey": "your-api-key",
    "secret": "your-api-secret",
    "passphrase": "your-passphrase"
  },
  "markets": ["0xCONDITION_ID_1", "0xCONDITION_ID_2"],
  "type": "user"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | `string` | 固定为 `"user"` |
| `auth` | `object` | **必需**。L2 API 凭证 (`apiKey`, `secret`, `passphrase`) |
| `markets` | `string[]` | 可选。要订阅的 **Condition ID** 列表。省略则接收所有市场的事件。 |

> **安全警告**: API 凭证**绝不能**暴露在客户端代码中。User Channel 仅应从服务端环境连接。

### 3.3 事件类型

#### `order` — 订单生命周期事件

```json
{
  "event_type": "order",
  "order": {
    "id": "0xORDER_ID",
    "market": "0xCONDITION_ID",
    "asset_id": "12345",
    "side": "BUY",
    "price": "0.43",
    "original_size": "100",
    "size_matched": "0",
    "type": "PLACEMENT",
    "outcome": "YES"
  },
  "timestamp": "1712345678000"
}
```

| `type` 值 | 说明 |
|-----------|------|
| `PLACEMENT` | 订单已提交并被接受 |
| `UPDATE` | 订单部分成交（`size_matched` 更新） |
| `CANCELLATION` | 订单已被取消 |

#### `trade` — 成交生命周期事件

```json
{
  "event_type": "trade",
  "trade": {
    "id": "0xTRADE_ID",
    "market": "0xCONDITION_ID",
    "asset_id": "12345",
    "side": "BUY",
    "size": "10.0",
    "price": "0.43",
    "status": "CONFIRMED",
    "maker_orders": [
      {
        "order_id": "0xORDER_ID_A",
        "matched_amount": "5.0",
        "price": "0.43"
      }
    ],
    "outcome": "YES"
  }
}
```

| `status` 值 | 说明 |
|------------|------|
| `MATCHED` | 订单已匹配（链下） |
| `MINED` | 交易已提交到 Polygon 内存池 |
| `CONFIRMED` | 交易已上链确认 |
| `RETRYING` | 交易提交失败，正在重试 |
| `FAILED` | 交易失败（需人工介入） |

**成交状态流转**:
```
MATCHED → MINED → CONFIRMED     (正常路径)
MATCHED → RETRYING → MINED → CONFIRMED  (重试成功)
MATCHED → RETRYING → FAILED     (重试失败，需关注)
```

### 3.4 动态订阅

```json
{
  "markets": ["0xNEW_CONDITION_ID"],
  "operation": "subscribe"
}

{
  "markets": ["0xOLD_CONDITION_ID"],
  "operation": "unsubscribe"
}
```

---

## 4. 连接管理

### 4.1 Keep-Alive

客户端必须每 **~10 秒**发送一个 PING 帧：

```
→ PING
← PONG
```

如果在 ~30 秒内未收到 PONG，应断开并重连。

### 4.2 重连策略

```
reconnect_delay = min(60, 1 * 2^retry_count)  // 指数退避，最大 60 秒
max_retries = 无限制（持续重连）
```

### 4.3 心跳示例 (Rust)

```rust
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message;

let mut heartbeat = interval(Duration::from_secs(10));
loop {
    tokio::select! {
        _ = heartbeat.tick() => {
            ws_sink.send(Message::Ping(vec![])).await?;
        }
        msg = ws_stream.next() => {
            // 处理消息
        }
    }
}
```

---

## 5. Market Channel 事件与 REST 端点的对应

| WebSocket 事件 | 等价 REST 端点 | 推荐用法 |
|---------------|---------------|----------|
| `book` | `GET /book` | **首选 WebSocket**（避免轮询 book） |
| `price_change` | 无直接等价 | **仅 WebSocket** |
| `last_trade_price` | `GET /last-trade-price` | WebSocket 用于实时，REST 用于初始化 |
| `tick_size_change` | `GET /tick-size` | **必须监听** WebSocket，否则 tick size 可能过时 |
| `best_bid_ask` | `GET /price` | WebSocket 用于实时顶部报价 |

---

## 6. 在当前项目中的接入位置

### 6.1 已有接入

当前项目中**尚无 WebSocket 接入**。所有数据通过 REST 轮询获取。

### 6.2 需要新增

| 组件 | 建议路径 | 用途 |
|------|----------|------|
| Market WS Client | `crates/scanner/src/datasource/ws_market.rs` | 订阅订单簿和价格，替代 REST 轮询 |
| User WS Client | `crates/execution/src/ws_user.rs` | 监听成交和订单状态更新 |
| WS 连接管理 | `crates/trading/src/connection.rs` | 重连、心跳、连接健康检查 |

### 6.3 与现有架构的集成

WebSocket 可以作为 `MarketDataProvider` trait 的新实现：

```rust
// 提案：WsMarketProvider 实现 MarketDataProvider
pub struct WsMarketProvider {
    asset_ids: Vec<String>,
    orderbooks: Arc<RwLock<HashMap<String, OrderBook>>>,
    // ...
}

impl MarketDataProvider for WsMarketProvider {
    fn name(&self) -> &str { "ws-market" }
    fn capability(&self) -> ProviderCapability { /* 全部支持 */ }
    async fn fetch_orderbooks(&self, ids: &[String]) -> Result<Vec<OrderBook>> {
        // 从内存缓存读取（由 WS 持续更新）
    }
    // ...
}
```

---

## 7. 推荐策略

### 7.1 初始化流程

```
1. 通过 REST 获取初始数据（市场列表、token ID 列表）
2. 建立 Market WS 连接，订阅全部关注的 token ID
3. 建立 User WS 连接（如有交易），订阅全部 Condition ID
4. WS 持续更新本地缓存
5. Scanner 从本地缓存读取（零延迟）
```

### 7.2 降级策略

当 WebSocket 断开时，自动回退到 REST 轮询模式（当前的 `GammaProvider` + `ClobProvider`）。

---

## 8. 未来扩展建议

1. **差别订阅**: 热门市场用 WS 实时更新，冷门市场用 REST 低频轮询
2. **book 增量更新**: 对 `price_change` 事件做增量更新而非替换整个订单簿
3. **事件时间线重建**: 将 `trade` 和 `order` 事件关联，重建完整的订单生命周期时间线
4. **L2 OrderBook**: 构建本地 L2 订单簿（聚合多级深度），用于滑点估算
5. **Market 监控频道**: 当 WS 不可用时自动切换到 REST（`/health` 监控 + 自动切换）
