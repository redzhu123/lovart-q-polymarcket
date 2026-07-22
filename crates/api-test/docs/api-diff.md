# Polymarket API 差异记录

> 自动生成于 2026-07-23 | 由 pm-api-test 维护

## 概述

本文档记录 Polymarket 官方文档与实际 API 响应之间的差异。
当发现不一致时，遵循以下原则：

1. **不修改业务代码**来适配实际响应
2. **记录差异**到此文件
3. **保存真实响应**到 `fixtures/` 目录
4. **给出修改建议**，但不自动修复业务逻辑

---

## 已确认的差异

### 1. CLOB API 端点路径差异

| 项目 | 预期（Gateway 代码） | 实际（CLOB 文档） | 差异 |
|------|---------------------|-------------------|------|
| 余额端点 | `GET /balance` | `GET /balances` | 网关使用单数 `/balance`，实际 API 使用复数 `/balances` |
| 认证方式 | `Authorization: Bearer <key>` | L2 HMAC-SHA256 | 网关使用简单 Bearer Token，实际需要 HMAC 签名 |
| 订单请求体 | 简单 JSON `{token_id, price, size, side, type}` | EIP-712 签名订单 | 网关不包含完整的 EIP-712 签名结构 |

**建议**: 更新 `crates/gateway/src/polymarket/rest.rs` 中的端点路径和认证方式。

### 2. Gamma API 字段格式

| 字段 | Gamma 实际返回 | 影响 |
|------|---------------|------|
| `outcomePrices` | JSON 字符串 `"[\"0.43\",\"0.57\"]"` | 需要 `serde_json::from_str` 二次解析 |
| `outcomes` | JSON 字符串 `"[\"Yes\",\"No\"]"` | 同上 |
| `clobTokenIds` | JSON 字符串 `"[\"id1\",\"id2\"]"` | 同上 |

**建议**: 当前 Gateway 未处理这些字段。`ClobProvider` 已正确处理。

### 3. CLOB V2 变更 (2026-04-28)

| 变更项 | V1 | V2 |
|--------|----|----|
| 抵押品 | USDC.e | pUSD |
| EIP-712 domain version | "1" | "2" |
| Exchange 合约 | V2 | V3 |
| EOA 签名 (signatureType=0) | 支持 | **已禁用** |
| deposit wallet (signatureType=3) | 不支持 | **新用户必须使用** |
| 费用 | 订单内嵌 | 协议动态确定 (taker-only) |

**建议**: 所有新接入必须使用 V2 格式。Gateway 代码需更新签名类型和合约地址。

---

## 待验证的差异

以下端点尚未通过 Live 测试验证（需要 API Key）：

- [ ] `GET /balances` - 余额响应格式
- [ ] `GET /orders` - 订单列表响应格式
- [ ] `GET /trades` - 成交记录响应格式
- [ ] `POST /order` - 下单请求/响应格式（需要 enable_live=true）
- [ ] `DELETE /order` - 撤单响应格式（需要 enable_live=true）
- [ ] `GET /positions` - 持仓响应格式（Data API）

---

## Schema 校验状态

| Schema | Mock 测试 | Live 测试 | 状态 |
|--------|----------|-----------|------|
| `server-time.schema.json` | ✅ | 待验证 | 就绪 |
| `markets.schema.json` | ✅ | 待验证 | 就绪 |
| `market-detail.schema.json` | ✅ | 待验证 | 就绪 |
| `orderbook.schema.json` | ✅ | 待验证 | 就绪 |
| `trades.schema.json` | ✅ | 待验证 | 就绪 |
| `balance.schema.json` | ✅ | 待验证（需 API Key） | 就绪 |
| `orders.schema.json` | ✅ | 待验证（需 API Key） | 就绪 |
| `positions.schema.json` | ✅ | 待验证（需 API Key） | 就绪 |
| `error.schema.json` | ✅ | 待验证 | 就绪 |

---

## 修改建议

### 高优先级

1. **认证机制**: 将 `PolymarketRestClient` 的 `Authorization: Bearer` 替换为 L2 HMAC-SHA256 签名
2. **端点路径**: 将 `/balance` 改为 `/balances`
3. **订单签名**: 实现完整的 EIP-712 订单签名流程（参考 `docs/polymarket/signing.md`）
4. **签名类型**: 使用 `signatureType=3` (POLY_1271)，放弃 `signatureType=0` (EOA)

### 中优先级

5. **批量端点**: 优先使用 `/books`, `/prices`, `/orders` 批量端点减少请求数
6. **Tick Size**: 缓存 `GET /tick-size` 返回值
7. **时间同步**: 后台定时调用 `GET /time` 维护 clock skew

### 低优先级

8. 支持 WebSocket Market Channel 实时数据推送
9. 实现价格历史 `/prices-history` 用于波动率模型

---

*本文档将在每次 Live 测试运行后自动更新。*
