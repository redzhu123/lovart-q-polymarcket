//! 凭证管理（P2-06 第三节）。
//!
//! 在 pm-trading Credential 基础上扩展企业级特性：
//! - CredentialVersion：凭证版本追踪
//! - CredentialSource：凭证来源（环境变量/配置文件/KMS）
//! - SensitiveString：自动脱敏的字符串包装
//!
//! 安全要求：
//! - 禁止在日志输出敏感信息
//! - 所有 Display/Debug 自动脱敏
//! - 支持 .env / 环境变量 / 未来 KMS

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ============================================================================
// CredentialVersion — 凭证版本
// ============================================================================

/// 凭证版本号（语义化版本）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl CredentialVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn v1() -> Self {
        Self::new(1, 0, 0)
    }
}

impl fmt::Display for CredentialVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ============================================================================
// CredentialSource — 凭证来源
// ============================================================================

/// 凭证来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialSource {
    /// 环境变量。
    Environment,
    /// 配置文件。
    ConfigFile,
    /// .env 文件。
    DotEnv,
    /// KMS（接口预留）。
    Kms,
    /// 未知来源。
    Unknown,
}

impl CredentialSource {
    pub fn as_zh(&self) -> &'static str {
        match self {
            CredentialSource::Environment => "环境变量",
            CredentialSource::ConfigFile => "配置文件",
            CredentialSource::DotEnv => ".env 文件",
            CredentialSource::Kms => "KMS",
            CredentialSource::Unknown => "未知",
        }
    }
}

impl Default for CredentialSource {
    fn default() -> Self {
        CredentialSource::Unknown
    }
}

impl fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_zh())
    }
}

// ============================================================================
// SensitiveString — 自动脱敏字符串
// ============================================================================

/// 自动脱敏字符串包装器。
///
/// Display 和 Debug 输出自动脱敏，防止日志泄露。
/// 内部值通过 `reveal()` 方法显式访问。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveString(String);

impl SensitiveString {
    /// 创建脱敏字符串。
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }

    /// 显式获取原始值（需审计使用）。
    pub fn reveal(&self) -> &str {
        &self.0
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 长度。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 脱敏后的字符串：保留前 4 和后 4 字符。
    pub fn masked(&self) -> String {
        pm_trading::mask::mask_api_key(&self.0)
    }

    /// 脱敏后的钱包地址：保留前 6 和后 4 字符。
    pub fn masked_address(&self) -> String {
        pm_trading::mask::mask_address(&self.0)
    }

    /// 完全隐藏（用于私钥等）。
    pub fn masked_full(&self) -> String {
        if self.0.is_empty() {
            "无".to_string()
        } else {
            "[PRIVATE_KEY]".to_string()
        }
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SensitiveString({})", self.masked())
    }
}

impl fmt::Display for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.masked())
    }
}

impl Default for SensitiveString {
    fn default() -> Self {
        Self(String::new())
    }
}

impl From<String> for SensitiveString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SensitiveString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ============================================================================
// ExtendedCredential — 扩展凭证
// ============================================================================

/// 扩展凭证（在 pm-trading Credential 基础上增加版本/来源/元数据）。
#[derive(Debug, Clone)]
pub struct ExtendedCredential {
    /// API Key（脱敏）。
    pub api_key: SensitiveString,
    /// API Secret（脱敏）。
    pub api_secret: SensitiveString,
    /// API Passphrase（脱敏）。
    pub api_passphrase: SensitiveString,
    /// 钱包地址（脱敏）。
    pub wallet_address: SensitiveString,
    /// 私钥（脱敏，禁止打印）。
    pub private_key: SensitiveString,
    /// Chain Id（如 137=Polygon）。
    pub chain_id: Option<u64>,
    /// 环境。
    pub environment: String,
    /// 凭证版本。
    pub version: CredentialVersion,
    /// 凭证来源。
    pub source: CredentialSource,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 标签/备注。
    pub labels: HashMap<String, String>,
}

impl ExtendedCredential {
    /// 创建空凭证。
    pub fn empty() -> Self {
        Self {
            api_key: SensitiveString::default(),
            api_secret: SensitiveString::default(),
            api_passphrase: SensitiveString::default(),
            wallet_address: SensitiveString::default(),
            private_key: SensitiveString::default(),
            chain_id: None,
            environment: "paper".to_string(),
            version: CredentialVersion::v1(),
            source: CredentialSource::Unknown,
            created_at: Local::now(),
            labels: HashMap::new(),
        }
    }

    /// 从 pm-trading Credential 转换。
    pub fn from_trading_credential(cred: &pm_trading::Credential) -> Self {
        Self {
            api_key: SensitiveString::new(cred.api_key.clone().unwrap_or_default()),
            api_secret: SensitiveString::new(cred.api_secret.clone().unwrap_or_default()),
            api_passphrase: SensitiveString::new(cred.api_passphrase.clone().unwrap_or_default()),
            wallet_address: SensitiveString::new(cred.wallet_address.clone().unwrap_or_default()),
            private_key: SensitiveString::new(cred.private_key.clone().unwrap_or_default()),
            chain_id: cred.chain_id,
            environment: cred.environment.clone(),
            version: CredentialVersion::v1(),
            source: CredentialSource::Environment,
            created_at: Local::now(),
            labels: HashMap::new(),
        }
    }

    /// 是否包含真实凭据。
    pub fn is_real(&self) -> bool {
        !self.api_key.is_empty() || !self.private_key.is_empty()
    }

    /// 安全摘要（中文，脱敏）。
    pub fn safe_summary(&self) -> String {
        format!(
            "API Key: {} | Secret: {} | 钱包: {} | 私钥: {} | 环境: {} | Chain: {} | 版本: {} | 来源: {}",
            if self.api_key.is_empty() {
                "无".to_string()
            } else {
                self.api_key.masked()
            },
            if self.api_secret.is_empty() {
                "无".to_string()
            } else {
                "[SECRET]".to_string()
            },
            if self.wallet_address.is_empty() {
                "无".to_string()
            } else {
                self.wallet_address.masked_address()
            },
            if self.private_key.is_empty() {
                "无".to_string()
            } else {
                "[PRIVATE_KEY]".to_string()
            },
            self.environment,
            self.chain_id
                .map(|c| c.to_string())
                .unwrap_or_else(|| "无".to_string()),
            self.version.to_string(),
            self.source.as_zh(),
        )
    }
}

impl Default for ExtendedCredential {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// CredentialManager — 扩展凭证管理器
// ============================================================================

/// 扩展凭证管理器（P2-06 第三节）。
///
/// 在 pm-trading CredentialManager 基础上增加：
/// - 版本管理
/// - 来源追踪
/// - KMS 接口预留
/// - 多环境支持
pub struct CredentialManager {
    /// Provider 名称 -> 凭证。
    credentials: HashMap<String, ExtendedCredential>,
    /// 默认 Provider 名称。
    default_provider: String,
    /// 是否已初始化。
    initialized: bool,
}

impl CredentialManager {
    /// 创建空的凭证管理器。
    pub fn new() -> Self {
        Self {
            credentials: HashMap::new(),
            default_provider: "mock".to_string(),
            initialized: false,
        }
    }

    /// 从 pm-trading CredentialManager 初始化。
    pub fn from_trading_manager(trading_mgr: &pm_trading::CredentialManager) -> Self {
        let mut mgr = Self::new();
        mgr.default_provider = trading_mgr.default_provider_name().to_string();
        for name in trading_mgr.provider_names() {
            if let Some(cred) = trading_mgr.get(name) {
                let ext = ExtendedCredential::from_trading_credential(cred);
                mgr.credentials.insert(name.to_string(), ext);
            }
        }
        mgr.initialized = true;
        mgr
    }

    /// 从环境变量加载凭据。
    pub fn load_from_env(&mut self) -> Result<()> {
        self.load_provider_from_env("POLYMARKET", "polymarket")?;
        self.load_provider_from_env("KALSHI", "kalshi").ok();
        self.load_provider_from_env("DEX_WALLET", "dex_wallet").ok();
        self.initialized = true;
        Ok(())
    }

    fn load_provider_from_env(&mut self, prefix: &str, name: &str) -> Result<()> {
        let api_key_var = format!("{}_API_KEY", prefix);
        if let Ok(key) = std::env::var(&api_key_var) {
            if key.is_empty() {
                return Ok(());
            }
            let mut cred = ExtendedCredential::empty();
            cred.api_key = SensitiveString::new(key);
            cred.api_secret = SensitiveString::new(
                std::env::var(format!("{}_API_SECRET", prefix)).unwrap_or_default(),
            );
            cred.api_passphrase = SensitiveString::new(
                std::env::var(format!("{}_API_PASSPHRASE", prefix)).unwrap_or_default(),
            );
            cred.wallet_address = SensitiveString::new(
                std::env::var(format!("{}_WALLET_ADDRESS", prefix)).unwrap_or_default(),
            );
            cred.private_key = SensitiveString::new(
                std::env::var(format!("{}_PRIVATE_KEY", prefix)).unwrap_or_default(),
            );
            cred.chain_id = std::env::var(format!("{}_CHAIN_ID", prefix))
                .ok()
                .and_then(|v| v.parse().ok());
            cred.environment =
                std::env::var(format!("{}_ENV", prefix)).unwrap_or_else(|_| "paper".into());
            cred.source = CredentialSource::Environment;
            cred.version = CredentialVersion::v1();
            cred.created_at = Local::now();
            self.credentials.insert(name.to_string(), cred);
        }
        Ok(())
    }

    /// 注册 Provider 凭据。
    pub fn register(&mut self, provider: &str, credential: ExtendedCredential) {
        tracing::info!(
            provider = %provider,
            source = %credential.source,
            version = %credential.version,
            "注册凭证"
        );
        self.credentials.insert(provider.to_string(), credential);
    }

    /// 获取指定 Provider 的凭据。
    pub fn get(&self, provider: &str) -> Option<&ExtendedCredential> {
        self.credentials.get(provider)
    }

    /// 获取默认 Provider 的凭据。
    pub fn get_default(&self) -> Option<&ExtendedCredential> {
        self.credentials.get(&self.default_provider)
    }

    /// 是否有真实凭据。
    pub fn has_real_credentials(&self) -> bool {
        self.credentials.values().any(|c| c.is_real())
    }

    /// 设置默认 Provider。
    pub fn set_default_provider(&mut self, name: &str) {
        self.default_provider = name.to_string();
    }

    /// 获取默认 Provider 名称。
    pub fn default_provider_name(&self) -> &str {
        &self.default_provider
    }

    /// 列出所有已注册的 Provider 名称。
    pub fn provider_names(&self) -> Vec<&str> {
        self.credentials.keys().map(|s| s.as_str()).collect()
    }

    /// 是否已初始化。
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// 获取凭据数量。
    pub fn len(&self) -> usize {
        self.credentials.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }

    /// 安全摘要（中文，所有凭据脱敏）。
    pub fn safe_summary(&self) -> String {
        if self.credentials.is_empty() {
            return "无凭据配置（Mock 模式）".to_string();
        }
        let mut lines: Vec<String> = vec![format!(
            "凭据数量: {} | 默认: {} | 已初始化: {}",
            self.credentials.len(),
            self.default_provider,
            if self.initialized { "是" } else { "否" },
        )];
        for (name, cred) in &self.credentials {
            lines.push(format!("  {}: {}", name, cred.safe_summary()));
        }
        lines.join("\n")
    }

    /// 保存凭据到文件（未来 KMS 接口预留，当前仅打日志）。
    pub fn save_credentials(&self, _path: &str) -> Result<()> {
        tracing::info!("保存凭据（接口预留，当前仅日志）");
        if self.has_real_credentials() {
            tracing::warn!("检测到真实凭据，禁止写入文件（安全策略）");
        }
        Ok(())
    }
}

impl Default for CredentialManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_string_masking() {
        let s = SensitiveString::new("abcdefghijklmnop");
        let display = format!("{}", s);
        assert!(display.contains("abcd"));
        assert!(display.contains("mnop"));
        assert!(!display.contains("abcdefghijklmnop"));
    }

    #[test]
    fn sensitive_string_empty() {
        let s = SensitiveString::default();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn sensitive_string_reveal() {
        let s = SensitiveString::new("secret123");
        assert_eq!(s.reveal(), "secret123");
    }

    #[test]
    fn sensitive_string_debug_masks() {
        let s = SensitiveString::new("my-api-key-value");
        let debug = format!("{:?}", s);
        assert!(!debug.contains("my-api-key-value"));
        assert!(debug.contains("SensitiveString"));
    }

    #[test]
    fn credential_version_display() {
        let v = CredentialVersion::new(2, 1, 3);
        assert_eq!(v.to_string(), "v2.1.3");
    }

    #[test]
    fn credential_version_v1() {
        let v = CredentialVersion::v1();
        assert_eq!(v.to_string(), "v1.0.0");
    }

    #[test]
    fn credential_source_zh() {
        assert_eq!(CredentialSource::Environment.as_zh(), "环境变量");
        assert_eq!(CredentialSource::ConfigFile.as_zh(), "配置文件");
        assert_eq!(CredentialSource::Kms.as_zh(), "KMS");
    }

    #[test]
    fn extended_credential_empty_is_not_real() {
        let cred = ExtendedCredential::empty();
        assert!(!cred.is_real());
    }

    #[test]
    fn extended_credential_with_key_is_real() {
        let mut cred = ExtendedCredential::empty();
        cred.api_key = SensitiveString::new("test-key");
        assert!(cred.is_real());
    }

    #[test]
    fn extended_credential_safe_summary_masks() {
        let mut cred = ExtendedCredential::empty();
        cred.api_key = SensitiveString::new("my-api-key-abcdefgh");
        cred.wallet_address = SensitiveString::new("0x1234567890abcdef1234567890abcdef12345678");
        let summary = cred.safe_summary();
        assert!(!summary.contains("my-api-key-abcdefgh"));
        assert!(!summary.contains("0x1234567890abcdef1234567890abcdef12345678"));
        assert!(summary.contains("v1.0.0"));
    }

    #[test]
    fn credential_manager_starts_empty() {
        let mgr = CredentialManager::new();
        assert!(mgr.is_empty());
        assert!(!mgr.has_real_credentials());
        assert!(!mgr.is_initialized());
    }

    #[test]
    fn credential_manager_register_and_get() {
        let mut mgr = CredentialManager::new();
        mgr.register("test", ExtendedCredential::empty());
        assert!(mgr.get("test").is_some());
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.provider_names().len(), 1);
    }

    #[test]
    fn credential_manager_default_provider() {
        let mut mgr = CredentialManager::new();
        mgr.register("polymarket", ExtendedCredential::empty());
        mgr.set_default_provider("polymarket");
        assert_eq!(mgr.default_provider_name(), "polymarket");
        assert!(mgr.get_default().is_some());
    }

    #[test]
    fn credential_manager_safe_summary() {
        let mgr = CredentialManager::new();
        let summary = mgr.safe_summary();
        assert!(summary.contains("Mock"));
    }

    #[test]
    fn credential_manager_save_credentials_is_safe() {
        let mgr = CredentialManager::new();
        assert!(mgr.save_credentials("/tmp/test").is_ok());
    }
}
