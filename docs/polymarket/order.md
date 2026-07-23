# Polymarket 订单接口

> 最后更新: 2026-07-23 | 适用版本: CLOB V2 (2026-04-28+)

---

## 1. 概述

Polymarket CLOB 支持四种订单类型，通过 EIP-712 签名后提交到链下匹配引擎。成交后由 CTF Exchange 合约在 Polygon 链上结算。

---

## 2. 订单类型

### 2.1 GTC (Good-Till-Cancelled)

| 属性 | 说明 |
|------|------|
| **行为** | 挂单持续有效，直到成交或主动取消 |
| **适用场景** | 限价单、做市策略 |
| **过期** | 不自动过期（无 `expiration` 限制） |
| **API 参数** | `orderType: "GTC"` |

**示例**:
```json
{
  "order": { "side": "BUY", "makerAmount": "43000000", "takerAmount": "100000000", "...": "..." },
  "signature": "0x...",
  "orderType": "GTC"
}
```

### 2.2 GTD (Good-Till-Date)

| 属性 | 说明 |
|------|------|
| **行为** | 挂单直到指定时间后自动过期 |
| **适用场景** | 限时策略（如新闻事件前） |
| **API 参数** | `orderType: "GTD"`，需指定 `expiration` |

### 2.3 FOK (Fill-Or-Kill)

| 属性 | 说明 |
|------|------|
| **行为** | 全部立即成交，否则完全取消（零部分成交） |
| **适用场景** | 必须全部执行的订单 |
| **失败响应** | `"order couldn't be fully filled. FOK orders are fully filled or killed."` |

### 2.4 FAK (Fill-And-Kill)

| 属性 | 说明 |
|------|------|
| **行为** | 尽可能立即成交，剩余部分取消 |
| **适用场景** | 实现"市价单"效果（取当前 book 的最优价格） |
| **失败响应** | `"no orders found to match with FAK order."` |

---

## 3. 订单数据结构

### 3.1 API 请求体 (POST /order)

```json
{
  "order": {
    "salt": 18446744073709551615,
    "maker": "0x1234567890abcdef1234567890abcdef12345678",
    "signer": "0x1234567890abcdef1234567890abcdef12345678",
    "taker": "0x0000000000000000000000000000000000000000",
    "tokenId": "12345",
    "makerAmount": "43000000",
    "takerAmount": "100000000",
    "expiration": "0",
    "nonce": "0",
    "feeRateBps": "0",
    "side": "BUY",
    "signatureType": 3
  },
  "signature": "0xabc123...",
  "orderType": "GTC"
}
```

### 3.2 字段说明

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `salt` | `uint256` | ✅ | 随机盐值（64-bit，每个订单唯一） |
| `maker` | `address` | ✅ | Maker 地址（资金方/Order Owner） |
| `signer` | `address` | ✅ | 签名地址（必须与 API Key 绑定的地址匹配） |
| `taker` | `address` | ✅ | Taker 地址。`0x0` 表示公开订单（任何人可成交） |
| `tokenId` | `uint256` | ✅ | ERC-1155 Token ID（Yes/No 各自不同） |
| `makerAmount` | `uint256` | ✅ | Maker 愿意出售的资产最大数量 |
| `takerAmount` | `uint256` | ✅ | Maker 期望获得的最小资产数量 |
| `expiration` | `uint256` | - | Unix 过期时间戳（0 = 不过期, GTC） |
| `nonce` | `uint256` | - | 链上取消用 nonce（V2 中可设为 0） |
| `feeRateBps` | `uint256` | - | Maker 费率（基点），V2 中可设为 0 |
| `side` | `string` | ✅ | `"BUY"` 或 `"SELL"` |
| `signatureType` | `uint8` | ✅ | 签名类型：0=EOA, 1=PROXY, 2=SAFE, 3=POLY_1271 |
| `signature` | `bytes` | ✅ | EIP-712 签名（独立字段，不在 order 内） |
| `orderType` | `string` | ✅ | `"GTC"` / `"GTD"` / `"FOK"` / `"FAK"` |

### 3.3 BUY vs SELL 金额计算

```
BUY (side="BUY"):
  makerAmount = 要支付的 pUSD 总额（价格 × 数量 × 10^6）
  takerAmount = 要收到的 token 数量（数量 × 10^6）

SELL (side="SELL"):
  makerAmount = 要出售的 token 数量（数量 × 10^6）
  takerAmount = 期望收到的 pUSD 总额（价格 × 数量 × 10^6）
```

### 3.4 响应

```json
{
  "orderID": "0xdef456...",
  "status": "accepted",
  "transactions": []
}
```

---

## 4. 订单查询

### 4.1 单订单查询

```
GET /order/{order_id}
```

**响应**:
```json
{
  "id": "0xdef456...",
  "market": "0xCONDITION_ID",
  "asset_id": "12345",
  "side": "BUY",
  "price": "0.43",
  "original_size": "100.0",
  "size_matched": "50.0",
  "outcome": "YES",
  "owner": "0x...",
  "status": "PARTIAL",
  "created_at": 1712345678
}
```

### 4.2 活跃订单列表

```
GET /orders
```

可选参数:
- `market` — 按 condition_id 过滤
- `asset_id` — 按 token_id 过滤
- `limit` — 返回数量上限

---

## 5. 订单取消

### 5.1 单个取消

```
DELETE /order
Body: { "orderID": "0xdef456..." }
```

**响应**: `{ "cancelled": ["0xdef456..."], "success": true }`

### 5.2 批量取消

```
DELETE /orders
Body: { "orderIDs": ["0xdef456...", "0xabc123..."] }
```

### 5.3 按市场取消

```
DELETE /cancel-market-orders
Body: { "market": "0xCONDITION_ID" }
```

### 5.4 全部取消

```
DELETE /cancel-all
Body: {}
```

---

## 6. 非标准市场参数

### 6.1 NegRisk 市场

NegRisk 是 Polymarket 的多结果互斥市场机制（如选举：候选人 A/B/C 互斥，总概率 ≤ 100%）。

| 属性 | 标准市场 | NegRisk 市场 |
|------|----------|-------------|
| 验证合约 | CTF Exchange | NegRisk Adapter |
| 订单签名 | 标准 domain | NegRisk domain |
| 检查方式 | `GET /neg-risk?token_id=...` 返回 `false` | 返回 `true` |

**在 Non-NegRisk 市场使用 NegRisk domain 签名，或反之，订单会被拒绝。**

### 6.2 Tick Size

Tick size 是价格的最小变动单位。必须从 `GET /tick-size` 获取并对齐价格。

**常见 tick size**: `0.01`, `0.001`, `0.0001`（价格接近 0 或 1 时动态调整）

---

## 7. 订单提交前验证清单

在 `POST /order` 之前，必须通过以下检查：

| # | 检查项 | 端点/来源 | 失败后果 |
|---|--------|----------|----------|
| 1 | Token ID 有效 | `GET /markets` | 404 Not Found |
| 2 | 市场活跃 | `GET /market` | 订单被拒 |
| 3 | Tick Size 对齐 | `GET /tick-size` | `breaks minimum tick size rule` |
| 4 | 数量 ≥ 最小值 | 市场配置 | `Size lower than the minimum` |
| 5 | pUSD 余额充足 | `GET /balances` | `not enough balance / allowance` |
| 6 | CTF Exchange 已授权 | `GET /balances` (allowance) | `not enough balance / allowance` |
| 7 | 非重复订单 | 本地 client_order_id 检查 | `Duplicated` |
| 8 | Post-only 不会成交 | 本地 book 对比 | `order crosses book` |
| 9 | 签名者与 API Key 匹配 | 本地地址对比 | `signer address has to be the address of the API KEY` |

---

## 8. 在当前项目中的接入位置

### 8.1 当前状态

```rust
// crates/execution/src/gateway.rs — MockGateway
pub struct MockGateway {
    fill_probability: f64,    // 70% 直接成交
    partial_fill_probability: f64,  // 30% 部分成交
    liquidity_fail_probability: f64,  // 4% 流动性不足
    slippage_base: f64,
    rng: Mutex<StdRng>,
}
```

MockGateway 模拟成交流程，无真实 API 调用。所有订单 `simulation_only = true`。

### 8.2 真实接入需要修改的模块

| 模块 | 文件 | 需要的改动 |
|------|------|-----------|
| ExecutionGateway trait | `crates/execution/src/gateway.rs` | 增加 `order_type`, `signature` 参数 |
| OrderBuilder | `crates/execution/src/builder.rs` | 增加精度转换（price → makerAmount/takerAmount） |
| ExecutionValidator | `crates/execution/src/validator.rs` | 新增 `TickSizeRule`, `BalanceRule` |
| Order 模型 | `crates/execution/src/order.rs` | 增加 `signature`, `signature_type`, `salt`, `order_type` 字段 |
| CLOB API Client | 新建 `crates/execution/src/clob_client.rs` | `POST /order`, `DELETE /order`, `GET /order` |
| EIP-712 签名 | 新建 `crates/execution/src/signing.rs` | 订单签名逻辑 |

---

## 9. 订单错误码速查

| 错误信息 | HTTP | 原因 | 解决方法 |
|----------|------|------|----------|
| `Invalid order payload` | 400 | JSON 格式错误或缺少字段 | 检查请求结构 |
| `breaks minimum tick size rule` | 400 | 价格不对齐 tick size | 对齐价格到 tick_size |
| `Size lower than the minimum` | 400 | 数量低于最小值 | 增加数量 |
| `invalid signature` | 400 | EIP-712 签名无效 | 检查签名逻辑 |
| `not enough balance / allowance` | 400 | pUSD 不足或未授权 | 充值或 approve |
| `maker address not allowed` | 400 | EOA 直接交易被禁用 | 使用 deposit wallet |
| `signer address has to be the address of the API KEY` | 400 | 签名地址 ≠ API Key 地址 | 使用正确地址签名 |
| `order_version_mismatch` | 400 | SDK domain 版本不对 | 更新 SDK 到 V2 |
| `post-only order: order crosses book` | 400 | Post-only 会立即成交 | 调整价格 |
| `FOK orders are fully filled or killed` | 400 | 无法全部成交 | 减小数量或使用 FAK |
| `market is not yet ready` | 400 | 市场尚未开放 | 等待市场就绪 |
| `order timed out` | 400 | 服务器处理超时 | 重试提交 |
| `Unauthorized` | 401 | L2 认证失败 | 检查 API Key/HMAC |
| `Trading is currently cancel-only` | 503 | 交易所暂停交易 | 等待恢复 |

---

## 10. 未来扩展建议

1. **订单模板**: 预定义常见订单类型（价差单、冰山单等），简化策略层的订单构造
2. **智能路由**: 根据市场深度自动选择 GTC/FOK/FAK
3. **TWAP 执行**: 将大单拆分为多个时间分布的 GTC 单
4. **订单回执**: 将 CLOB 返回的 orderID 与本地 `client_order_id` 关联存储
5. **自动续期**: 对 GTD 订单在过期前自动续期
