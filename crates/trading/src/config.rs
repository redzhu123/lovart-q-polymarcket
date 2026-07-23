//! Trading Configuration（V1.07 第九节）。
//!
//! 从 provider.toml 读取 Trading 配置。
//! 支持环境：dev / paper / sandbox / production。
//! 默认：paper。禁止默认 production。

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::credential::CredentialTomlConfig;

// ============================================================================
// Trading Environment
// ============================================================================

/// Trading 环境（V1.07 第九节）。
///
/// - `Dev`：开发环境，仅 Mock。
/// - `Paper`：纸面交易，模拟成交但不签名/下单。
/// - `Sandbox`：沙盒环境，使用测试网。
/// - `Production`：生产环境，真实交易。
///
/// 默认：Paper。禁止默认 Production。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradingEnvironment {
    /// 开发环境。
    Dev,
    /// 纸面交易（默认）。
    Paper,
    /// 沙盒环境。
    Sandbox,
    /// 生产环境。
    Production,
}

impl TradingEnvironment {
    /// 从字符串解析（不区分大小写）。
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dev" => Some(TradingEnvironment::Dev),
            "paper" => Some(TradingEnvironment::Paper),
            "sandbox" => Some(TradingEnvironment::Sandbox),
            "production" => Some(TradingEnvironment::Production),
            _ => None,
        }
    }

    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            TradingEnvironment::Dev => "开发",
            TradingEnvironment::Paper => "纸面",
            TradingEnvironment::Sandbox => "沙盒",
            TradingEnvironment::Production => "生产",
        }
    }

    /// 是否为生产环境。
    pub fn is_production(&self) -> bool {
        matches!(self, TradingEnvironment::Production)
    }

    /// 是否允许真实交易。
    pub fn allows_real_trading(&self) -> bool {
        matches!(self, TradingEnvironment::Sandbox | TradingEnvironment::Production)
    }
}

impl Default for TradingEnvironment {
    fn default() -> Self {
        TradingEnvironment::Paper
    }
}

impl std::fmt::Display for TradingEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_zh())
    }
}

// ============================================================================
// Provider Config
// ============================================================================

/// 单个 Provider 的 TOML 配置。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProviderTomlConfig {
    /// Provider 类型：mock / polymarket / kalshi。
    #[serde(default)]
    pub provider_type: String,
    /// 是否启用。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 环境。
    #[serde(default)]
    pub environment: String,
    /// HTTP 基础 URL。
    #[serde(default)]
    pub http_url: Option<String>,
    /// WebSocket URL。
    #[serde(default)]
    pub ws_url: Option<String>,
    /// 连接超时（毫秒）。
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,
    /// 请求超时（毫秒）。
    #[serde(default = "default_request_timeout")]
    pub request_timeout_ms: u64,
    /// 最大重试次数。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 凭据。
    #[serde(default)]
    pub credential: CredentialTomlConfig,
}

fn default_enabled() -> bool { true }
fn default_connect_timeout() -> u64 { 5000 }
fn default_request_timeout() -> u64 { 30000 }
fn default_max_retries() -> u32 { 5 }

// ============================================================================
// Trading Config
// ============================================================================

/// Trading 顶层配置（V1.07 第九节）。
///
/// 从 provider.toml 读取。
#[derive(Debug, Clone, Deserialize)]
pub struct TradingConfig {
    /// 当前环境。
    #[serde(default)]
    pub environment: String,

    /// 默认 Provider。
    #[serde(default = "default_default_provider")]
    pub default_provider: String,

    /// Session TTL（秒）。
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: i64,

    /// 心跳间隔（秒）。
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,

    /// 健康检查间隔（秒）。
    #[serde(default = "default_health_interval")]
    pub health_interval_secs: u64,

    /// 自动重连。
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,

    /// 重连最大次数。
    #[serde(default = "default_reconnect_max")]
    pub reconnect_max_attempts: u32,

    /// Provider 配置列表。
    #[serde(default)]
    pub providers: Vec<ProviderTomlConfig>,
}

fn default_default_provider() -> String { "mock".into() }
fn default_session_ttl() -> i64 { 3600 }
fn default_heartbeat_interval() -> u64 { 30 }
fn default_health_interval() -> u64 { 60 }
fn default_auto_reconnect() -> bool { true }
fn default_reconnect_max() -> u32 { 10 }

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            environment: "paper".into(),
            default_provider: default_default_provider(),
            session_ttl_secs: default_session_ttl(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            health_interval_secs: default_health_interval(),
            auto_reconnect: default_auto_reconnect(),
            reconnect_max_attempts: default_reconnect_max(),
            providers: vec![],
        }
    }
}

impl TradingConfig {
    /// 从 provider.toml 加载。
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取 Trading 配置文件失败: {}", path))?;
        let cfg: TradingConfig = toml::from_str(&text)
            .with_context(|| format!("解析 Trading 配置文件失败: {}", path))?;

        // 安全检查：禁止默认 production
        if cfg.environment == "production" {
            anyhow::bail!(
                "安全错误：禁止默认使用 production 环境。请显式设置 environment = \"production\" 并确认。"
            );
        }

        Ok(cfg)
    }

    /// 尝试加载，失败时使用默认值。
    pub fn load_or_default(path: &str) -> Self {
        match Self::load(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Trading 配置加载失败，使用默认配置: {:#}", e);
                TradingConfig::default()
            }
        }
    }

    /// 解析环境。
    pub fn environment_enum(&self) -> TradingEnvironment {
        TradingEnvironment::from_str(&self.environment).unwrap_or_default()
    }

    /// 是否允许真实交易。
    pub fn allows_real_trading(&self) -> bool {
        self.environment_enum().allows_real_trading()
    }

    /// 获取默认 Provider 的配置。
    pub fn default_provider_config(&self) -> Option<&ProviderTomlConfig> {
        self.providers
            .iter()
            .find(|p| p.provider_type == self.default_provider && p.enabled)
    }

    /// 安全摘要。
    pub fn safe_summary(&self) -> String {
        let env_warning = if self.environment == "production" {
            "⚠️ 生产环境"
        } else {
            ""
        };
        format!(
            "Trading 配置: 环境={} ({}){} | 默认Provider={} | Session TTL={}s | 心跳={}s | 健康检查={}s | 自动重连={}",
            self.environment,
            self.environment_enum().as_zh(),
            env_warning,
            self.default_provider,
            self.session_ttl_secs,
            self.heartbeat_interval_secs,
            self.health_interval_secs,
            if self.auto_reconnect { "是" } else { "否" },
        )
    }
}

// ============================================================================
// 默认 provider.toml 内容
// ============================================================================

/// 默认 provider.toml 模板内容。
pub const DEFAULT_PROVIDER_TOML: &str = r#"# Polymarket Quant Platform V1.07 -- Trading Provider 配置
#
# 环境：dev / paper / sandbox / production
# 默认 paper。禁止在未确认的情况下设置为 production。
#
# 安全警告：
#   - 不要在此文件中填写真实的 API Key / Secret / Private Key
#   - 使用环境变量或 .env 文件管理凭据
#   - production 环境需要显式确认

environment = "paper"
default_provider = "mock"

# Session 配置
session_ttl_secs = 3600

# 心跳间隔（秒）
heartbeat_interval_secs = 30

# 健康检查间隔（秒）
health_interval_secs = 60

# 自动重连
auto_reconnect = true
reconnect_max_attempts = 10

# ---- Mock Provider（默认启用）----
[[providers]]
provider_type = "mock"
enabled = true
environment = "paper"

# ---- Polymarket Provider（未来启用）----
# [[providers]]
# provider_type = "polymarket"
# enabled = false
# environment = "paper"
# http_url = "https://clob.polymarket.com"
# ws_url = "wss://ws.polymarket.com"
# connect_timeout_ms = 5000
# request_timeout_ms = 30000
# max_retries = 5
#
# [providers.credential]
# # 注意：不要在此填写真实凭据，使用环境变量：
# #   POLYMARKET_API_KEY / POLYMARKET_API_SECRET / POLYMARKET_WALLET_ADDRESS
# environment = "paper"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_from_str() {
        assert_eq!(
            TradingEnvironment::from_str("dev"),
            Some(TradingEnvironment::Dev)
        );
        assert_eq!(
            TradingEnvironment::from_str("PAPER"),
            Some(TradingEnvironment::Paper)
        );
        assert_eq!(TradingEnvironment::from_str("nope"), None);
    }

    #[test]
    fn environment_not_production_by_default() {
        assert!(!TradingEnvironment::default().is_production());
        assert_eq!(TradingEnvironment::default(), TradingEnvironment::Paper);
    }

    #[test]
    fn environment_allows_real_trading() {
        assert!(!TradingEnvironment::Dev.allows_real_trading());
        assert!(!TradingEnvironment::Paper.allows_real_trading());
        assert!(TradingEnvironment::Sandbox.allows_real_trading());
        assert!(TradingEnvironment::Production.allows_real_trading());
    }

    #[test]
    fn default_config_is_paper() {
        let cfg = TradingConfig::default();
        assert_eq!(cfg.environment, "paper");
        assert!(!cfg.allows_real_trading());
    }

    #[test]
    fn parse_minimal_config() {
        let text = r#"
environment = "paper"
default_provider = "mock"
"#;
        let cfg: TradingConfig = toml::from_str(text).expect("parse");
        assert_eq!(cfg.environment, "paper");
        assert_eq!(cfg.default_provider, "mock");
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let text = r#"
environment = "sandbox"
default_provider = "polymarket"
session_ttl_secs = 1800
heartbeat_interval_secs = 15

[[providers]]
provider_type = "mock"
enabled = true
environment = "paper"

[[providers]]
provider_type = "polymarket"
enabled = true
environment = "sandbox"
http_url = "https://clob.polymarket.com"
max_retries = 3

[providers.credential]
environment = "sandbox"
"#;
        let cfg: TradingConfig = toml::from_str(text).expect("parse");
        assert_eq!(cfg.environment, "sandbox");
        assert_eq!(cfg.default_provider, "polymarket");
        assert_eq!(cfg.providers.len(), 2);
        assert!(cfg.allows_real_trading());
    }

    #[test]
    fn safe_summary_no_secrets() {
        let cfg = TradingConfig::default();
        let summary = cfg.safe_summary();
        assert!(summary.contains("paper"));
        assert!(!summary.contains("production"));
    }

    #[test]
    fn default_provider_toml_is_valid() {
        let _: TradingConfig =
            toml::from_str(DEFAULT_PROVIDER_TOML).expect("default provider.toml should parse");
    }
}
