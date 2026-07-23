# Polymarket 认证体系 (Authentication)

> 最后更新: 2026-07-23 | 适用版本: CLOB V2 (2026-04-28+)

---

## 1. 概述

Polymarket CLOB API 使用**双层认证**体系：

| 层级 | 用途 | 签名方式 | 触发频率 |
|------|------|----------|----------|
| **L1** | 一次性创建/派生 API 凭证 | EIP-712 钱包签名 | 仅首次（或凭证丢失时） |
| **L2** | 所有交易请求的请求级认证 | HMAC-SHA256 | 每次请求 |

此外，**订单本身**也需要 EIP-712 签名（使用钱包私钥），这与 L2 认证是独立的。

> **⚠ 重要提醒 (2026)**: CLOB V2 升级后，EOA 直接签名 (signatureType=0) 已被禁用。新用户必须使用 deposit wallet 流程 (signatureType=3, POLY_1271)。已有 Proxy (type=1) 或 Safe (type=2) 用户可继续使用。详见 [signing.md](signing.md)。

---

## 2. L1 认证：EIP-712 凭证引导

### 2.1 端点

```
POST https://clob.polymarket.com/auth/api-key
GET  https://clob.polymarket.com/auth/derive-api-key
```

`POST /auth/api-key` 创建新的 API Key（每次调用生成不同的 key）。  
`GET /auth/derive-api-key` 从相同的 EIP-712 签名**确定性派生** API Key（同一签名始终返回相同凭证）。

### 2.2 请求头

| Header | 说明 |
|--------|------|
| `POLY_ADDRESS` | 钱包地址 (0x + 40 hex) |
| `POLY_SIGNATURE` | `ClobAuth` 类型化数据的 EIP-712 签名 |
| `POLY_TIMESTAMP` | Unix 时间戳 (秒) |
| `POLY_NONCE` | 随机 nonce（防重放） |

### 2.3 EIP-712 签名负载 (ClobAuth)

签名的类型化数据包含以下字段：

```
domain:
  name: "ClobAuth"
  version: "1"
  chainId: 137 (Polygon mainnet)

message:
  address: 用户钱包地址
  timestamp: 当前 Unix 时间戳
  nonce: 0
  message: "This message is to generate a Polymarket API key"
```

> **注意**: `ClobAuth` 的 EIP-712 domain 和订单签名的 domain 是**不同**的。前者用于认证，后者用于交易。

### 2.4 响应

```json
{
  "apiKey": "hex-string",
  "secret": "base64url-encoded-string",
  "passphrase": "hex-string"
}
```

### 2.5 凭证存储

- `apiKey`: 明文存储（公开标识符）
- `secret`: **必须安全存储**。使用前需从 URL-safe Base64 解码为原始字节。
- `passphrase`: **必须安全存储**。明文使用。

在当前项目中，凭证应存储在 `provider.toml` 的 `[polymarket.credential]` 段，或通过环境变量 `POLY_API_KEY` / `POLY_SECRET` / `POLY_PASSPHRASE` 注入。

### 2.6 限制

- API Key 绑定到创建时的钱包地址
- 已知问题 (2026-06): `/auth/api-key` 仅支持 ECDSA 签名恢复（不支持 ERC-1271），导致 deposit wallet 用户的 API Key 始终绑定到 EOA 地址。变通方案：使用 MetaMask 创建账户。

---

## 3. L2 认证：HMAC-SHA256 请求签名

### 3.1 适用端点

所有 CLOB API 的**交易相关端点**需要 L2 认证：
- `POST /order` / `DELETE /order` / `DELETE /orders`
- `DELETE /cancel-market-orders` / `DELETE /cancel-all`
- `GET /orders` / `GET /trades`
- `GET /balances` / `GET /api-keys`
- `POST /create-api-key` / `DELETE /delete-api-key`
- `GET /notifications` 系列
- `GET /rewards/*` 系列

公开端点（`/book`, `/price`, `/markets` 等）**不需要** L2 认证。

### 3.2 请求头

| Header | 说明 |
|--------|------|
| `POLY_ADDRESS` | 钱包地址 |
| `POLY_API_KEY` | L1 获取的 apiKey |
| `POLY_PASSPHRASE` | L1 获取的 passphrase |
| `POLY_SIGNATURE` | HMAC-SHA256 签名 |
| `POLY_TIMESTAMP` | Unix 时间戳（秒） |

### 3.3 HMAC 签名构造

```
签名输入 = POLY_TIMESTAMP + HTTP_METHOD + REQUEST_PATH + BODY

其中:
  POLY_TIMESTAMP  = 当前 Unix 时间戳（字符串）
  HTTP_METHOD     = "GET" | "POST" | "DELETE"
  REQUEST_PATH    = URL 路径部分，如 "/order" 或 "/orders?market=0x..."
  BODY            = 请求体（GET/DELETE 时为空字符串 ""；POST 时为 JSON 字符串）
```

**签名算法**:
```
HMAC-SHA256(secret_bytes, signature_input)
输出: hex 编码的哈希值
```

**Rust 参考实现**:
```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn sign_l2(secret_base64url: &str, timestamp: u64, method: &str, path: &str, body: &str) -> String {
    // 1. 解码 secret
    let secret_bytes = decode_base64url(secret_base64url);
    // 2. 构造签名输入
    let prehash = format!("{}{}{}{}", timestamp, method, path, body);
    // 3. HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(&secret_bytes).unwrap();
    mac.update(prehash.as_bytes());
    // 4. 输出 hex
    hex::encode(mac.finalize().into_bytes())
}
```

### 3.4 时间戳有效期

`POLY_TIMESTAMP` 必须在服务器时间的 **±30 秒**范围内，否则返回 401。
务必先调用 `GET /time` 同步服务器时间，或使用 NTP 校准系统时钟。

### 3.5 在当前项目中的接入位置

| 组件 | 文件 | 接入方式 |
|------|------|----------|
| 交易凭证存储 | `provider.toml` → `[polymarket.credential]` | 明文配置或环境变量 |
| 凭证管理器 | `crates/trading/src/credential.rs` → `CredentialManager` | 加载/验证/刷新凭证 |
| L2 签名 | `crates/trading/src/session.rs` 或新增 `auth.rs` | 每次交易请求前构造 HMAC |
| API Key 创建 | 一次性手动操作（或 CLI 工具） | 不嵌入自动化流程 |

---

## 4. 认证流程总结

```
┌────────────────────────────────────────────────────────────┐
│                     一次性引导（仅一次）                      │
│                                                             │
│  1. 用钱包私钥签名 EIP-712 ClobAuth 消息                     │
│  2. POST /auth/api-key 获取 {apiKey, secret, passphrase}    │
│  3. 安全存储凭证 ←─────────────────────────────┐             │
│                                                 │             │
├─────────────────────────────────────────────────┤             │
│                  日常交易（每次请求）             │             │
│                                                 │             │
│  4. 加载凭证 ─────────────────────────────────┘             │
│  5. 构造 HMAC-SHA256(timestamp+method+path+body)             │
│  6. 设置 L2 请求头 (POLY_ADDRESS, POLY_API_KEY,              │
│     POLY_PASSPHRASE, POLY_SIGNATURE, POLY_TIMESTAMP)         │
│  7. 发送请求                                                 │
│                                                             │
│  注意: 订单本身还需要独立的 EIP-712 签名（见 signing.md）     │
└────────────────────────────────────────────────────────────┘
```

---

## 5. 安全注意事项

1. **Secret / Passphrase 绝不能**提交到 Git。使用 `.gitignore` 排除 `provider.toml` 或使用环境变量。
2. **私钥管理**：建议使用专用交易钱包，与主资金钱包分离。
3. **Timestamp 同步**：每次会话启动时调用 `GET /time` 校准时间偏移。
4. **HTTPS 强制**：所有 CLOB 通信必须使用 HTTPS。
5. **敏感数据遮蔽**：日志中通过 `crates/trading/src/mask.rs` 遮蔽 API Key 和地址。

---

## 6. 未来扩展建议

- `CredentialManager` 应支持从**加密文件**加载凭证（如 age-encrypted toml）
- 增加 `GET /auth/derive-api-key` 支持，确定性派生避免存储多个凭证
- 实现 L2 签名的**自动重试**机制（timestamp 过期时自动刷新）
- 考虑增加 **session token** 缓存以减少 HMAC 计算频率
