# Gateway 错误处理指南

> P2-03 Exchange Gateway Implementation | 更新: 2026-07-23

本文档描述 Gateway 的统一错误类型系统、如何处理每个错误、以及最佳实践。

## 1. 错误类型层次

```
GatewayError (enum)
├── NetworkError           网络错误（连接失败、DNS、超时）
├── AuthenticationError     认证失败（API 密钥无效、签名错误）
├── RateLimitError          速率限制（429 或本地限流）
├── ValidationError         参数校验失败
├── ExchangeError           交易所业务错误（余额不足、市场已关闭）
├── TimeoutError            请求超时
└── SerializationError      序列化/反序列化失败
```

## 2. 错误结构

每个错误包含：

| 字段 | 类型 | 说明 |
|------|------|------|
| `code` | `&'static str` | 错误码（如 `GW_NET_001`） |
| `message` | `String` | 中文错误消息 |
| `suggestion` | `String` | 中文建议处理方式 |
| `kind_zh` | `&'static str` | 错误类型中文名 |
| `is_retryable` | `bool` | 是否可重试 |

## 3. 错误码

| 错误码 | 类型 | 中文名 | 是否可重试 |
|--------|------|--------|------------|
| `GW_NET_001` | 网络错误 | 网络错误 | ✅ 是 |
| `GW_AUTH_001` | 认证失败 | 认证失败 | ❌ 否 |
| `GW_RATE_001` | 速率限制 | 速率限制 | ✅ 是 |
| `GW_VAL_001` | 参数校验失败 | 参数校验失败 | ❌ 否 |
| `GW_EXCH_001` | 交易所错误 | 交易所错误 | ❌ 否 |
| `GW_TO_001` | 请求超时 | 请求超时 | ✅ 是 |
| `GW_SER_001` | 序列化错误 | 序列化错误 | ❌ 否 |

## 4. 处理建议

### 4.1 网络错误（GW_NET_001）

**何时抛出**：HTTP 连接失败、DNS 解析失败、TLS 握手失败。

**建议处理**：
- ✅ 自动重试（已由中间件处理）
- ✅ 检查网络连接
- ✅ 检查 API URL 是否正确
- ❌ 不要立即降级为离线模式

### 4.2 认证失败（GW_AUTH_001）

**何时抛出**：API 密钥缺失、无效、签名错误。

**建议处理**：
- ❌ 不要重试（认证错误不会自动恢复）
- ✅ 检查环境变量 `POLYMARKET_API_KEY`
- ✅ 检查 API 密钥是否过期
- ✅ 检查钱包签名是否正确（HMAC / EIP-712）

### 4.3 速率限制（GW_RATE_001）

**何时抛出**：收到 HTTP 429、本地 Token Bucket 耗尽。

**建议处理**：
- ✅ 自动等待 `retry_after_ms` 后重试
- ✅ 检查是否触发了 Cloudflare 滑动窗口
- ✅ 启用本地速率限制（已在中间件中实现）
- ❌ 不要立即重试

### 4.4 参数校验失败（GW_VAL_001）

**何时抛出**：价格超出 [0,1]、数量 ≤ 0、市场 ID 格式错误。

**建议处理**：
- ❌ 不要重试（参数错误是确定的）
- ✅ 检查 OrderRequest 构造参数
- ✅ 验证 market_id / token_id 格式

### 4.5 交易所错误（GW_EXCH_001）

**何时抛出**：余额不足、市场已关闭、订单参数被拒绝、签名错误。

**建议处理**：
- ❌ 不要自动重试
- ✅ 检查账户状态（get_balance）
- ✅ 检查市场状态（get_markets）
- ✅ 调整订单参数

### 4.6 请求超时（GW_TO_001）

**何时抛出**：HTTP 请求超过 `api_timeout_ms`。

**建议处理**：
- ✅ 自动重试（中间件已处理）
- ✅ 检查网络延迟
- ✅ 增加 `api_timeout_ms` 配置

### 4.7 序列化错误（GW_SER_001）

**何时抛出**：JSON 字段缺失、类型不匹配。

**建议处理**：
- ❌ 不要重试（API 协议问题）
- ✅ 检查 API 文档与响应结构
- ✅ 升级 pm-api-test 版本

## 5. 使用示例

### 5.1 创建错误

```rust
use pm_gateway::GatewayError;

let err = GatewayError::network("连接被拒绝");
```

### 5.2 检查错误类型

```rust
match result {
    Ok(value) => { /* 成功 */ }
    Err(GatewayError::NetworkError { .. }) => { /* 网络错误 */ }
    Err(GatewayError::AuthenticationError { .. }) => { /* 认证错误 */ }
    Err(_) => { /* 其他 */ }
}
```

### 5.3 是否可重试

```rust
if err.is_retryable() {
    // 重试逻辑（由 Retry Middleware 处理）
} else {
    // 直接失败，不重试
}
```

### 5.4 转换为 GatewayResult

```rust
let err = GatewayError::network("连接失败");
let result = err.to_failed_result("order-123", 100);
// result: GatewayResult { success: false, message: "[网络错误] 连接失败", latency_ms: 100 }
```

### 5.5 显示错误

```rust
println!("{}", err);
// 输出: [GW_NET_001] 连接被拒绝 — 建议: 请检查网络连接和 API 地址是否正确...

println!("{}", err.message_zh());
// 输出: 连接被拒绝

println!("{}", err.suggestion_zh());
// 输出: 请检查网络连接和 API 地址是否正确...
```

## 6. 错误分类

### 6.1 按可重试性

**可重试**（网络、限流、超时）：
```rust
if err.is_retryable() { /* 自动重试 */ }
```

**不可重试**（认证、校验、交易所、序列化）：
```rust
if !err.is_retryable() { /* 立即失败 */ }
```

### 6.2 按来源

| 来源 | 错误类型 |
|------|----------|
| 网络层 | NetworkError / TimeoutError |
| 认证层 | AuthenticationError |
| 业务层 | RateLimitError / ExchangeError |
| 数据层 | ValidationError / SerializationError |

## 7. 错误日志

所有错误都会通过 `tracing::error!` 记录中文日志：

```
[错误] GET /time | id=req-001 | 网络错误: 连接被拒绝
[错误] POST /order | id=req-002 | 认证失败: API 密钥无效
```

## 8. 错误统计

`GatewayMetrics` 自动统计错误次数：

- `total_http_requests` — 总请求数
- `http_failures` — 失败次数
- `http_success_rate()` — 成功率
- `record_retry()` — 重试次数

可以通过 `diagnose_metrics()` 查看完整指标。