//! API 测试配置（V1.08）。
//!
//! 独立于业务模块的配置结构。
//! 支持环境变量 + 默认值，不依赖 pm-models。

use serde::{Deserialize, Serialize};

/// 客户端模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMode {
    /// Mock 模式：从 mock/ 目录加载数据，不发送 HTTP 请求。
    Mock,
    /// Live 模式：真实 HTTP 请求到 Polymarket API。
    Live,
}

impl ClientMode {
    pub fn as_zh(&self) -> &'static str {
        match self {
            ClientMode::Mock => "模拟",
            ClientMode::Live => "真实",
        }
    }
}

/// API 测试配置。
///
/// 支持从环境变量读取：
/// - `HTTPS_PROXY` / `https_proxy`：代理地址
/// - `POLYMARKET_API_KEY`：API 密钥
/// - `PM_API_TEST_MODE`：测试模式（mock/live）
/// - `PM_ENABLE_LIVE`：是否允许真实交易测试
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTestConfig {
    /// Polymarket CLOB API 基础 URL。
    #[serde(default = "default_clob_url")]
    pub clob_url: String,

    /// Polymarket Gamma API 基础 URL。
    #[serde(default = "default_gamma_url")]
    pub gamma_url: String,

    /// Polymarket WebSocket URL。
    #[serde(default = "default_ws_url")]
    pub ws_url: String,

    /// HTTP 请求超时（毫秒）。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// 最大重试次数。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// 基础重试延迟（毫秒）。
    #[serde(default = "default_retry_base_ms")]
    pub retry_base_ms: u64,

    /// 最大重试延迟（毫秒）。
    #[serde(default = "default_retry_max_ms")]
    pub retry_max_ms: u64,

    /// 退避乘数。
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// 每秒最大请求数（速率限制）。
    #[serde(default = "default_rate_limit_per_sec")]
    pub rate_limit_per_sec: u32,

    /// 代理 URL（从 HTTPS_PROXY 环境变量读取）。
    #[serde(default)]
    pub proxy_url: Option<String>,

    /// API 密钥（从 POLYMARKET_API_KEY 环境变量读取）。
    #[serde(default)]
    pub api_key: Option<String>,

    /// 客户端模式。
    #[serde(default = "default_mode")]
    pub mode: ClientMode,

    /// 是否允许真实交易测试。
    #[serde(default)]
    pub enable_live: bool,

    /// Mock 数据目录。
    #[serde(default = "default_mock_dir")]
    pub mock_dir: String,
}

// ---- 默认值函数 ----

fn default_clob_url() -> String {
    "https://clob.polymarket.com".into()
}

fn default_gamma_url() -> String {
    "https://gamma-api.polymarket.com".into()
}

fn default_ws_url() -> String {
    "wss://ws.polymarket.com".into()
}

fn default_timeout_ms() -> u64 {
    10000
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_base_ms() -> u64 {
    500
}

fn default_retry_max_ms() -> u64 {
    15000
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

fn default_rate_limit_per_sec() -> u32 {
    10
}

fn default_mode() -> ClientMode {
    ClientMode::Mock
}

fn default_mock_dir() -> String {
    // 工作区顶层共享 fixtures/ 目录（单一 Mock 数据源，跨 crate 复用，禁止重复）。
    // env!("CARGO_MANIFEST_DIR") 在编译期固定为本 crate 目录（crates/api-test），
    // 上溯两级即工作区根。
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures").to_string()
}

impl Default for ApiTestConfig {
    fn default() -> Self {
        Self {
            clob_url: default_clob_url(),
            gamma_url: default_gamma_url(),
            ws_url: default_ws_url(),
            timeout_ms: default_timeout_ms(),
            max_retries: default_max_retries(),
            retry_base_ms: default_retry_base_ms(),
            retry_max_ms: default_retry_max_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            rate_limit_per_sec: default_rate_limit_per_sec(),
            proxy_url: std::env::var("HTTPS_PROXY")
                .or_else(|_| std::env::var("https_proxy"))
                .ok(),
            api_key: std::env::var("POLYMARKET_API_KEY").ok(),
            mode: default_mode(),
            enable_live: false,
            mock_dir: default_mock_dir(),
        }
    }
}

impl ApiTestConfig {
    /// 创建 Live 模式配置。
    pub fn live() -> Self {
        Self {
            mode: ClientMode::Live,
            ..Default::default()
        }
    }

    /// 创建 Mock 模式配置。
    pub fn mock() -> Self {
        Self {
            mode: ClientMode::Mock,
            ..Default::default()
        }
    }

    /// 设置代理。
    pub fn with_proxy(mut self, proxy: &str) -> Self {
        self.proxy_url = Some(proxy.to_string());
        self
    }

    /// 设置 API 密钥。
    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    /// 是否允许真实交易。
    pub fn is_live_enabled(&self) -> bool {
        self.enable_live
    }

    /// 安全摘要（中文）。
    pub fn safety_summary_zh(&self) -> String {
        let mode_str = self.mode.as_zh();
        let live_str = if self.enable_live {
            "⚠️ 真实交易已启用"
        } else {
            "🔒 DryRun 模式"
        };
        let proxy_str = self.proxy_url.as_deref().unwrap_or("未配置");
        let auth_str = if self.api_key.is_some() {
            "已配置"
        } else {
            "未配置"
        };

        format!(
            "【API 测试配置】\n\
             模式: {}\n\
             {}\n\
             CLOB URL: {}\n\
             Gamma URL: {}\n\
             WS URL: {}\n\
             代理: {}\n\
             认证: {}\n\
             超时: {}ms | 最大重试: {} | 速率限制: {}/s",
            mode_str,
            live_str,
            self.clob_url,
            self.gamma_url,
            self.ws_url,
            proxy_str,
            auth_str,
            self.timeout_ms,
            self.max_retries,
            self.rate_limit_per_sec,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_mock_mode() {
        let cfg = ApiTestConfig::default();
        assert_eq!(cfg.mode, ClientMode::Mock);
    }

    #[test]
    fn default_config_has_safe_defaults() {
        let cfg = ApiTestConfig::default();
        assert!(!cfg.enable_live);
        assert_eq!(cfg.max_retries, 3);
        assert!(cfg.timeout_ms > 0);
    }

    #[test]
    fn live_config_uses_clob_url() {
        let cfg = ApiTestConfig::live();
        assert_eq!(cfg.mode, ClientMode::Live);
        assert_eq!(cfg.clob_url, "https://clob.polymarket.com");
    }

    #[test]
    fn mock_config_does_not_connect() {
        let cfg = ApiTestConfig::mock();
        assert_eq!(cfg.mode, ClientMode::Mock);
        assert!(!cfg.enable_live);
    }

    #[test]
    fn builder_pattern() {
        let cfg = ApiTestConfig::default()
            .with_proxy("http://127.0.0.1:7890")
            .with_api_key("test-key");
        assert_eq!(cfg.proxy_url.unwrap(), "http://127.0.0.1:7890");
        assert_eq!(cfg.api_key.unwrap(), "test-key");
    }

    #[test]
    fn safety_summary_contains_chinese() {
        let cfg = ApiTestConfig::default();
        let summary = cfg.safety_summary_zh();
        assert!(summary.contains("模拟"));
        assert!(summary.contains("DryRun"));
    }

    #[test]
    fn live_enabled_shows_warning() {
        let cfg = ApiTestConfig {
            enable_live: true,
            ..ApiTestConfig::default()
        };
        let summary = cfg.safety_summary_zh();
        assert!(summary.contains("真实交易"));
    }
}
