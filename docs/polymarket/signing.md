# Polymarket EIP-712 订单签名

> 最后更新: 2026-07-23 | 适用版本: CLOB V2 (2026-04-28+) | Exchange V3

---

## 1. 概述

Polymarket 的所有订单必须使用 **EIP-712 类型化数据签名**。签名的订单与请求级 HMAC 认证（L2）是**两个独立的签名层**：
- **L2 HMAC** 认证 API 请求的合法性
- **EIP-712 签名**证明订单本身由钱包所有者授权

---

## 2. EIP-712 Domain Separator

Domain separator 将签名绑定到特定的链和合约，防止跨链/跨合约重放。

### 2.1 Domain 定义（V2 / Exchange V3）

```
{
  name: "Polymarket CTF Exchange",
  version: "2",
  chainId: 137,
  verifyingContract: "0xE111180000d2663C0091e4f400237545B87B996B"
}
```

### 2.2 版本对照

| 版本 | Exchange 合约 | 说明 |
|------|--------------|------|
| `"1"` | `0x4bfb...982e` (V1) | 旧版，**已废弃** |
| `"2"` | `0xE111...996B` (V2/V3) | **当前版本**，CLOB V2 使用 |

### 2.3 NegRisk 合约

对于 NegRisk 市场（多结果互斥，如选举），验证合约不同：

| 版本 | NegRisk 合约 |
|------|-------------|
| V2/V3 | `0xe222...0F59` |

---

## 3. 订单结构体 (Order Struct)

### 3.1 V2 订单字段（11-field signed struct）

CLOB V2 的 EIP-712 签名负载包含 11 个字段：

| # | 字段 | 类型 | 说明 |
|---|------|------|------|
| 1 | **salt** | `uint256` | 随机盐值（63-64 bit 随机数），每个订单唯一 |
| 2 | **maker** | `address` | 订单创建者（资金方）地址 |
| 3 | **signer** | `address` | 签名者地址（必须与 API Key 绑定的地址匹配） |
| 4 | **tokenId** | `uint256` | ERC-1155 代币 ID（YES/NO 对应不同 tokenId） |
| 5 | **makerAmount** | `uint256` | Maker 出售的最大资产数量（精度: 10⁶） |
| 6 | **takerAmount** | `uint256` | Maker 期望收到的最小资产数量（精度: 10⁶） |
| 7 | **side** | `uint8` | 0 = BUY, 1 = SELL |
| 8 | **signatureType** | `uint8` | 签名类型 (0-3，见下文) |
| 9 | **metadata** | `bytes` | 元数据（V2 新增，包含 builder 归属码等） |
| 10 | **builder** | `address` | Builder 归属地址（V2 新增） |
| 11 | **timestamp** | `uint256` | 订单创建时间戳（V2 新增，替代 nonce） |

### 3.2 V2 变更要点

相比 V1，V2 的主要变更：

| 变更 | V1 | V2 |
|------|----|----|
| 签名字段数 | 12 | 11 |
| taker | 包含在签名中 | 移出签名（服务端指定） |
| nonce | 包含在签名中 | 移除，改用 timestamp 防重放 |
| expiration | 包含在签名中 | 移出签名（仅 API 层传输） |
| feeRateBps | 包含在签名中 | 移除（协议动态确定） |
| metadata | 无 | 新增 |
| builder | 无 | 新增 |
| timestamp | 无 | 新增 |

### 3.3 BUY vs SELL 的金额含义

**BUY (side=0)**:
- `makerAmount` = 愿意支付的最大 pUSD 总额
- `takerAmount` = 期望获得的最小 YES/NO token 数量

**SELL (side=1)**:
- `makerAmount` = 愿意出售的最大 YES/NO token 数量
- `takerAmount` = 期望获得的最小 pUSD 总额

所有金额以最小单位整数表示（精度 10⁶，即 1 USDC = 1,000,000）。

---

## 4. 签名类型详解

| 值 | 类型名 | 签名字段验证方式 | 适用场景 |
|----|--------|-----------------|----------|
| **0** | EOA | ECDSA `ecrecover` | 直接使用 EOA 私钥签名（**CLOB V2 已禁用**） |
| **1** | POLY_PROXY | ERC-1271 `isValidSignature` | Proxy 智能合约钱包 |
| **2** | POLY_GNOSIS_SAFE | ERC-1271 `isValidSignature` | Gnosis Safe 多签钱包 |
| **3** | POLY_1271 | ERC-1271 `isValidSignature` | Deposit Wallet（推荐新用户使用） |

> **2026 年新用户**: 必须使用 signatureType=3 (POLY_1271)，通过 Polymarket 前端创建 Deposit Wallet。

---

## 5. 签名流程

### 5.1 完整签名过程

```
1. 准备订单参数（tokenId, price, size, side）
      │
2. 从 CLOB 获取当前 nonce → GET /nonce?user=<address>
      │
3. 生成随机 salt（64-bit）
      │
4. 构造 EIP-712 typed data payload：
   ├── domain: {name, version, chainId, verifyingContract}
   └── message: Order struct (11 fields)
      │
5. 用钱包私钥对 typed data 签名
   ├── signatureType=0: 标准 ECDSA EIP-712 签名
   ├── signatureType=1/2/3: 通过 ERC-1271 验证
   │
6. 组装完整请求：
   └── { order: {...}, signature: "0x...", orderType: "GTC" }
      │
7. 设置 L2 HMAC 认证头
      │
8. POST /order → 提交签名订单
```

### 5.2 Rust 签名实现参考

使用 `ethers-rs` 或 `alloy` 进行 EIP-712 签名：

```rust
use ethers::types::transaction::eip712::{Eip712, TypedData};

// 1. 定义订单类型
#[derive(Eip712, Clone)]
#[eip712(
    name = "Polymarket CTF Exchange",
    version = "2",
    chain_id = 137,
    verifying_contract = "0xE111180000d2663C0091e4f400237545B87B996B"
)]
struct Order {
    salt: U256,
    maker: Address,
    signer: Address,
    token_id: U256,
    maker_amount: U256,
    taker_amount: U256,
    side: u8,
    signature_type: u8,
    metadata: Bytes,
    builder: Address,
    timestamp: U256,
}

// 2. 签名
let order = Order { /* ... */ };
let signature = wallet.sign_typed_data(&order).await?;
```

> **注意**: Rust 生态中的 Polymarket SDK 参考：
> - `rs-clob-client-v2` (GitHub: `tdergouzi/rs-clob-client-v2`) — 社区 Rust SDK
> - `polymarket-hft` (docs.rs) — 高频交易 Rust 客户端
> - `polymarket-sdk` (docs.rs) — 通用 Rust SDK

---

## 6. 价格与数量的精度转换

### 6.1 价格 → makerAmount/takerAmount

以 BUY 订单为例，价格 0.43，买 100 个 token：

```
raw_price = 0.43
raw_size  = 100.0
decimals  = 10^6 = 1,000,000

makerAmount (pUSD cost) = round(raw_price * raw_size * decimals)
                         = round(0.43 * 100.0 * 1,000,000)
                         = 43,000,000

takerAmount (token qty)  = round(raw_size * decimals)
                         = 100,000,000
```

### 6.2 Tick Size 对齐

价格必须对齐到市场的 tick_size：

```
tick_size = GET /tick-size → "0.01" → 0.01
aligned_price = round(raw_price / tick_size) * tick_size
```

如果价格不对齐，服务器返回：
`"order {id} is invalid. Price ({price}) breaks minimum tick size rule: {tick_size}"`

---

## 7. Salt 生成

```rust
use rand::Rng;

fn generate_salt() -> u64 {
    rand::thread_rng().gen::<u64>()
}
```

Salt 仅需保证单用户范围内唯一，64-bit 随机数足够。

---

## 8. 预交易检查清单

签名前必须确认：

| 检查项 | 如何确认 |
|--------|----------|
| ✅ pUSD 余额充足 | `GET /balances` |
| ✅ CTF Exchange 已授权 pUSD | `GET /balances` 中检查 `allowance` |
| ✅ Conditional Tokens 合约已 approve | `setApprovalForAll(CTF_EXCHANGE, true)` |
| ✅ 价格符合 tick size | `GET /tick-size` |
| ✅ 市场非 NegRisk（或使用正确的合约地址） | `GET /neg-risk` |
| ✅ 服务器时间已同步 | `GET /time` |
| ✅ 订单 nonce 已获取 | `GET /nonce`（V1）或使用 timestamp（V2） |

---

## 9. 在当前项目中的接入位置

| 组件 | 路径 | 需要实现 |
|------|------|----------|
| 订单签名 | `crates/execution/src/gateway.rs` 或新建 `crates/execution/src/signing.rs` | EIP-712 typed data 构造与签名 |
| Tick Size 验证 | `crates/execution/src/validator.rs` | 新增 `TickSizeRule` |
| 精度转换 | `crates/execution/src/builder.rs` | `OrderBuilder` 中加入精度换算 |
| Nonce 管理 | `crates/execution/` 或 `crates/trading/` | 从 CLOB 获取并维护 nonce |
| Salt 生成 | `crates/execution/src/builder.rs` | 订单构造时自动生成 |

---

## 10. 未来扩展建议

1. **硬件签名支持**: 考虑支持 AWS KMS / HashiCorp Vault 签名（signatureType=3 通过 ERC-1271）
2. **Gasless 中继**: 对于 Deposit Wallet 用户，使用 Polymarket Relayer 进行免 Gas 交易
3. **签名缓存**: 相同参数的重试订单复用签名（需注意 salt 和 timestamp 更新）
4. **签名测试模式**: 在 Mock Gateway 中模拟 EIP-712 验证，确保签名逻辑正确
5. **Order Hash 预计算**: 在提交前计算预期 order hash，用于后续状态跟踪
