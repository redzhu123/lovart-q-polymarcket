//! Risk 配置（V1.05 第十二节）。
//!
//! 所有风险参数从配置读取，禁止写死。

use serde::Deserialize;

/// 仓位规模策略（V1.05 第四节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PositionSizerKind {
    /// 固定金额
    Fixed,
    /// 固定风险比例
    FixedRisk,
    /// Kelly 公式
    Kelly,
    /// 基于波动率
    Volatility,
    /// 基于流动性
    Liquidity,
    /// 基于置信度
    Confidence,
}

impl Default for PositionSizerKind {
    fn default() -> Self {
        Self::Fixed
    }
}

impl PositionSizerKind {
    pub fn as_zh(&self) -> &'static str {
        match self {
            PositionSizerKind::Fixed => "固定金额",
            PositionSizerKind::FixedRisk => "固定风险",
            PositionSizerKind::Kelly => "Kelly公式",
            PositionSizerKind::Volatility => "波动率",
            PositionSizerKind::Liquidity => "流动性",
            PositionSizerKind::Confidence => "置信度",
        }
    }
}

/// 风险规则配置。
#[derive(Debug, Clone, Deserialize)]
pub struct RiskRuleConfig {
    /// 是否启用此规则。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Risk Engine 完整配置（V1.05 第十二节）。
///
/// 支持从 risk.toml 独立加载，也可嵌入 config.toml 的 `[risk]` 段。
#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    // ---- 仓位规模 ----
    /// 仓位规模策略。
    #[serde(default)]
    pub position_sizer: PositionSizerKind,
    /// 固定仓位金额（USDC，Fixed 策略）。
    #[serde(default = "default_fixed_size")]
    pub fixed_size: f64,
    /// 固定风险比例（FixedRisk 策略，如 0.01 = 1%）。
    #[serde(default = "default_risk_ratio")]
    pub risk_ratio: f64,

    // ---- 仓位限制 ----
    /// 最大同时持仓数。
    #[serde(default = "default_max_positions")]
    pub max_positions: usize,
    /// 单笔持仓最大成本（USDC）。
    #[serde(default = "default_max_position_size")]
    pub max_position_size: f64,
    /// 最大待处理订单数。
    #[serde(default = "default_max_open_orders")]
    pub max_open_orders: usize,
    /// 最大单笔资金占用（USDC）。
    #[serde(default = "default_max_single_capital")]
    pub max_single_capital: f64,
    /// 最大总资金占用比例（0.0~1.0）。
    #[serde(default = "default_max_capital_usage")]
    pub max_capital_usage: f64,

    // ---- 亏损限制 ----
    /// 单日最大亏损（USDC）。
    #[serde(default = "default_max_daily_loss")]
    pub max_daily_loss: f64,
    /// 连续亏损次数上限（达到后暂停交易）。
    #[serde(default = "default_max_consecutive_losses")]
    pub max_consecutive_losses: usize,
    /// 最大回撤比例（0.0~1.0，如 0.2 = 20%）。
    #[serde(default = "default_max_drawdown")]
    pub max_drawdown: f64,

    // ---- 暴露限制 ----
    /// 单一市场最大暴露比例（0.0~1.0）。
    #[serde(default = "default_max_market_exposure")]
    pub max_market_exposure: f64,
    /// 单一类别最大暴露比例（0.0~1.0）。
    #[serde(default = "default_max_category_exposure")]
    pub max_category_exposure: f64,
    /// YES/NO 单边最大暴露比例（0.0~1.0）。
    #[serde(default = "default_max_side_exposure")]
    pub max_side_exposure: f64,

    // ---- 市场质量 ----
    /// 最低流动性要求（USDC）。
    #[serde(default = "default_min_liquidity")]
    pub min_liquidity: f64,
    /// 最低买卖深度（USDC）。
    #[serde(default = "default_min_depth")]
    pub min_depth: f64,
    /// 最大允许滑点（0.0~1.0）。
    #[serde(default = "default_max_slippage")]
    pub max_slippage: f64,
    /// 最高允许波动率（0.0~1.0）。
    #[serde(default = "default_max_volatility")]
    pub max_volatility: f64,

    // ---- 评分阈值 ----
    /// Risk Score 阈值：>= accept_threshold → Accept。
    #[serde(default = "default_accept_threshold")]
    pub accept_threshold: f64,
    /// Risk Score 阈值：< accept_threshold && >= review_threshold → Review。
    #[serde(default = "default_review_threshold")]
    pub review_threshold: f64,
    /// 低于此值为 Reject。

    // ---- CSV ----
    /// 风险事件 CSV 路径。
    #[serde(default = "default_risk_events_csv")]
    pub risk_events_csv: String,
    /// 风险仪表盘快照 CSV 路径。
    #[serde(default = "default_risk_dashboard_csv")]
    pub risk_dashboard_csv: String,
}

// ---- 默认值 ----
fn default_fixed_size() -> f64 {
    100.0
}
fn default_risk_ratio() -> f64 {
    0.01
}
fn default_max_positions() -> usize {
    10
}
fn default_max_position_size() -> f64 {
    100.0
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
fn default_max_daily_loss() -> f64 {
    1000.0
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
            position_sizer: PositionSizerKind::default(),
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

impl RiskConfig {
    /// 从 TOML 文件加载。
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("读取风险配置文件失败: {} -- {}", path, e))?;
        let cfg: Self = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("解析风险配置文件失败: {} -- {}", path, e))?;
        Ok(cfg)
    }

    /// 尝试加载；失败返回默认。
    pub fn load_or_default(path: &str) -> Self {
        Self::load(path).unwrap_or_else(|e| {
            tracing::warn!("风险配置加载失败，使用默认配置: {}", e);
            Self::default()
        })
    }

    /// 从 pm_models::Config 的 [risk] 段 + [portfolio] 段 + [execution] 段合并。
    pub fn from_pm_config(cfg: &pm_models::Config) -> Self {
        let position_sizer = match cfg.risk.position_sizer.as_str() {
            "FixedRisk" => PositionSizerKind::FixedRisk,
            "Kelly" => PositionSizerKind::Kelly,
            "Volatility" => PositionSizerKind::Volatility,
            "Liquidity" => PositionSizerKind::Liquidity,
            "Confidence" => PositionSizerKind::Confidence,
            _ => PositionSizerKind::Fixed,
        };
        Self {
            position_sizer,
            fixed_size: cfg.risk.fixed_size,
            risk_ratio: cfg.risk.risk_ratio,
            max_positions: cfg.portfolio.max_positions,
            max_position_size: cfg.portfolio.max_position_size,
            max_open_orders: cfg.execution.max_pending_orders,
            max_single_capital: cfg.risk.max_single_capital,
            max_capital_usage: cfg.risk.max_capital_usage,
            max_daily_loss: cfg.risk.max_daily_loss,
            max_consecutive_losses: cfg.risk.max_consecutive_losses,
            max_drawdown: cfg.risk.max_drawdown,
            max_market_exposure: cfg.risk.max_market_exposure,
            max_category_exposure: cfg.risk.max_category_exposure,
            max_side_exposure: cfg.risk.max_side_exposure,
            min_liquidity: cfg.risk.min_liquidity,
            min_depth: cfg.risk.min_depth,
            max_slippage: cfg.risk.max_slippage,
            max_volatility: cfg.risk.max_volatility,
            accept_threshold: cfg.risk.accept_threshold,
            review_threshold: cfg.risk.review_threshold,
            risk_events_csv: cfg.risk.risk_events_csv.clone(),
            risk_dashboard_csv: cfg.risk.risk_dashboard_csv.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let c = RiskConfig::default();
        assert_eq!(c.position_sizer, PositionSizerKind::Fixed);
        assert!((c.fixed_size - 100.0).abs() < 1e-9);
        assert_eq!(c.max_positions, 10);
        assert_eq!(c.max_consecutive_losses, 5);
        assert!((c.max_daily_loss - 1000.0).abs() < 1e-9);
        assert!((c.accept_threshold - 70.0).abs() < 1e-9);
        assert!((c.review_threshold - 40.0).abs() < 1e-9);
    }

    #[test]
    fn parse_partial_config_uses_defaults() {
        let text = r#"
max_positions = 5
max_daily_loss = 500.0
"#;
        let cfg: RiskConfig = toml::from_str(text).expect("parse");
        assert_eq!(cfg.max_positions, 5);
        assert!((cfg.max_daily_loss - 500.0).abs() < 1e-9);
        // 其余为默认
        assert_eq!(cfg.max_consecutive_losses, 5);
        assert!((cfg.accept_threshold - 70.0).abs() < 1e-9);
    }

    #[test]
    fn position_sizer_kind_zh() {
        assert_eq!(PositionSizerKind::Fixed.as_zh(), "固定金额");
        assert_eq!(PositionSizerKind::Kelly.as_zh(), "Kelly公式");
        assert_eq!(PositionSizerKind::Volatility.as_zh(), "波动率");
    }
}
