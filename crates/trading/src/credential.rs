//! Credential Manager（V1.07 第三节）。
//!
//! 统一管理所有认证凭据：
//! - API Key
//! - Secret
//! - Passphrase
//! - Address（钱包地址）
//! - Chain Id
//! - Environment
//!
//! 凭据来源优先级：环境变量 > .env > config.toml。
//! 禁止硬编码凭据。未来支持 KMS。
//!
//! 安全要求（第十二节）：
//! - 禁止打印 API Key / Secret / Private Key
//! - 日志输出自动脱敏

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

use crate::mask;

// ============================================================================
// Credential
// ============================================================================

/// 单个 Provider 的凭据集合。
#[derive(Debug, Clone, Default)]
pub struct Credential {
    /// API Key（日志时脱敏）。
    pub api_key: Option<String>,
    /// API Secret（日志时脱敏）。
    pub api_secret: Option<String>,
    /// API Passphrase（日志时脱敏）。
    pub api_passphrase: Option<String>,
    /// 钱包地址（日志时脱敏）。
    pub wallet_address: Option<String>,
    /// 私钥（日志时脱敏，禁止打印）。
    pub private_key: Option<String>,
    /// Chain Id（如 137=Polygon）。
    pub chain_id: Option<u64>,
    /// 环境：dev / paper / sandbox / production。
    pub environment: String,
}

impl Credential {
    /// 创建空凭据（用于 Mock Provider）。
    pub fn empty() -> Self {
        Self {
            environment: "paper".to_string(),
            ..Default::default()
        }
    }

    /// 是否包含真实凭据（非 Mock）。
    pub fn is_real(&self) -> bool {
        self.api_key.is_some() || self.private_key.is_some()
    }

    /// 安全摘要（脱敏后的凭据信息）。
    pub fn safe_summary(&self) -> String {
        let api = self
            .api_key
            .as_ref()
            .map(|k| mask::mask_api_key(k))
            .unwrap_or_else(|| "无".to_string());
        let addr = self
            .wallet_address
            .as_ref()
            .map(|a| mask::mask_address(a))
            .unwrap_or_else(|| "无".to_string());
        let has_secret = if self.api_secret.is_some() {
            "[SECRET]"
        } else {
            "无"
        };
        let has_private = if self.private_key.is_some() {
            "[PRIVATE_KEY]"
        } else {
            "无"
        };
        format!(
            "API Key: {} | Secret: {} | 钱包: {} | 私钥: {} | 环境: {} | Chain: {}",
            api,
            has_secret,
            addr,
            has_private,
            self.environment,
            self.chain_id
                .map(|c| c.to_string())
                .unwrap_or_else(|| "无".to_string())
        )
    }
}

// ============================================================================
// Credential Manager
// ============================================================================

/// 凭据管理器（V1.07 第三节）。
///
/// 统一管理所有 Provider 的凭据。
/// 加载优先级：环境变量 > .env 文件 > config.toml > 默认值。
pub struct CredentialManager {
    /// Provider 名称 -> Credential。
    credentials: HashMap<String, Credential>,
    /// 默认 Provider 名称。
    default_provider: String,
}

impl CredentialManager {
    /// 创建空的凭据管理器。
    pub fn new() -> Self {
        Self {
            credentials: HashMap::new(),
            default_provider: "mock".to_string(),
        }
    }

    /// 从环境变量加载凭据。
    ///
    /// 环境变量命名规则：
    /// - `POLYMARKET_API_KEY`
    /// - `POLYMARKET_API_SECRET`
    /// - `POLYMARKET_API_PASSPHRASE`
    /// - `POLYMARKET_WALLET_ADDRESS`
    /// - `POLYMARKET_PRIVATE_KEY`
    /// - `POLYMARKET_CHAIN_ID`
    /// - `POLYMARKET_ENV`
    pub fn load_from_env(&mut self) -> Result<()> {
        // Polymarket
        if let Ok(key) = std::env::var("POLYMARKET_API_KEY") {
            let mut cred = Credential::empty();
            cred.api_key = Some(key);
            cred.api_secret = std::env::var("POLYMARKET_API_SECRET").ok();
            cred.api_passphrase = std::env::var("POLYMARKET_API_PASSPHRASE").ok();
            cred.wallet_address = std::env::var("POLYMARKET_WALLET_ADDRESS").ok();
            cred.private_key = std::env::var("POLYMARKET_PRIVATE_KEY").ok();
            cred.chain_id = std::env::var("POLYMARKET_CHAIN_ID")
                .ok()
                .and_then(|v| v.parse().ok());
            cred.environment = std::env::var("POLYMARKET_ENV").unwrap_or_else(|_| "paper".into());
            self.credentials.insert("polymarket".to_string(), cred);
        }

        // 通用
        if let Ok(key) = std::env::var("TRADING_API_KEY") {
            let mut cred = Credential::empty();
            cred.api_key = Some(key);
            cred.api_secret = std::env::var("TRADING_API_SECRET").ok();
            cred.wallet_address = std::env::var("TRADING_WALLET_ADDRESS").ok();
            cred.private_key = std::env::var("TRADING_PRIVATE_KEY").ok();
            cred.environment = std::env::var("TRADING_ENV").unwrap_or_else(|_| "paper".into());
            self.credentials.insert("default".to_string(), cred);
        }

        Ok(())
    }

    /// 注册 Provider 凭据。
    pub fn register(&mut self, provider: &str, credential: Credential) {
        self.credentials.insert(provider.to_string(), credential);
    }

    /// 获取指定 Provider 的凭据。
    pub fn get(&self, provider: &str) -> Option<&Credential> {
        self.credentials.get(provider)
    }

    /// 获取默认 Provider 的凭据。
    pub fn get_default(&self) -> Option<&Credential> {
        self.credentials.get(&self.default_provider)
    }

    /// 是否有真实凭据（非 Mock）。
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

    /// 安全摘要（所有凭据脱敏）。
    pub fn safe_summary(&self) -> String {
        if self.credentials.is_empty() {
            return "无凭据配置（Mock 模式）".to_string();
        }
        let lines: Vec<String> = self
            .credentials
            .iter()
            .map(|(name, cred)| format!("  {}: {}", name, cred.safe_summary()))
            .collect();
        format!(
            "凭据数量: {} | 默认: {}\n{}",
            self.credentials.len(),
            self.default_provider,
            lines.join("\n")
        )
    }
}

impl Default for CredentialManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TOML 配置中的凭据段
// ============================================================================

/// config.toml / provider.toml 中的凭据配置。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CredentialTomlConfig {
    /// API Key。
    #[serde(default)]
    pub api_key: Option<String>,
    /// API Secret。
    #[serde(default)]
    pub api_secret: Option<String>,
    /// API Passphrase。
    #[serde(default)]
    pub api_passphrase: Option<String>,
    /// 钱包地址。
    #[serde(default)]
    pub wallet_address: Option<String>,
    /// 私钥。
    #[serde(default)]
    pub private_key: Option<String>,
    /// Chain Id。
    #[serde(default)]
    pub chain_id: Option<u64>,
    /// 环境。
    #[serde(default = "default_cred_env")]
    pub environment: String,
}

fn default_cred_env() -> String {
    "paper".into()
}

impl CredentialTomlConfig {
    /// 转换为 Credential。
    pub fn to_credential(&self) -> Credential {
        Credential {
            api_key: self.api_key.clone(),
            api_secret: self.api_secret.clone(),
            api_passphrase: self.api_passphrase.clone(),
            wallet_address: self.wallet_address.clone(),
            private_key: self.private_key.clone(),
            chain_id: self.chain_id,
            environment: self.environment.clone(),
        }
    }

    /// 是否为空配置。
    pub fn is_empty(&self) -> bool {
        self.api_key.is_none()
            && self.api_secret.is_none()
            && self.wallet_address.is_none()
            && self.private_key.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_empty_is_not_real() {
        let cred = Credential::empty();
        assert!(!cred.is_real());
    }

    #[test]
    fn credential_with_api_key_is_real() {
        let mut cred = Credential::empty();
        cred.api_key = Some("test-key".into());
        assert!(cred.is_real());
    }

    #[test]
    fn safe_summary_masks_sensitive_data() {
        let mut cred = Credential::empty();
        cred.api_key = Some("abcdefghijklmnop".into());
        cred.api_secret = Some("secret123".into());
        cred.wallet_address = Some("0x1234567890abcdef1234567890abcdef12345678".into());
        let summary = cred.safe_summary();
        assert!(summary.contains("abcd...mnop"));
        assert!(summary.contains("[SECRET]"));
        assert!(summary.contains("0x1234...5678"));
    }

    #[test]
    fn credential_manager_starts_empty() {
        let mgr = CredentialManager::new();
        assert!(mgr.get("polymarket").is_none());
        assert!(!mgr.has_real_credentials());
    }

    #[test]
    fn credential_manager_register_and_get() {
        let mut mgr = CredentialManager::new();
        mgr.register("test", Credential::empty());
        assert!(mgr.get("test").is_some());
        assert_eq!(mgr.provider_names().len(), 1);
    }

    #[test]
    fn credential_manager_safe_summary_empty() {
        let mgr = CredentialManager::new();
        assert!(mgr.safe_summary().contains("Mock"));
    }

    #[test]
    fn toml_config_empty_check() {
        let cfg = CredentialTomlConfig::default();
        assert!(cfg.is_empty());
        let cfg = CredentialTomlConfig {
            api_key: Some("k".into()),
            ..Default::default()
        };
        assert!(!cfg.is_empty());
    }
}
