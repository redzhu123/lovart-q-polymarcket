//! 配置模型：从 `config.toml` 读取全部可调参数。
//!
//! 不读环境变量（V1.0 约定）。每个字段带 `#[serde(default)]`，缺字段时用代码默认值，
//! 因此即使 `config.toml` 不存在或缺段也能退化运行（[`Config::default`]）。
//! 真实读取见 [`Config::load`]。

use anyhow::{Context, Result};
use serde::Deserialize;

// ============================================================================
// 日志级别（V1.01 可观测性）
// ============================================================================

/// 日志级别：控制控制台输出详尽程度（V1.01 第九节）。
///
/// 顺序（`Ord`）：`Error < Warn < Info < Debug < Trace`。
/// 一条级别为 `L` 的日志在 `effective >= L` 时输出。
/// - `Error`：仅错误
/// - `Warn`：+ 警告
/// - `Info`：+ 统计 / 仪表盘 / System Summary / Pipeline Timeline
/// - `Debug`：+ Scanner 调试块（HTTP 逐页 / 过滤明细 / 市场样本 / JSON 诊断）-- 默认
/// - `Trace`：+ 全量 Market 字段转储
///
/// 不写死：由 `config.toml` 的 `[logging].log_level` 控制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Debug
    }
}

impl LogLevel {
    /// 从字符串解析（不区分大小写）。未知值返回 `None`，由调用方决定回退。
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ERROR" => Some(LogLevel::Error),
            "WARN" => Some(LogLevel::Warn),
            "INFO" => Some(LogLevel::Info),
            "DEBUG" => Some(LogLevel::Debug),
            "TRACE" => Some(LogLevel::Trace),
            _ => None,
        }
    }

    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            LogLevel::Error => "错误",
            LogLevel::Warn => "警告",
            LogLevel::Info => "信息",
            LogLevel::Debug => "调试",
            LogLevel::Trace => "跟踪",
        }
    }

    /// 是否输出某级别（`self` 为生效级别，`level` 为日志级别）。
    pub fn shows(&self, level: LogLevel) -> bool {
        level <= *self
    }
}

/// 日志配置（V1.01 第九节）。
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// 日志级别。`#[serde(default)]` -> `LogLevel::Debug`（spec 默认）。
    /// 支持自定义反序列化：既接受 `"DEBUG"` 字符串，也兼容已解析枚举。
    #[serde(default, deserialize_with = "deserialize_log_level")]
    pub log_level: LogLevel,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Debug,
        }
    }
}

/// 自定义反序列化：字符串 -> `LogLevel`，未知值回退到 `Debug`（不阻断启动）。
fn deserialize_log_level<'de, D>(d: D) -> Result<LogLevel, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    Ok(LogLevel::from_str(&s).unwrap_or(LogLevel::Debug))
}

/// 数据源配置（V1.02 第十二节）。
///
/// 控制使用哪个 Provider 以及内存缓存 TTL。缺省段 -> gamma / 10 秒。
#[derive(Debug, Clone, Deserialize)]
pub struct DataSourceConfig {
    /// Provider 选择：`gamma` / `clob` / `mock`。缺省 `gamma`。
    #[serde(default = "default_provider")]
    pub provider: String,
    /// 内存缓存 TTL（秒）。缺省 10。
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: u64,
}

fn default_provider() -> String {
    "gamma".into()
}
fn default_cache_ttl() -> u64 {
    10
}

impl Default for DataSourceConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            cache_ttl: default_cache_ttl(),
        }
    }
}

/// 扫描器配置。
#[derive(Debug, Clone, Deserialize)]
pub struct ScannerConfig {
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    #[serde(default = "default_opportunity_threshold")]
    pub opportunity_threshold: f64,
    /// 调试开关：true 输出完整数据流统计/HTTP/JSON/价格/策略调试信息；
    /// false 恢复简洁输出（V1.0 行为）。不写死，由 config.toml 控制。
    #[serde(default)]
    pub debug: bool,
}

fn default_scan_interval() -> u64 {
    10
}
fn default_opportunity_threshold() -> f64 {
    0.99
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: default_scan_interval(),
            opportunity_threshold: default_opportunity_threshold(),
            debug: false,
        }
    }
}

/// 组合配置（Paper Trading 用）。
#[derive(Debug, Clone, Deserialize)]
pub struct PortfolioConfig {
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    #[serde(default = "default_max_positions")]
    pub max_positions: usize,
    #[serde(default = "default_max_position_size")]
    pub max_position_size: f64,
}

fn default_initial_capital() -> f64 {
    10000.0
}
fn default_max_positions() -> usize {
    10
}
fn default_max_position_size() -> f64 {
    100.0
}

impl Default for PortfolioConfig {
    fn default() -> Self {
        Self {
            initial_capital: default_initial_capital(),
            max_positions: default_max_positions(),
            max_position_size: default_max_position_size(),
        }
    }
}

/// Execution Simulator 配置（V1.06 扩展）。
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionConfig {
    // ---- V0.9 原有字段 ----
    #[serde(default = "default_exec_capital")]
    pub capital: f64,
    #[serde(default = "default_max_pending")]
    pub max_pending_orders: usize,
    #[serde(default = "default_order_notional")]
    pub order_notional: f64,
    #[serde(default = "default_max_fill_delay")]
    pub max_fill_delay: u32,
    #[serde(default = "default_max_wait_scans")]
    pub max_wait_scans: u32,

    // ---- V1.06 新增字段 ----
    /// 最大队列长度。
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    /// 最大重试次数。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 重试延迟（毫秒）。
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// 订单超时（毫秒）。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 每秒最大订单数（速率限制）。
    #[serde(default = "default_max_orders_per_second")]
    pub max_orders_per_second: u32,
    /// 突发容量。
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
    /// Gateway 类型："mock" | "polymarket" | "kalshi"。
    #[serde(default = "default_exec_gateway")]
    pub gateway: String,
    /// 事件 CSV 路径。
    #[serde(default = "default_exec_events_csv")]
    pub events_csv: String,
    /// 报告 CSV 路径。
    #[serde(default = "default_exec_report_csv")]
    pub report_csv: String,
}

fn default_exec_capital() -> f64 {
    10000.0
}
fn default_max_pending() -> usize {
    20
}
fn default_order_notional() -> f64 {
    100.0
}
fn default_max_fill_delay() -> u32 {
    3
}
fn default_max_wait_scans() -> u32 {
    5
}
fn default_max_queue_size() -> usize {
    1000
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay_ms() -> u64 {
    1000
}
fn default_timeout_ms() -> u64 {
    30000
}
fn default_max_orders_per_second() -> u32 {
    10
}
fn default_burst_size() -> u32 {
    5
}
fn default_exec_gateway() -> String {
    "mock".into()
}
fn default_exec_events_csv() -> String {
    "data/execution_events.csv".into()
}
fn default_exec_report_csv() -> String {
    "data/execution_report.csv".into()
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            capital: default_exec_capital(),
            max_pending_orders: default_max_pending(),
            order_notional: default_order_notional(),
            max_fill_delay: default_max_fill_delay(),
            max_wait_scans: default_max_wait_scans(),
            max_queue_size: default_max_queue_size(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            timeout_ms: default_timeout_ms(),
            max_orders_per_second: default_max_orders_per_second(),
            burst_size: default_burst_size(),
            gateway: default_exec_gateway(),
            events_csv: default_exec_events_csv(),
            report_csv: default_exec_report_csv(),
        }
    }
}

/// 风控配置（V1.05 扩展）。
#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    // ---- 仓位规模 ----
    #[serde(default)]
    pub position_sizer: String,
    #[serde(default = "default_fixed_size")]
    pub fixed_size: f64,
    #[serde(default = "default_risk_ratio")]
    pub risk_ratio: f64,

    // ---- 仓位限制 ----
    #[serde(default = "default_max_positions")]
    pub max_positions: usize,
    #[serde(default = "default_max_position_size")]
    pub max_position_size: f64,
    #[serde(default = "default_max_open_orders")]
    pub max_open_orders: usize,
    #[serde(default = "default_max_single_capital")]
    pub max_single_capital: f64,
    #[serde(default = "default_max_capital_usage")]
    pub max_capital_usage: f64,

    // ---- 亏损限制 ----
    #[serde(default = "default_max_daily_loss")]
    pub max_daily_loss: f64,
    #[serde(default = "default_max_consecutive_losses")]
    pub max_consecutive_losses: usize,
    #[serde(default = "default_max_drawdown")]
    pub max_drawdown: f64,

    // ---- 暴露限制 ----
    #[serde(default = "default_max_market_exposure")]
    pub max_market_exposure: f64,
    #[serde(default = "default_max_category_exposure")]
    pub max_category_exposure: f64,
    #[serde(default = "default_max_side_exposure")]
    pub max_side_exposure: f64,

    // ---- 市场质量 ----
    #[serde(default = "default_min_liquidity")]
    pub min_liquidity: f64,
    #[serde(default = "default_min_depth")]
    pub min_depth: f64,
    #[serde(default = "default_max_slippage")]
    pub max_slippage: f64,
    #[serde(default = "default_max_volatility")]
    pub max_volatility: f64,

    // ---- 评分阈值 ----
    #[serde(default = "default_accept_threshold")]
    pub accept_threshold: f64,
    #[serde(default = "default_review_threshold")]
    pub review_threshold: f64,

    // ---- CSV ----
    #[serde(default = "default_risk_events_csv")]
    pub risk_events_csv: String,
    #[serde(default = "default_risk_dashboard_csv")]
    pub risk_dashboard_csv: String,
}

fn default_position_sizer() -> String {
    "Fixed".into()
}
fn default_fixed_size() -> f64 {
    100.0
}
fn default_risk_ratio() -> f64 {
    0.01
}
fn default_max_daily_loss() -> f64 {
    1000.0
}
fn default_max_open_orders() -> usize {
    20
}
fn default_max_single_capital() -> f64 {
    500.0
}
fn default_max_capital_usage() -> f64 {
    0.5
}
fn default_max_consecutive_losses() -> usize {
    5
}
fn default_max_drawdown() -> f64 {
    0.2
}
fn default_max_market_exposure() -> f64 {
    0.3
}
fn default_max_category_exposure() -> f64 {
    0.5
}
fn default_max_side_exposure() -> f64 {
    0.6
}
fn default_min_liquidity() -> f64 {
    100.0
}
fn default_min_depth() -> f64 {
    50.0
}
fn default_max_slippage() -> f64 {
    0.02
}
fn default_max_volatility() -> f64 {
    0.5
}
fn default_accept_threshold() -> f64 {
    70.0
}
fn default_review_threshold() -> f64 {
    40.0
}
fn default_risk_events_csv() -> String {
    "data/risk_events.csv".into()
}
fn default_risk_dashboard_csv() -> String {
    "data/risk_dashboard.csv".into()
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            position_sizer: default_position_sizer(),
            fixed_size: default_fixed_size(),
            risk_ratio: default_risk_ratio(),
            max_positions: default_max_positions(),
            max_position_size: default_max_position_size(),
            max_open_orders: default_max_open_orders(),
            max_single_capital: default_max_single_capital(),
            max_capital_usage: default_max_capital_usage(),
            max_daily_loss: default_max_daily_loss(),
            max_consecutive_losses: default_max_consecutive_losses(),
            max_drawdown: default_max_drawdown(),
            max_market_exposure: default_max_market_exposure(),
            max_category_exposure: default_max_category_exposure(),
            max_side_exposure: default_max_side_exposure(),
            min_liquidity: default_min_liquidity(),
            min_depth: default_min_depth(),
            max_slippage: default_max_slippage(),
            max_volatility: default_max_volatility(),
            accept_threshold: default_accept_threshold(),
            review_threshold: default_review_threshold(),
            risk_events_csv: default_risk_events_csv(),
            risk_dashboard_csv: default_risk_dashboard_csv(),
        }
    }
}

/// CSV 路径配置。
#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_opportunities_csv")]
    pub opportunities_csv: String,
    #[serde(default = "default_shadow_csv")]
    pub shadow_csv: String,
    #[serde(default = "default_paper_orders_csv")]
    pub paper_orders_csv: String,
    #[serde(default = "default_paper_positions_csv")]
    pub paper_positions_csv: String,
    #[serde(default = "default_paper_portfolio_csv")]
    pub paper_portfolio_csv: String,
    #[serde(default = "default_execution_csv")]
    pub execution_csv: String,
    #[serde(default = "default_backtest_report_csv")]
    pub backtest_report_csv: String,
}

fn default_data_dir() -> String {
    "data".into()
}
fn default_opportunities_csv() -> String {
    "data/opportunities.csv".into()
}
fn default_shadow_csv() -> String {
    "data/shadow_trades.csv".into()
}
fn default_paper_orders_csv() -> String {
    "data/paper_orders.csv".into()
}
fn default_paper_positions_csv() -> String {
    "data/paper_positions.csv".into()
}
fn default_paper_portfolio_csv() -> String {
    "data/paper_portfolio.csv".into()
}
fn default_execution_csv() -> String {
    "data/execution_orders.csv".into()
}
fn default_backtest_report_csv() -> String {
    "data/backtest_report.csv".into()
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            opportunities_csv: default_opportunities_csv(),
            shadow_csv: default_shadow_csv(),
            paper_orders_csv: default_paper_orders_csv(),
            paper_positions_csv: default_paper_positions_csv(),
            paper_portfolio_csv: default_paper_portfolio_csv(),
            execution_csv: default_execution_csv(),
            backtest_report_csv: default_backtest_report_csv(),
        }
    }
}

/// 回放配置。
#[derive(Debug, Clone, Deserialize)]
pub struct ReplayConfig {
    #[serde(default = "default_replay_speed")]
    pub speed: u32,
    #[serde(default = "default_replay_step")]
    pub step_secs: u64,
}

fn default_replay_speed() -> u32 {
    10
}
fn default_replay_step() -> u64 {
    10
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            speed: default_replay_speed(),
            step_secs: default_replay_step(),
        }
    }
}

/// 回测配置。
#[derive(Debug, Clone, Deserialize)]
pub struct BacktestConfig {
    #[serde(default = "default_entry_slippage")]
    pub entry_slippage: f64,
    #[serde(default = "default_strategy_name")]
    pub strategy_name: String,
}

fn default_entry_slippage() -> f64 {
    0.005
}
fn default_strategy_name() -> String {
    "ShadowStrategy-v1.0".into()
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            entry_slippage: default_entry_slippage(),
            strategy_name: default_strategy_name(),
        }
    }
}

// Gateway 默认值（V1.08）— 必须在 struct 前定义。
fn default_retry_base_ms() -> u64 {
    500
}
fn default_retry_max_ms_val() -> u64 {
    15000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}
fn default_cb_failure_threshold() -> u32 {
    5
}
fn default_cb_recovery_timeout_ms() -> u64 {
    30000
}
fn default_cb_half_open_max() -> u32 {
    3
}

/// Gateway 原始配置（V1.08）-- 从 config.toml [gateway] 段读取。
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayRawConfig {
    /// Gateway 类型。
    #[serde(default = "default_gateway_type")]
    pub gateway_type: String,
    /// 是否启用真实交易。
    #[serde(default)]
    pub enable_live: bool,
    /// Polymarket API URL。
    #[serde(default = "default_polymarket_api_url")]
    pub polymarket_api_url: String,
    /// Polymarket WS URL。
    #[serde(default = "default_polymarket_ws_url")]
    pub polymarket_ws_url: String,
    /// API 密钥环境变量名。
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    /// API 私钥环境变量名。
    #[serde(default = "default_api_secret_env")]
    pub api_secret_env: String,
    /// API 口令环境变量名。
    #[serde(default = "default_api_passphrase_env")]
    pub api_passphrase_env: String,
    /// 最大重试次数。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 基础重试延迟。
    #[serde(default = "default_retry_base_ms")]
    pub retry_base_ms: u64,
    /// 最大重试延迟。
    #[serde(default = "default_retry_max_ms_val")]
    pub retry_max_ms: u64,
    /// 退避乘数。
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// 断路器失败阈值。
    #[serde(default = "default_cb_failure_threshold")]
    pub cb_failure_threshold: u32,
    /// 断路器恢复超时。
    #[serde(default = "default_cb_recovery_timeout_ms")]
    pub cb_recovery_timeout_ms: u64,
    /// 断路器半开最大请求数。
    #[serde(default = "default_cb_half_open_max")]
    pub cb_half_open_max: u32,
}

fn default_gateway_type() -> String {
    "mock".into()
}
fn default_polymarket_api_url() -> String {
    "https://clob.polymarket.com".into()
}
fn default_polymarket_ws_url() -> String {
    "wss://ws.polymarket.com".into()
}
fn default_api_key_env() -> String {
    "POLYMARKET_API_KEY".into()
}
fn default_api_secret_env() -> String {
    "POLYMARKET_API_SECRET".into()
}
fn default_api_passphrase_env() -> String {
    "POLYMARKET_API_PASSPHRASE".into()
}

impl Default for GatewayRawConfig {
    fn default() -> Self {
        Self {
            gateway_type: default_gateway_type(),
            enable_live: false,
            polymarket_api_url: default_polymarket_api_url(),
            polymarket_ws_url: default_polymarket_ws_url(),
            api_key_env: default_api_key_env(),
            api_secret_env: default_api_secret_env(),
            api_passphrase_env: default_api_passphrase_env(),
            max_retries: default_max_retries(),
            retry_base_ms: default_retry_base_ms(),
            retry_max_ms: default_retry_max_ms_val(),
            backoff_multiplier: default_backoff_multiplier(),
            cb_failure_threshold: default_cb_failure_threshold(),
            cb_recovery_timeout_ms: default_cb_recovery_timeout_ms(),
            cb_half_open_max: default_cb_half_open_max(),
        }
    }
}

/// 顶层配置。各子段缺省时退化为代码默认值。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub scanner: ScannerConfig,
    #[serde(default)]
    pub portfolio: PortfolioConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub replay: ReplayConfig,
    #[serde(default)]
    pub backtest: BacktestConfig,
    /// 日志配置（V1.01）。缺省段 -> `LogLevel::Debug`。
    #[serde(default)]
    pub logging: LoggingConfig,
    /// 数据源配置（V1.02）。缺省段 -> provider=gamma, cache_ttl=10。
    #[serde(default)]
    pub datasource: DataSourceConfig,
    /// Gateway 配置（V1.08）。缺省段 -> gateway_type=mock, enable_live=false。
    #[serde(default)]
    pub gateway: GatewayRawConfig,
}

impl Config {
    /// 从 `path` 读取并解析 `config.toml`。文件缺失或解析失败返回 Err。
    pub fn load(path: &str) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("读取配置文件失败: {}", path))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("解析配置文件失败: {}", path))?;
        Ok(cfg)
    }

    /// 尝试加载；失败时返回默认配置（不阻断启动，由调用方决定是否提示）。
    pub fn load_or_default(path: &str) -> Config {
        match Self::load(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("配置加载失败，使用默认配置: {:#}", e);
                Config::default()
            }
        }
    }

    /// 生效日志级别（V1.01 第九节）。
    ///
    /// 以 `[logging].log_level` 为权威（缺省 `Debug`，即 spec 默认）。
    /// 旧版 `[scanner].debug` 布尔字段保留以兼容旧配置解析，但**不再**用于门控输出。
    pub fn effective_log_level(&self) -> LogLevel {
        self.logging.log_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let c = Config::default();
        assert_eq!(c.scanner.scan_interval_secs, 10);
        assert!((c.scanner.opportunity_threshold - 0.99).abs() < 1e-9);
        assert!(!c.scanner.debug); // 缺省关闭 -> 简洁输出
        assert!((c.portfolio.initial_capital - 10000.0).abs() < 1e-9);
        assert_eq!(c.portfolio.max_positions, 10);
        assert_eq!(c.execution.max_pending_orders, 20);
        assert_eq!(c.execution.max_fill_delay, 3);
        assert_eq!(c.execution.max_wait_scans, 5);
        assert_eq!(c.replay.speed, 10);
        assert_eq!(c.backtest.strategy_name, "ShadowStrategy-v1.0");
    }

    #[test]
    fn parse_partial_config_uses_defaults() {
        // 仅给 scanner 段，其余应退化为默认
        let text = r#"
[scanner]
scan_interval_secs = 5
"#;
        let cfg: Config = toml::from_str(text).expect("parse");
        assert_eq!(cfg.scanner.scan_interval_secs, 5);
        // opportunity_threshold 缺省 -> 默认 0.99
        assert!((cfg.scanner.opportunity_threshold - 0.99).abs() < 1e-9);
        // portfolio 段缺省 -> 默认
        assert!((cfg.portfolio.initial_capital - 10000.0).abs() < 1e-9);
    }

    #[test]
    fn load_or_default_missing_file_returns_default() {
        let cfg = Config::load_or_default("definitely_does_not_exist.toml");
        assert_eq!(cfg.scanner.scan_interval_secs, 10);
    }

    #[test]
    fn log_level_from_str_case_insensitive() {
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("Warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("trace"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("nope"), None);
    }

    #[test]
    fn log_level_ordering_and_shows() {
        // Error < Warn < Info < Debug < Trace
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
        // Debug 生效：显示 Error..Debug，不显示 Trace
        let lvl = LogLevel::Debug;
        assert!(lvl.shows(LogLevel::Error));
        assert!(lvl.shows(LogLevel::Info));
        assert!(lvl.shows(LogLevel::Debug));
        assert!(!lvl.shows(LogLevel::Trace));
        // Error 生效：只显示 Error
        let lvl = LogLevel::Error;
        assert!(lvl.shows(LogLevel::Error));
        assert!(!lvl.shows(LogLevel::Warn));
    }

    #[test]
    fn log_level_default_is_debug() {
        assert_eq!(LogLevel::default(), LogLevel::Debug);
        assert_eq!(LoggingConfig::default().log_level, LogLevel::Debug);
    }

    #[test]
    fn parse_logging_section() {
        let text = r#"
[logging]
log_level = "WARN"
"#;
        let cfg: Config = toml::from_str(text).expect("parse");
        assert_eq!(cfg.effective_log_level(), LogLevel::Warn);
    }

    #[test]
    fn parse_logging_section_case_insensitive_and_fallback() {
        // 小写
        let cfg: Config = toml::from_str("[logging]\nlog_level=\"info\"").expect("parse");
        assert_eq!(cfg.effective_log_level(), LogLevel::Info);
        // 未知值 -> 回退 Debug（不阻断）
        let cfg: Config = toml::from_str("[logging]\nlog_level=\"VERBOSE\"").expect("parse");
        assert_eq!(cfg.effective_log_level(), LogLevel::Debug);
    }

    #[test]
    fn missing_logging_section_defaults_to_debug() {
        let cfg: Config = toml::from_str("[scanner]\nscan_interval_secs=5").expect("parse");
        assert_eq!(cfg.effective_log_level(), LogLevel::Debug);
    }

    #[test]
    fn datasource_defaults_are_gamma_and_10s() {
        let c = Config::default();
        assert_eq!(c.datasource.provider, "gamma");
        assert_eq!(c.datasource.cache_ttl, 10);
    }

    #[test]
    fn parse_datasource_section() {
        let text = r#"
[datasource]
provider = "mock"
cache_ttl = 30
"#;
        let cfg: Config = toml::from_str(text).expect("parse");
        assert_eq!(cfg.datasource.provider, "mock");
        assert_eq!(cfg.datasource.cache_ttl, 30);
    }

    #[test]
    fn missing_datasource_section_uses_defaults() {
        let cfg: Config = toml::from_str("[scanner]\nscan_interval_secs=5").expect("parse");
        assert_eq!(cfg.datasource.provider, "gamma");
        assert_eq!(cfg.datasource.cache_ttl, 10);
    }
}
