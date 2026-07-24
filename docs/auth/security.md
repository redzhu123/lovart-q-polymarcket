# Security Guidelines (P2-06)

## 密钥管理

### 禁止事项
- ❌ 密钥写入 Git
- ❌ 密钥写入日志
- ❌ 密钥硬编码
- ❌ 密钥在 Display/Debug 中暴露

### 支持方式
- ✅ 环境变量
- ✅ .env 文件
- ✅ TOML 配置文件（不推荐存放密钥）
- ✅ KMS（接口预留）

## 脱敏规则

| 类型 | 函数 | 示例 |
|------|------|------|
| API Key | `mask_api_key()` | `abcd************xyz` |
| 钱包地址 | `mask_address()` | `0x1234********5678` |
| Secret | `mask_secret()` | `[SECRET]` |
| Passphrase | `mask_passphrase()` | `[PASSPHRASE]` |
| Private Key | `mask_private_key()` | `[PRIVATE_KEY]` |

## SensitiveString

所有凭证字段使用 `SensitiveString` 包装：
- `Display` 自动脱敏
- `Debug` 自动脱敏
- `reveal()` 显式访问（需审计）
- 序列化/反序列化保持原始值

## 日志规范

- 统一使用 `tracing`，中文输出
- 禁止 `println!` 在库代码中
- 所有敏感信息输出前经过脱敏函数
- 禁止在日志中输出完整的 API Key / Secret / Private Key

## Simulation Only

- 所有签名均为模拟占位
- 不加载真实私钥
- 不产生真实交易签名
- `live_enabled()` 始终返回 `false`
