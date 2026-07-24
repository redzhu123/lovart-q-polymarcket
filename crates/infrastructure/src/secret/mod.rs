//! 密钥管理模块：统一管理 API Key、Secret、Private Key、Wallet、Token、Credential。
//!
//! 从 `pm-auth::credential` 和 `pm-trading::credential` 提取并统一。
//!
//! # 核心能力
//!
//! - [`SecretManager`] trait：统一的密钥管理接口
//! - [`EnvSecretManager`]：从环境变量加载密钥的默认实现
//! - [`Credential`]：完整的凭证数据结构
//! - [`SensitiveString`]：自动脱敏的敏感字符串
//!
//! # 安全约束
//!
//! - 所有密钥字段使用 [`SensitiveString`]，自动在日志/Display/Debug 中脱敏
//! - 禁止日志输出明文
//! - 支持 .env 和环境变量加载
//! - 未来支持 Vault/KMS（接口预留）

pub mod mask;
pub mod sensitive;

use crate::health::HealthStatus;
use async_trait::async_trait;
use sensitive::SensitiveString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 凭证来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialSource {
    /// 环境变量
    Environment,
    /// 配置文件
    ConfigFile,
    /// .env 文件
    DotEnv,
    /// KMS（预留）
    Kms,
    /// Vault（预留）
    Vault,
    /// 未知来源
    Unknown,
}

impl CredentialSource {
    pub fn as_zh(&self) -> &'static str {
        match self {
            CredentialSource::Environment => "环境变量",
            CredentialSource::ConfigFile => "配置文件",
            CredentialSource::DotEnv => ".env 文件",
            CredentialSource::Kms => "KMS",
            CredentialSource::Vault => "Vault",
            CredentialSource::Unknown => "未知",
        }
    }
}

/// 凭证版本号（语义化版本）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Default for CredentialVersion {
    fn default() -> Self {
        Self {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }
}

/// 完整凭证数据结构
///
/// 所有敏感字段使用 [`SensitiveString`]，自动脱敏。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    /// API 密钥
    pub api_key: SensitiveString,
    /// API 密钥对应的 Secret
    pub api_secret: SensitiveString,
    /// API 密码短语（部分交易所使用）
    pub api_passphrase: SensitiveString,
    /// 钱包地址
    pub wallet_address: SensitiveString,
    /// 私钥
    pub private_key: SensitiveString,
    /// 链 ID
    pub chain_id: Option<u64>,
    /// 环境标识（dev/staging/production）
    pub environment: String,
    /// 凭证来源
    pub source: CredentialSource,
    /// 版本号
    pub version: CredentialVersion,
    /// 标签（可扩展的元数据）
    pub labels: HashMap<String, String>,
}

impl Credential {
    /// 创建空凭证
    pub fn empty() -> Self {
        Self {
            api_key: SensitiveString::default(),
            api_secret: SensitiveString::default(),
            api_passphrase: SensitiveString::default(),
            wallet_address: SensitiveString::default(),
            private_key: SensitiveString::default(),
            chain_id: None,
            environment: "unknown".to_string(),
            source: CredentialSource::Unknown,
            version: CredentialVersion::default(),
            labels: HashMap::new(),
        }
    }

    /// 是否包含真实凭证（非空）
    pub fn is_real(&self) -> bool {
        !self.api_key.is_empty() || !self.private_key.is_empty()
    }

    /// 安全检查：是否可用于真实交易
    pub fn can_trade_real(&self) -> bool {
        self.is_real()
            && (!self.api_key.is_empty() || !self.wallet_address.is_empty())
            && self.environment != "unknown"
    }

    /// 获取安全的摘要信息（所有敏感字段脱敏）
    pub fn safe_summary(&self) -> String {
        format!(
            "Credential(env={}, source={}, has_api_key={}, has_private_key={}, has_wallet={})",
            self.environment,
            self.source.as_zh(),
            !self.api_key.is_empty(),
            !self.private_key.is_empty(),
            !self.wallet_address.is_empty(),
        )
    }
}

impl Default for Credential {
    fn default() -> Self {
        Self::empty()
    }
}

/// 统一的密钥管理 trait
///
/// 所有业务模块通过此接口获取凭证，不得直接读取环境变量或配置文件。
#[async_trait]
pub trait SecretManager: Send + Sync {
    /// 管理器名称
    fn name(&self) -> &str;

    /// 根据提供商标识获取凭证
    fn get(&self, provider: &str) -> Option<&Credential>;

    /// 获取默认凭证
    fn get_default(&self) -> Option<&Credential>;

    /// 注册凭证
    fn register(&mut self, provider: &str, credential: Credential);

    /// 是否有真实凭证
    fn has_real_credentials(&self) -> bool;

    /// 所有提供商标识
    fn provider_names(&self) -> Vec<&str>;

    /// 凭证数量
    fn len(&self) -> usize;

    /// 是否为空
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 安全的摘要输出（所有敏感字段脱敏）
    fn safe_summary(&self) -> String;

    /// 从环境变量加载凭证
    ///
    /// 环境变量命名规则：`{PREFIX}_{NAME}_API_KEY`, `{PREFIX}_{NAME}_API_SECRET` 等
    async fn load_from_env(&mut self, prefix: &str, name: &str) -> anyhow::Result<()>;

    /// 从文件加载凭证（预留接口）
    async fn load_from_file(&mut self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }

    /// 健康检查
    async fn health_check(&self) -> HealthStatus;
}

/// 基于环境变量的密钥管理器（默认实现）
pub struct EnvSecretManager {
    name: String,
    credentials: HashMap<String, Credential>,
    default_provider: Option<String>,
}

impl EnvSecretManager {
    /// 创建新的环境变量密钥管理器
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            credentials: HashMap::new(),
            default_provider: None,
        }
    }

    /// 设置默认提供商标识
    pub fn set_default_provider(&mut self, provider: impl Into<String>) {
        self.default_provider = Some(provider.into());
    }
}

#[async_trait]
impl SecretManager for EnvSecretManager {
    fn name(&self) -> &str {
        &self.name
    }

    fn get(&self, provider: &str) -> Option<&Credential> {
        self.credentials.get(provider)
    }

    fn get_default(&self) -> Option<&Credential> {
        self.default_provider
            .as_ref()
            .and_then(|p| self.credentials.get(p))
    }

    fn register(&mut self, provider: &str, credential: Credential) {
        self.credentials.insert(provider.to_string(), credential);
    }

    fn has_real_credentials(&self) -> bool {
        self.credentials.values().any(|c| c.is_real())
    }

    fn provider_names(&self) -> Vec<&str> {
        self.credentials.keys().map(|k| k.as_str()).collect()
    }

    fn len(&self) -> usize {
        self.credentials.len()
    }

    fn safe_summary(&self) -> String {
        let names: Vec<_> = self
            .credentials
            .iter()
            .map(|(name, cred)| format!("{}: {}", name, cred.safe_summary()))
            .collect();
        format!(
            "EnvSecretManager({}), providers=[{}]",
            self.name,
            names.join(", ")
        )
    }

    async fn load_from_env(&mut self, prefix: &str, name: &str) -> anyhow::Result<()> {
        let env_prefix = format!("{}_{}", prefix.to_uppercase(), name.to_uppercase());

        let read_env = |suffix: &str| -> SensitiveString {
            let key = format!("{}_{}", env_prefix, suffix);
            std::env::var(&key)
                .map(SensitiveString::new)
                .unwrap_or_default()
        };

        let credential = Credential {
            api_key: read_env("API_KEY"),
            api_secret: read_env("API_SECRET"),
            api_passphrase: read_env("API_PASSPHRASE"),
            wallet_address: SensitiveString::default(),
            private_key: SensitiveString::default(),
            chain_id: std::env::var(format!("{}_CHAIN_ID", env_prefix))
                .ok()
                .and_then(|v| v.parse().ok()),
            environment: std::env::var("PM_ENV").unwrap_or_else(|_| "unknown".to_string()),
            source: CredentialSource::Environment,
            version: CredentialVersion::default(),
            labels: HashMap::new(),
        };

        let is_empty = credential.api_key.is_empty()
            && credential.api_secret.is_empty()
            && credential.api_passphrase.is_empty();

        if !is_empty {
            tracing::info!("从环境变量加载凭证: {} (prefix={})", name, env_prefix);
        } else {
            tracing::debug!("环境变量中未找到凭证: {} (prefix={})", name, env_prefix);
        }

        self.register(name, credential);
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        if self.has_real_credentials() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_empty_is_not_real() {
        let cred = Credential::empty();
        assert!(!cred.is_real());
        assert!(!cred.can_trade_real());
    }

    #[test]
    fn credential_with_api_key_is_real() {
        let mut cred = Credential::empty();
        cred.api_key = SensitiveString::new("sk-test-key");
        assert!(cred.is_real());
    }

    #[test]
    fn credential_safe_summary_no_leak() {
        let mut cred = Credential::empty();
        cred.api_key = SensitiveString::new("sk-very-secret-key-value");
        cred.environment = "production".to_string();
        let summary = cred.safe_summary();
        // 不应泄露明文
        assert!(!summary.contains("very-secret-key-value"));
        assert!(summary.contains("production"));
        assert!(summary.contains("has_api_key=true"));
    }

    #[test]
    fn env_secret_manager_register_and_get() {
        let mut mgr = EnvSecretManager::new("test-manager");
        let mut cred = Credential::empty();
        cred.api_key = SensitiveString::new("test-key");
        cred.environment = "test".to_string();
        mgr.register("polymarket", cred);

        assert_eq!(mgr.len(), 1);
        assert!(!mgr.is_empty());
        assert!(mgr.has_real_credentials());

        let retrieved = mgr.get("polymarket");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().api_key.reveal(), "test-key");
    }

    #[test]
    fn env_secret_manager_default_provider() {
        let mut mgr = EnvSecretManager::new("test-manager");
        let mut cred = Credential::empty();
        cred.api_key = SensitiveString::new("default-key");
        mgr.register("polymarket", cred);
        mgr.set_default_provider("polymarket");

        let default = mgr.get_default();
        assert!(default.is_some());
    }

    #[test]
    fn env_secret_manager_provider_names() {
        let mut mgr = EnvSecretManager::new("test-manager");
        mgr.register("a", Credential::empty());
        mgr.register("b", Credential::empty());
        let names = mgr.provider_names();
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn env_secret_manager_safe_summary() {
        let mut mgr = EnvSecretManager::new("test-manager");
        let mut cred = Credential::empty();
        cred.api_key = SensitiveString::new("secret-1234");
        mgr.register("test", cred);
        let summary = mgr.safe_summary();
        // 不应泄露明文
        assert!(!summary.contains("secret-1234"));
        assert!(summary.contains("EnvSecretManager"));
    }

    #[test]
    fn credential_source_zh_names() {
        assert_eq!(CredentialSource::Environment.as_zh(), "环境变量");
        assert_eq!(CredentialSource::ConfigFile.as_zh(), "配置文件");
        assert_eq!(CredentialSource::Kms.as_zh(), "KMS");
    }
}
