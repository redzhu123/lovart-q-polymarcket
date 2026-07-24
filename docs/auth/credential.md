# Credential Management (P2-06)

## 概述

在 pm-trading Credential 基础上扩展企业级凭证管理。

## 核心类型

### ExtendedCredential

```rust
pub struct ExtendedCredential {
    pub api_key: SensitiveString,       // 自动脱敏
    pub api_secret: SensitiveString,    // 自动脱敏
    pub api_passphrase: SensitiveString,// 自动脱敏
    pub wallet_address: SensitiveString,// 自动脱敏
    pub private_key: SensitiveString,   // 自动脱敏（完全隐藏）
    pub chain_id: Option<u64>,
    pub environment: String,
    pub version: CredentialVersion,     // 凭证版本
    pub source: CredentialSource,       // 来源追踪
    pub created_at: DateTime<Local>,
    pub labels: HashMap<String, String>,
}
```

### SensitiveString

自动脱敏字符串包装器：
- `Display`/`Debug` 自动脱敏
- `reveal()` 显式获取原始值
- `masked()` 保留前4后4
- `masked_address()` 保留前6后4
- `masked_full()` 完全隐藏

### CredentialVersion

语义化版本追踪：`v1.0.0`、`v2.1.3`...

### CredentialSource

凭证来源追踪：
- Environment：环境变量
- ConfigFile：配置文件
- DotEnv：.env 文件
- Kms：KMS（预留）
- Unknown：未知

## CredentialManager

```rust
pub struct CredentialManager {
    credentials: HashMap<String, ExtendedCredential>,
    default_provider: String,
    initialized: bool,
}
```

方法：
- `load_from_env()`：从环境变量加载
- `register(provider, credential)`：注册凭证
- `get(provider)` / `get_default()`：获取凭证
- `has_real_credentials()`：是否有真实凭证
- `safe_summary()`：脱敏摘要

## 环境变量

```
POLYMARKET_API_KEY=<your-key>
POLYMARKET_API_SECRET=<your-secret>
POLYMARKET_API_PASSPHRASE=<your-passphrase>
POLYMARKET_WALLET_ADDRESS=<your-address>
POLYMARKET_PRIVATE_KEY=<your-private-key>
POLYMARKET_CHAIN_ID=137
POLYMARKET_ENV=paper
```
