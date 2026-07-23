# Polymarket Balance / Position 接口

> 最后更新: 2026-07-23 | 覆盖: CLOB API + Data API + Polygon RPC

---

## 1. 概述

Polymarket 的余额和持仓信息分布在三个层面：

| 层面 | 数据源 | 类型 | 内容 |
|------|--------|------|------|
| **CLOB** | `GET /balances` | 链下+链上 | pUSD 余额、交易所授权额度 |
| **Data API** | `/positions`, `/trades`, `/value` | 链上索引 | 详细持仓、成交历史、总资产 |
| **Polygon RPC** | `balanceOf()`, CTF events | 链上原生 | 最权威但最原始的数据 |

---

## 2. CLOB API: 余额与授权

### 2.1 GET /balances

**Base URL**: `https://clob.polymarket.com`

**用途**: 查询用户的 pUSD 余额和 CTF Exchange 授权额度。

**认证**: L2 HMAC 认证

**请求**:
```
GET /balances
Headers:
  POLY_ADDRESS: 0x...
  POLY_API_KEY: ...
  POLY_PASSPHRASE: ...
  POLY_SIGNATURE: ...
  POLY_TIMESTAMP: ...
```

**响应**:
```json
{
  "balance": "10000000000",
  "allowance": "10000000000"
}
```

| 字段 | 说明 |
|------|------|
| `balance` | pUSD 余额（最小单位，精度 10⁶） |
| `allowance` | CTF Exchange 的 pUSD 授权额度 |

**限制**: 通用限制

**关键检查**:
- 下单前必须 `balance >= makerAmount`
- 首次交易前必须调用 `approve(CTF_EXCHANGE, MAX_UINT256)` 设置 allowance
- 如果 `allowance < makerAmount`，订单会被拒绝：`"not enough balance / allowance"`

---

## 3. Data API: 持仓与历史

### 3.1 概述

**Base URL**: `https://data-api.polymarket.com`

**认证**: 无需认证（公开 API）

Data API 索引链上数据，提供结构化的持仓、成交、活动记录。

### 3.2 GET /positions — 当前持仓

**用途**: 获取用户当前持有的头寸列表。

**请求**:
```
GET /positions?user=0xWALLET_ADDRESS
```

**参数**:

| 参数 | 必需 | 说明 |
|------|------|------|
| `user` | ✅ | 钱包地址 (0x + 40 hex) |
| `tokenId` | ❌ | 按 token ID 过滤 |
| `market` | ❌ | 按 condition_id 过滤（与 eventId 互斥） |
| `eventId` | ❌ | 按事件 ID 过滤（与 market 互斥） |
| `sizeThreshold` | ❌ | 最小持仓数量阈值 |
| `redeemable` | ❌ | 仅显示可赎回的持仓 |
| `mergeable` | ❌ | 仅显示可合并的持仓 |
| `limit` | ❌ | 0-500，默认 100 |
| `offset` | ❌ | 分页偏移 |
| `sortBy` | ❌ | `CURRENT`, `INITIAL`, `TOKENS`, `CASHPNL`, `PERCENTPNL`, `TITLE`, `RESOLVING`, `PRICE`, `AVGPRICE` |
| `sortDirection` | ❌ | `ASC` / `DESC` |
| `title` | ❌ | 按市场标题模糊搜索 |

**响应**:
```json
[
  {
    "positionId": "0x...",
    "tokenId": "12345",
    "conditionId": "0xCONDITION_ID",
    "title": "Will BTC exceed $100K in 2025?",
    "outcome": "Yes",
    "size": "100000000",
    "avgPrice": "0.43",
    "initialValue": "43000000",
    "currentValue": "45000000",
    "cashPnl": "2000000",
    "percentPnl": "4.65",
    "curPrice": "0.45",
    "redeemable": false,
    "mergeable": false
  }
]
```

| 字段 | 说明 |
|------|------|
| `positionId` | 持仓唯一 ID |
| `tokenId` | ERC-1155 token ID |
| `conditionId` | 市场 condition ID |
| `size` | 持仓数量（最小单位） |
| `avgPrice` | 平均成交价格 |
| `initialValue` | 建仓价值（pUSD，最小单位） |
| `currentValue` | 当前市值（pUSD，最小单位） |
| `cashPnl` | 已实现+未实现盈亏 |
| `curPrice` | 当前价格 |

**在当前项目中的接入位置**: `crates/portfolio/` 或 `crates/trading/src/provider.rs` → `TradingProvider::account()` 扩展

### 3.3 GET /trades — 成交历史

**用途**: 获取用户的成交记录。

**请求**:
```
GET /trades?user=0xWALLET_ADDRESS
```

**参数**:

| 参数 | 说明 |
|------|------|
| `user` | 钱包地址 |
| `tokenId` | 按 token 过滤 |
| `market` | 按 condition_id 过滤 |
| `startTime` | 开始时间 |
| `endTime` | 结束时间 |
| `limit` | 返回数量 |
| `offset` | 分页偏移 |

**响应**: 成交列表，每条记录包含价格、数量、方向、时间戳等。

**在当前项目中的接入位置**: `crates/execution/src/replay.rs` — 用于回测和成交验证

### 3.4 GET /activity — 活动记录

**用途**: 获取用户的全部链上操作记录（审计追踪）。

**请求**:
```
GET /activity?user=0xWALLET_ADDRESS
```

**活动类型**:

| eventType | 说明 |
|-----------|------|
| `order_created` | 订单创建 |
| `order_cancelled` | 订单取消 |
| `order_matched` | 订单匹配 |
| `trade` | 成交 |
| `deposit` | 充值 |
| `withdrawal` | 提现 |
| `split` | 拆分 tokens |
| `merge` | 合并 tokens |
| `redeem` | 赎回 |
| `reward` | 奖励发放 |

**响应字段**: `id`, `address`, `eventType`, `tokenId`, `orderId`, `details`, `timestamp`

**在当前项目中的接入位置**: `crates/execution/src/replay.rs` — 完整审计追踪

### 3.5 GET /value — 总资产价值

**用途**: 获取钱包的总 USDC/pUSD 暴露和 PnL。

**请求**:
```
GET /value?user=0xWALLET_ADDRESS
```

**响应**: 聚合的总资产价值、总盈亏。

### 3.6 GET /closed-positions — 已平仓

**用途**: 获取已完全卖出或赎回的历史持仓。

### 3.7 GET /holders — Token 持有人

**用途**: 获取指定市场的 Token 持有者排名（含个人资料）。

### 3.8 GET /traded — 交易过的市场数

**用途**: 获取钱包在多少个独特市场有过交易。

---

## 4. Polygon RPC: 链上余额

### 4.1 pUSD 余额 (ERC-20)

**合约地址**: 待确认（CLOB V2 新部署的 pUSD 合约）

**方法**: `balanceOf(address) → uint256`

**Rust 调用**:
```rust
use ethers::contract::abigen;

abigen!(PUSD, "./abi/pusd.json");
let pusd = PUSD::new(pusd_address, provider);
let balance = pusd.balance_of(user_address).call().await?;
```

### 4.2 Conditional Tokens 持仓 (ERC-1155)

**合约地址**: `0x4D97DCd97eC945f40cF65F87097ACe5EA0476045` (CTF)

**方法**: `balanceOf(address, tokenId) → uint256`

### 4.3 授权检查

**CTF Exchange 合约**: `0xE111180000d2663C0091e4f400237545B87B996B`

**方法**: `pUSD.allowance(user, ctf_exchange) → uint256`

---

## 5. 当前项目中的接入

### 5.1 已有接入

| 组件 | 说明 |
|------|------|
| `crates/portfolio/` | 模拟的 10,000 USDC 初始资金，固定 100/仓位 |
| `MockTradingProvider::account()` | 返回 Mock 的 `AccountSummary` |
| `ExecutionValidator::CashRule` | 检查 `available_cash >= order.notional()` |

### 5.2 需要新增

| 功能 | 建议位置 | 数据源 |
|------|----------|--------|
| 真实余额查询 | `crates/trading/src/provider.rs` | `GET /balances` + Data API `/value` |
| 持仓同步 | 新建 `crates/execution/src/portfolio_sync.rs` 扩展 | Data API `/positions` + User WS `trade` events |
| 授权检查 | `crates/execution/src/validator.rs` 新增 `AllowanceRule` | `GET /balances` (allowance 字段) |
| 成交验证 | `crates/execution/src/replay.rs` | Data API `/trades` |
| 链上余额 | `crates/trading/` 或新建 `crates/onchain/` | Polygon RPC `balanceOf()` |

---

## 6. 数据一致性

### 6.1 数据时效性

| 数据 | 延迟 | 权威性 |
|------|------|--------|
| CLOB `GET /balances` | ~100ms | 高（CLOB 内部状态） |
| Data API `/positions` | 数秒 | 中（链上索引延迟） |
| Polygon RPC `balanceOf()` | 区块确认后 (~3s) | **最高**（链上原生） |
| Data API `/trades` | 数秒 | 中 |
| Data API `/activity` | 数秒 | 中（最完整） |

### 6.2 推荐验证流程

```
1. 内部记账: Portfolio 模块维护本地 position 记录
2. 实时更新: User WS 的 trade/order 事件 → 更新本地状态
3. 定时对账: 每 N 分钟调用 Data API /positions 与本地状态对比
4. 上链确认: 关键操作后调用 Polygon RPC 做最终确认
5. 差异修复: 发现不一致时，以链上数据为准修正本地状态
```

---

## 7. 未来扩展建议

1. **仓位快照**: 定时保存 `/positions` 结果到 CSV，支持离线分析
2. **PnL 分解**: 将 `cashPnl` 分解为已实现/未实现，支持税务报告
3. **多钱包聚合**: 支持同时追踪多个交易钱包的总仓位
4. **风险计算**: 基于真实持仓计算 VAR、暴露度等风控指标
5. **Gas 余额监控**: 监控 Polygon MATIC 余额，确保有足够的 Gas 进行链上操作
