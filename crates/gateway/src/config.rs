//! Gateway 配置（V1.08 第十节）。
//!
//! 所有安全相关配置从此读取，禁止写死。
//! 默认 DryRun — 真实下单必须显式配置 `enable_live=true`。

use serde::Deserialize;

/// Gateway 配置（从 config.toml [gateway] 段读取）。
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    /// Gateway 类型："mock" | "polymarket" | "kalshi" | "dex" | "cex"。
    #[serde(default = "default_gateway_type")]
    pub gateway_type: String,

    /// 是否启用真实交易。默认 false（DryRun）。
    #[serde(default)]
    pub enable_live: bool,

    /// Polymarket API 基础 URL。
    #[serde(default = "default_polymarket_api_url")]
    pub polymarket_api_url: String,

    /// Polymarket WebSocket URL。
    #[serde(default = "default_polymarket_ws_url")]
    pub polymarket_ws_url: String,

    /// API 密钥（从环境变量读取，不写配置文件）。
    #[serde(default)]
    pub api_key: String,

    /// API 密钥环境变量名。
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// API 私钥环境变量名。
    #[serde(default = "default_api_secret_env")]
    pub api_secret_env: String,

    /// API 口令环境变量名。
    #[serde(default = "default_api_passphrase_env")]
    pub api_passphrase_env: String,

    // ---- 重试配置 ----
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

    // ---- Circuit Breaker ----
    /// 断路器失败阈值（连续失败次数）。
    #[serde(default = "default_cb_failure_threshold")]
    pub cb_failure_threshold: u32,

    /// 断路器恢复超时（毫秒）。
    #[serde(default = "default_cb_recovery_timeout_ms")]
    pub cb_recovery_timeout_ms: u64,

    /// 断路器半开最大请求数。
    #[serde(default = "default_cb_half_open_max")]
    pub cb_half_open_max: u32,

    // ---- 速率限制 ----
    /// 每秒最大请求数。
    #[serde(default = "default_rate_limit_per_sec")]
    pub rate_limit_per_sec: u32,

    /// 每分钟最大请求数。
    #[serde(default = "default_rate_limit_per_min")]
    pub rate_limit_per_min: u32,

    // ---- 健康检查 ----
    /// 健康检查间隔（秒）。
    #[serde(default = "default_health_check_interval_secs")]
    pub health_check_interval_secs: u64,

    /// API 超时（毫秒）。
    #[serde(default = "default_api_timeout_ms")]
    pub api_timeout_ms: u64,

    // ---- 同步 ----
    /// 订单同步间隔（秒）。
    #[serde(default = "default_order_sync_interval_secs")]
    pub order_sync_interval_secs: u64,

    /// 余额同步间隔（秒）。
    #[serde(default = "default_balance_sync_interval_secs")]
    pub balance_sync_interval_secs: u64,

    /// 持仓同步间隔（秒）。
    #[serde(default = "default_position_sync_interval_secs")]
    pub position_sync_interval_secs: u64,

    // ---- Metrics ----
    /// Metrics CSV 路径。
    #[serde(default = "default_gateway_metrics_csv")]
    pub gateway_metrics_csv: String,

    /// 健康检查 CSV 路径。
    #[serde(default = "default_gateway_health_csv")]
    pub gateway_health_csv: String,
}

// ---- 默认值函数 ----

fn default_gateway_type() -> String { "mock".into() }
fn default_polymarket_api_url() -> String { "https://clob.polymarket.com".into() }
fn default_polymarket_ws_url() -> String { "wss://ws.polymarket.com".into() }
fn default_api_key_env() -> String { "POLYMARKET_API_KEY".into() }
fn default_api_secret_env() -> String { "POLYMARKET_API_SECRET".into() }
fn default_api_passphrase_env() -> String { "POLYMARKET_API_PASSPHRASE".into() }
fn default_max_retries() -> u32 { 3 }
fn default_retry_base_ms() -> u64 { 500 }
fn default_retry_max_ms() -> u64 { 15000 }
fn default_backoff_multiplier() -> f64 { 2.0 }
fn default_cb_failure_threshold() -> u32 { 5 }
fn default_cb_recovery_timeout_ms() -> u64 { 30000 }
fn default_cb_half_open_max() -> u32 { 3 }
fn default_rate_limit_per_sec() -> u32 { 10 }
fn default_rate_limit_per_min() -> u32 { 300 }
fn default_health_check_interval_secs() -> u64 { 30 }
fn default_api_timeout_ms() -> u64 { 10000 }
fn default_order_sync_interval_secs() -> u64 { 5 }
fn default_balance_sync_interval_secs() -> u64 { 30 }
fn default_position_sync_interval_secs() -> u64 { 15 }
fn default_gateway_metrics_csv() -> String { "data/gateway_metrics.csv".into() }
fn default_gateway_health_csv() -> String { "data/gateway_health.csv".into() }

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            gateway_type: default_gateway_type(),
            enable_live: false,
            polymarket_api_url: default_polymarket_api_url(),
            polymarket_ws_url: default_polymarket_ws_url(),
            api_key: String::new(),
            api_key_env: default_api_key_env(),
            api_secret_env: default_api_secret_env(),
            api_passphrase_env: default_api_passphrase_env(),
            max_retries: default_max_retries(),
            retry_base_ms: default_retry_base_ms(),
            retry_max_ms: default_retry_max_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            cb_failure_threshold: default_cb_failure_threshold(),
            cb_recovery_timeout_ms: default_cb_recovery_timeout_ms(),
            cb_half_open_max: default_cb_half_open_max(),
            rate_limit_per_sec: default_rate_limit_per_sec(),
            rate_limit_per_min: default_rate_limit_per_min(),
            health_check_interval_secs: default_health_check_interval_secs(),
            api_timeout_ms: default_api_timeout_ms(),
            order_sync_interval_secs: default_order_sync_interval_secs(),
            balance_sync_interval_secs: default_balance_sync_interval_secs(),
            position_sync_interval_secs: default_position_sync_interval_secs(),
            gateway_metrics_csv: default_gateway_metrics_csv(),
            gateway_health_csv: default_gateway_health_csv(),
        }
    }
}

impl GatewayConfig {
    /// 从 pm-models 的 GatewayRawConfig 桥接构建（V1.08）。
    pub fn from_raw(raw: &pm_models::config::GatewayRawConfig) -> Self {
        Self {
            gateway_type: raw.gateway_type.clone(),
            enable_live: raw.enable_live,
            polymarket_api_url: raw.polymarket_api_url.clone(),
            polymarket_ws_url: raw.polymarket_ws_url.clone(),
            api_key_env: raw.api_key_env.clone(),
            api_secret_env: raw.api_secret_env.clone(),
            api_passphrase_env: raw.api_passphrase_env.clone(),
            max_retries: raw.max_retries,
            retry_base_ms: raw.retry_base_ms,
            retry_max_ms: raw.retry_max_ms,
            backoff_multiplier: raw.backoff_multiplier,
            cb_failure_threshold: raw.cb_failure_threshold,
            cb_recovery_timeout_ms: raw.cb_recovery_timeout_ms,
            cb_half_open_max: raw.cb_half_open_max,
            ..Self::default()
        }
    }

    /// 从 pm-models 的 ExecutionConfig 桥接构建（兼容旧配置）。
    pub fn from_exec_config(cfg: &pm_models::config::ExecutionConfig) -> Self {
        Self {
            gateway_type: cfg.gateway.clone(),
            ..Self::default()
        }
    }

    /// 是否为 DryRun 模式（安全默认）。
    pub fn is_dry_run(&self) -> bool {
        !self.enable_live
    }

    /// 转换为 pm-api-test 的 ApiTestConfig（P2-03 bridge）。
    ///
    /// 使得 PolymarketGateway 可以复用 P2-01 已验证的 ApiClient。
    pub fn to_api_test_config(&self) -> pm_api_test::client::config::ApiTestConfig {
        use pm_api_test::client::config::ClientMode;

        pm_api_test::client::config::ApiTestConfig {
            clob_url: self.polymarket_api_url.clone(),
            ws_url: self.polymarket_ws_url.clone(),
            timeout_ms: self.api_timeout_ms,
            max_retries: self.max_retries,
            retry_base_ms: self.retry_base_ms,
            retry_max_ms: self.retry_max_ms,
            backoff_multiplier: self.backoff_multiplier,
            rate_limit_per_sec: self.rate_limit_per_sec,
            mode: ClientMode::Mock, // 默认 Mock，live 时切换
            enable_live: self.enable_live,
            ..pm_api_test::client::config::ApiTestConfig::default()
        }
    }

    /// 安全摘要（中文）。
    pub fn safety_summary_zh(&self) -> String {
        if self.enable_live {
            "⚠️ 真实交易模式已启用！订单将提交到真实交易所。".to_string()
        } else {
            "🔒 DryRun 模式 — 所有下单请求将被拒绝。".to_string()
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "【Gateway 配置】\n\
             Gateway 类型: {}\n\
             {}\n\
             API URL: {}\n\
             WS URL: {}\n\
             最大重试: {} | 退避乘数: {}x\n\
             断路器阈值: {} 次 | 恢复超时: {}ms\n\
             速率限制: {}/s | {}/min\n\
             健康检查间隔: {}s | API 超时: {}ms\n\
             同步间隔: 订单{}s / 余额{}s / 持仓{}s",
            self.gateway_type,
            self.safety_summary_zh(),
            self.polymarket_api_url,
            self.polymarket_ws_url,
            self.max_retries,
            self.backoff_multiplier,
            self.cb_failure_threshold,
            self.cb_recovery_timeout_ms,
            self.rate_limit_per_sec,
            self.rate_limit_per_min,
            self.health_check_interval_secs,
            self.api_timeout_ms,
            self.order_sync_interval_secs,
            self.balance_sync_interval_secs,
            self.position_sync_interval_secs,
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
    fn default_config_is_dry_run() {
        let cfg = GatewayConfig::default();
        assert!(cfg.is_dry_run());
        assert!(!cfg.enable_live);
        assert_eq!(cfg.gateway_type, "mock");
    }

    #[test]
    fn enable_live_allows_trading() {
        let cfg = GatewayConfig {
            enable_live: true,
            ..GatewayConfig::default()
        };
        assert!(!cfg.is_dry_run());
        assert!(cfg.safety_summary_zh().contains("真实交易"));
    }

    #[test]
    fn dry_run_summary_mentions_safety() {
        let cfg = GatewayConfig::default();
        let summary = cfg.safety_summary_zh();
        assert!(summary.contains("DryRun"));
        assert!(summary.contains("拒绝"));
    }

    #[test]
    fn bridge_from_exec_config() {
        let exec_cfg = pm_models::config::ExecutionConfig {
            gateway: "polymarket".into(),
            ..Default::default()
        };
        let gw_cfg = GatewayConfig::from_exec_config(&exec_cfg);
        assert_eq!(gw_cfg.gateway_type, "polymarket");
        assert!(gw_cfg.is_dry_run()); // 仍然默认 DryRun
    }

    #[test]
    fn retry_config_has_expected_values() {
        let cfg = GatewayConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.retry_base_ms, 500);
        assert_eq!(cfg.retry_max_ms, 15000);
        assert!((cfg.backoff_multiplier - 2.0).abs() < 1e-9);
    }

    #[test]
    fn circuit_breaker_config() {
        let cfg = GatewayConfig::default();
        assert_eq!(cfg.cb_failure_threshold, 5);
        assert_eq!(cfg.cb_recovery_timeout_ms, 30000);
        assert_eq!(cfg.cb_half_open_max, 3);
    }
}
