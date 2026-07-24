# Authentication Infrastructure (P2-06)

## 架构

```
OMS / Execution / PMS
      │
      ▼
┌──────────────────────────┐
│  AuthMiddleware          │  ← 统一认证入口
└─────────┬────────────────┘
          │
┌─────────▼────────────────┐
│  AuthenticationProvider  │  ← PolymarketAuth / KalshiAuth / DexWalletAuth
└─────────┬────────────────┘
          │
┌─────────▼────────────────┐
│  CredentialManager      │  ← 凭证管理
│  SessionManager         │  ← 会话管理
│  Signer                  │  ← 签名器（EIP-712 / EVM / Ed25519）
└──────────────────────────┘
```

## 模块

| 模块 | 文件 | 说明 |
|------|------|------|
| auth_provider | `src/auth_provider/mod.rs` | AuthenticationProvider trait + 实现 |
| credential | `src/credential/mod.rs` | 扩展凭证管理（版本/来源/脱敏） |
| session | `src/session/mod.rs` | 认证会话生命周期管理 |
| signer | `src/signer/mod.rs` | 统一签名接口 |
| middleware | `src/middleware/mod.rs` | AuthMiddleware 统一入口 |
| refresh | `src/refresh/mod.rs` | Token 续期调度器 |
| diagnostics | `src/diagnostics/mod.rs` | 健康诊断命令 |

## AuthenticationProvider Trait

```rust
#[async_trait]
pub trait AuthenticationProvider: Send + Sync {
    fn name(&self) -> &str;
    fn provider_type(&self) -> &str;
    fn live_enabled(&self) -> bool;
    async fn login(&mut self) -> Result<()>;
    async fn logout(&mut self) -> Result<()>;
    async fn refresh(&mut self) -> Result<()>;
    async fn validate(&self) -> Result<bool>;
    async fn health(&self) -> Result<AuthHealth>;
    fn load_credentials(&mut self) -> Result<()>;
    fn save_credentials(&self) -> Result<()>;
}
```

## 实现

- **PolymarketAuth**：Polymarket API 认证（Simulation Only）
- **KalshiAuth**：Kalshi 认证（接口预留）
- **DexWalletAuth**：DEX 钱包认证（接口预留）
- **MockAuthProvider**：测试/演示用

## 安全约束

- 禁止真实交易 / 真实私钥签名
- Execution / OMS / PMS 禁止直接处理认证
- 所有日志使用 tracing，中文输出
- 所有敏感信息自动脱敏
