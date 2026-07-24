# Session Management (P2-06)

## AuthSession

```rust
pub struct AuthSession {
    pub session_id: String,
    pub provider: String,
    pub authenticated: bool,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub created_at: DateTime<Local>,
    pub expires_at: DateTime<Local>,
    pub token_type: String,
    pub scopes: Vec<String>,
    pub metadata: HashMap<String, String>,
}
```

## 生命周期

1. **创建**：`AuthSession::new(provider, ttl_secs, token, refresh_token)`
2. **验证**：`is_expired()` / `expires_soon(secs)` / `needs_renewal()`
3. **续期**：`AuthSessionManager::renew(provider, new_token, new_rt)`
4. **注销**：`AuthSessionManager::invalidate(provider)`
5. **清理**：`AuthSessionManager::purge_expired()`

## AuthSessionManager

```rust
pub struct AuthSessionManager {
    sessions: HashMap<String, AuthSession>,
    default_ttl: i64,
}
```

## TokenRefreshScheduler

自动检测即将过期的 Session 并执行续期：

```rust
pub struct TokenRefreshScheduler {
    pub renewal_threshold_secs: i64,  // 默认 300（5分钟）
    pub max_retries: u32,             // 默认 3
    pub renewal_count: u64,
    pub failure_count: u64,
}
```

## 安全

- Token 在日志中自动脱敏（`mask_api_key`）
- Refresh Token 完全不显示（`[REFRESH_TOKEN]`）
- Session ID 仅显示前 8 位
