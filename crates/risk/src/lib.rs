//! pm-risk：V1.05 统一风险引擎（Risk Engine）。
//!
//! 架构：
//! ```text
//! Opportunity → Strategy → TradeSuggestion → RiskEngine → Execution
//!                                    ↑
//!                              RiskContext
//! ```
//!
//! Risk Engine 拥有最终否决权。所有交易必须经过 Risk Engine，禁止绕过。
//!
//! 模块：
//! - [`context`]：RiskContext（组合/现金/暴露/持仓/待处理订单/市场/机会/策略结果/快照）。
//! - [`rules`]：RiskRule trait + 内置规则（最大仓位/单日最大亏损/连续亏损/流动性/滑点等）。
//! - [`engine`]：RiskEngine 编排器（评估 TradeSuggestion → Accept/Review/Reject）。
//! - [`score`]：RiskScore（0~100）计算。
//! - [`position_sizer`]：PositionSizer trait（Fixed/Risk/Kelly/Volatility/Liquidity/Confidence）。
//! - [`exposure`]：Exposure 实时统计（YES/NO/Category/Provider/Market）。
//! - [`portfolio_risk`]：Portfolio Risk 实时统计（资金利用率/风险暴露/现金比例/最大回撤/连续亏损）。
//! - [`explain`]：Explain 中文解释（所有拒绝/审核理由）。
//! - [`events`]：RiskEvent + CSV 记录。
//! - [`dashboard`]：Risk Dashboard 渲染（CLI）。
//! - [`replay`]：Risk Replay 历史风险重算。
//! - [`config`]：RiskConfig（risk.toml 或 config.toml [risk] 段）。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不下单。

pub mod config;
pub mod context;
pub mod dashboard;
pub mod engine;
pub mod events;
pub mod explain;
pub mod exposure;
pub mod portfolio_risk;
pub mod position_sizer;
pub mod replay;
pub mod rules;
pub mod score;

pub use config::{PositionSizerKind, RiskConfig};
pub use context::RiskContext;
pub use dashboard::RiskDashboard;
pub use engine::{RiskDecision, RiskEngine, TradeSuggestion};
pub use events::{RiskEvent, RiskEventKind, RiskEventRecord};
pub use explain::RiskExplain;
pub use exposure::{Exposure, ExposureReport};
pub use portfolio_risk::{PortfolioRisk, PortfolioRiskReport, RiskLevel};
pub use position_sizer::{PositionSizer, SizeRecommendation};
pub use replay::RiskReplay;
pub use rules::{
    ConsecutiveLossRule, DailyLossRule, DrawdownRule, ExposureLimitRule, LiquidityRule,
    MaxOrderCountRule, MaxPositionCountRule, MaxSingleCapitalRule, PositionSizeLimitRule, RiskRule,
    SlippageRule, VolatilityRule,
};
pub use score::RiskScore;
