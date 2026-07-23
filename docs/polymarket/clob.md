# Polymarket CLOB API 参考

> 最后更新: 2026-07-23 | Base URL: `https://clob.polymarket.com` | 适用版本: CLOB V2 (2026-04-28+)

---

## 1. 概述

CLOB (Central Limit Order Book) API 是 Polymarket 的核心交易接口。订单匹配在链下进行，结算在 Polygon 链上通过 CTF Exchange 合约完成。

**关键变更 (2026-04-28)**:
- 抵押品从 USDC.e 迁移至 **pUSD** (Polymarket USD，1:1 USDC 支持)
- EIP-712 domain 版本升级为 `"2"`
- Exchange V3 合约部署在新的地址
- 旧版 `py-clob-client` / `clob-client` SDK 已废弃，必须使用 `-v2` 版本

---

## 2. 公开端点（无需认证）

### 2.1 服务器状态

#### `GET /` — 健康检查
- **用途**: 检查服务是否在线
- **响应**: `"OK"` (text/plain)
- **限制**: 100 req / 10s

#### `GET /time` — 服务器时间
- **用途**: 同步服务器时间，用于 L2 签名 timestamp 校准
- **响应**: `{ "timestamp": 1712345678 }`
- **限制**: 通用限制

### 2.2 市场信息

#### `GET /markets` — 市场列表
- **用途**: 获取所有活跃市场
- **参数**: 分页参数（具体格式待官方文档确认）
- **响应**: 市场列表，包含 `condition_id`, `tokens`, `neg_risk` 等
- **限制**: 通用限制
- **接入位置**: `crates/scanner/src/datasource/clob.rs` → `ClobProvider`（需扩展）

#### `GET /market` — 单个市场
- **用途**: 按 condition_id 查询单个市场详情
- **请求**: `GET /market?condition_id=0x...`
- **响应**: 市场详情，含 `condition_id`, `tokens[]`, `neg_risk`, `minimum_tick_size`
- **限制**: 通用限制

#### `GET /simplified-markets` — 简化市场列表
- **用途**: 获取精简市场数据（字段更少，加载更快）
- **请求**: `GET /simplified-markets?next_cursor=...`
- **响应**: `{ "markets": [...], "next_cursor": "..." }`
- **限制**: 通用限制
- **说明**: 推荐用于批量扫描场景，响应体积更小

### 2.3 订单簿

#### `GET /book` — 单个订单簿
- **用途**: 获取指定 token 的完整订单簿深度
- **请求**: `GET /book?token_id=<token-id>`
- **响应**:
  ```json
  {
    "bids": [{"price": "0.43", "size": "100.0"}, ...],
    "asks": [{"price": "0.57", "size": "200.0"}, ...],
    "tick_size": "0.01",
    "neg_risk": false,
    "hash": "0x..."
  }
  ```
- **限制**: 1,500 req / 10s
- **重要**: 返回的 bids/asks **不保证排序**。客户端必须自行排序：bids 按 price 降序，asks 按 price 升序。
- **接入位置**: `crates/scanner/src/datasource/clob.rs` → `ClobProvider::fetch_orderbooks()`

#### `GET /books` — 批量订单簿
- **用途**: 一次获取多个 token 的订单簿（最多 500 个 token）
- **请求**: `POST /books` (传递 token_id 数组)
- **响应**: 多个订单簿的数组
- **限制**: 500 req / 10s

### 2.4 价格

#### `GET /price` — 最优报价
- **用途**: 获取指定 token 的最优买价或卖价
- **请求**: `GET /price?token_id=<token-id>&side=buy|sell`
- **响应**: `{ "price": "0.43", "side": "BUY", "token_id": "..." }`
- **限制**: 1,500 req / 10s

#### `GET /prices` — 批量价格
- **用途**: 一次获取多个 token 的最优买/卖价（最多 500 个）
- **限制**: 500 req / 10s

#### `GET /midpoint` — 中间价
- **用途**: 最优买价和卖价的平均值
- **请求**: `GET /midpoint?token_id=<token-id>`
- **响应**: `{ "mid": "0.50" }`
- **限制**: 1,500 req / 10s

#### `GET /spread` — 价差
- **用途**: 获取买卖价差
- **请求**: `GET /spread?token_id=<token-id>`
- **响应**: `{ "spread": "0.02", "bid": "0.49", "ask": "0.51" }`
- **限制**: 通用限制

#### `GET /last-trade-price` — 最新成交价
- **用途**: 获取最近一笔成交价
- **限制**: 通用限制

#### `GET /prices-history` — 历史价格
- **用途**: 获取 ~30 天滚动窗口的历史价格数据
- **请求**: `GET /data/price-history?market=<condition_id>`
- **响应**: `[{t: unix_timestamp, p: price}, ...]`
- **限制**: 1,000 req / 10s

### 2.5 市场参数

#### `GET /tick-size` — 最小价格单位
- **用途**: 获取市场的 tick size（价格必须是 tick_size 的整数倍）
- **请求**: `GET /tick-size?token_id=<token-id>`
- **响应**: `{ "tick_size": "0.01", "minimum_tick_size": "0.01" }`
- **限制**: 通用限制（缓存）

#### `GET /neg-risk` — NegRisk 标志
- **用途**: 检查市场是否使用 NegRisk 适配器（多结果互斥市场）
- **响应**: `{ "neg_risk": true|false }`
- **限制**: 通用限制（缓存）

#### `GET /fee-rate-bps` — 费率
- **用途**: 获取当前交易费率（基点, basis points）
- **响应**: `{ "fee_rate_bps": "0" }`
- **限制**: 通用限制（缓存）

> **CLOB V2 注意**: V2 中费用由协议动态确定（taker-only），不再嵌入订单。

#### `GET /geoblock` — 地理限制检查
- **用途**: 检查请求来源是否被地理封锁
- **响应**: `{ "blocked": false, "country": "..." }`

---

## 3. 认证端点（需要 L2 HMAC 认证）

### 3.1 订单管理

#### `POST /order` — 下单
- **用途**: 提交一个新订单
- **认证**: L2 HMAC + 订单 EIP-712 签名
- **请求体**:
  ```json
  {
    "order": {
      "salt": 123456789,
      "maker": "0x...",
      "signer": "0x...",
      "taker": "0x0000000000000000000000000000000000000000",
      "tokenId": "12345",
      "makerAmount": "100000000",
      "takerAmount": "50000000",
      "expiration": "0",
      "nonce": "0",
      "feeRateBps": "0",
      "side": "BUY",
      "signatureType": 3
    },
    "signature": "0x...",
    "orderType": "GTC"
  }
  ```
- **响应**: `{ "orderID": "...", "status": "...", "transactions": [...] }`
- **限制**: 5,000 req / 10s (burst), 120,000 req / 10min (持续)
- **错误码**: 见 [错误码章节](#7-常见错误码)
- **接入位置**: `crates/execution/src/gateway.rs` → `ExecutionGateway::submit_order()`

#### `DELETE /order` — 撤单
- **用途**: 取消指定订单
- **请求**: `DELETE /order` + body `{ "orderID": "0x..." }`
- **响应**: `{ "cancelled": [...], "success": true }`
- **限制**: 5,000 req / 10s (burst), 120,000 req / 10min (持续)

#### `DELETE /orders` — 批量撤单
- **用途**: 一次取消多个订单
- **请求**: `DELETE /orders` + body `{ "orderIDs": ["0x...", "0x..."] }`
- **限制**: 通用限制

#### `DELETE /cancel-market-orders` — 撤销市场所有订单
- **用途**: 取消指定市场中用户的所有活跃订单
- **请求**: `DELETE /cancel-market-orders` + body `{ "market": "0xCONDITION_ID" }`
- **限制**: 1,500 req / 10s

#### `DELETE /cancel-all` — 撤销全部订单
- **用途**: 取消用户在所有市场中的所有活跃订单
- **限制**: 250 req / 10s

#### `GET /orders` — 查询订单
- **用途**: 获取用户的活跃订单列表
- **请求**: 可选参数 `market`, `asset_id`, `limit`
- **响应**: 订单列表
- **接入位置**: `crates/execution/src/gateway.rs` → `ExecutionGateway::order_status()`

#### `GET /trades` — 交易历史
- **用途**: 获取用户的成交历史
- **请求**: 可选参数 `market`, `asset_id`, `limit`, `starting_after`
- **响应**: 成交列表

### 3.2 账户

#### `GET /balances` — 余额与授权
- **用途**: 查询 USDC/pUSD 余额和交易所授权额度
- **响应**:
  ```json
  {
    "balance": "10000000000",
    "allowance": "10000000000"
  }
  ```
- **接入位置**: `crates/trading/src/provider.rs` → `TradingProvider::account()`

### 3.3 API Key 管理

#### `GET /api-keys` — 列出 API Keys
#### `POST /create-api-key` — 创建新 API Key
#### `DELETE /delete-api-key` — 删除 API Key

### 3.4 Rewards 与通知

#### `GET /rewards/current` / `GET /rewards/percentages`
#### `GET /order-scoring` — 订单评分
#### `GET /notifications` / `POST /mark-notifications-as-read` / `DELETE /drop-notifications`

---

## 4. 订单类型

| 类型 | 全称 | 行为 |
|------|------|------|
| **GTC** | Good-Till-Cancelled | 挂单直到成交或主动取消 |
| **GTD** | Good-Till-Date | 挂单直到指定过期时间 |
| **FOK** | Fill-Or-Kill | 全部立即成交或完全取消 |
| **FAK** | Fill-And-Kill | 尽可能立即成交，剩余取消（用于市价单效果） |

---

## 5. 签名类型

| 值 | 名称 | 钱包类型 |
|----|------|----------|
| `0` | EOA | 标准 ECDSA 钱包（MetaMask 等，**CLOB V2 已禁用**） |
| `1` | POLY_PROXY | Polymarket Proxy 钱包（Email/Magic 登录） |
| `2` | POLY_GNOSIS_SAFE | Gnosis Safe 多签钱包 |
| `3` | POLY_1271 | Deposit Wallet（新用户推荐，支持 ERC-1271） |

---

## 6. 响应头信息

| Header | 说明 |
|--------|------|
| `Retry-After` | 503 时返回，表示建议重试等待秒数 |
| `X-RateLimit-Remaining` | 剩余请求配额 |
| `X-RateLimit-Reset` | 配额重置时间 |

---

## 7. 常见错误码

| HTTP | 错误信息 | 原因 |
|------|----------|------|
| 400 | `Invalid order payload` | 请求体格式错误 |
| 400 | `order {id} is invalid. Price ({p}) breaks minimum tick size rule: {t}` | 价格不符合 tick size |
| 400 | `order {id} is invalid. Size ({s}) lower than the minimum: {m}` | 数量低于最小限制 |
| 400 | `invalid post-only order: order crosses book` | Post-only 订单会立即成交 |
| 400 | `not enough balance / allowance` | pUSD 余额不足或未授权 |
| 400 | `invalid signature` | EIP-712 签名验证失败 |
| 400 | `order canceled in the CTF exchange contract` | 订单已在链上取消 |
| 400 | `maker address not allowed, please use the deposit wallet flow` | EOA 直接交易已禁用 |
| 400 | `the order signer address has to be the address of the API KEY` | 签名地址与 API Key 不匹配 |
| 400 | `order {id} is invalid. Duplicated` | 重复订单 |
| 401 | `Unauthorized` / `Invalid api key` | API Key 无效或过期 |
| 401 | `Invalid L1 Request headers` | HMAC 签名错误 |
| 404 | `not found` | 市场/订单/token 不存在 |
| 425 | `Too Early` | 匹配引擎重启中 |
| 429 | `Too Many Requests` | 触发频率限制 |
| 500 | `Internal Server Error` | 服务器内部错误 |
| 503 | `Trading is currently cancel-only` | 交易暂停，仅允许撤单 |
| 503 | `post-only mode` | 仅允许 post-only 订单和撤单 |

完整错误码参考见 [ratelimit.md](ratelimit.md) 和官方文档。

---

## 8. 在当前项目中的接入

### 8.1 已有接入

| 位置 | 说明 |
|------|------|
| `crates/scanner/src/datasource/clob.rs` | `ClobProvider` — 调用 `GET /orderbook` 获取订单簿（最多 10 级深度） |

### 8.2 待接入

| 端点 | 需要接入的模块 | 用途 |
|------|---------------|------|
| `GET /book` | `crates/orderbook/` | 替换当前 `/orderbook`，获取完整订单簿 |
| `GET /books` | `crates/orderbook/` | 批量订单簿，减少请求数 |
| `GET /price` / `GET /prices` | `crates/scanner/` | 补充 Gamma 缺失的真实报价 |
| `GET /markets` | `crates/scanner/src/datasource/clob.rs` | CLOB 端的市场列表 |
| `GET /time` | `crates/trading/` | 时间同步 |
| `GET /tick-size` | `crates/execution/src/validator.rs` | 订单 tick size 验证 |
| `POST /order` | `crates/execution/src/gateway.rs` | 真实下单 |
| `DELETE /order` | `crates/execution/src/gateway.rs` | 真实撤单 |
| `GET /orders` | `crates/execution/src/gateway.rs` | 订单状态查询 |
| `GET /balances` | `crates/trading/src/provider.rs` | 账户余额 |
| `GET /fee-rate-bps` | `crates/execution/` | 费率信息 |

---

## 9. 未来扩展建议

1. **批量端点优先**: `GET /books`, `GET /prices`, `DELETE /orders` 可大幅减少 HTTP 请求数
2. **tick-size 缓存**: `GET /tick-size` 和 `GET /neg-risk` 的返回值很少变化，应在本地缓存
3. **价格历史**: `GET /prices-history` 可用于构建波动率模型和历史回测
4. **时间同步守护**: 后台定时调用 `GET /time` 维护 clock skew 估计值
5. **迁移确认**: 所有新接入必须使用 V2 端点格式和 Exchange V3 合约地址
