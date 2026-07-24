//! pm-opportunity：套利机会引擎（V1.04 Opportunity Engine）。
//!
//! 本 crate 是量化平台的核心中间层，负责：
//! ```text
//! UnifiedMarket + OrderBook → Opportunity（评分 / 分类 / 过滤 / 排序）
//! ```
//!
//! Strategy 永远只处理 Opportunity，不直接分析 Market。
//!
//! 模块：
//! - [`model`]：`Opportunity` 数据模型 + `OpportunityType` / `OpportunityStatus` 枚举。
//! - [`score`]：统一评分引擎（0~100 综合评分）。
//! - [`confidence`]：置信度引擎（数据完整度 / 盘口 / 流动性 / 历史一致性）。
//! - [`engine`]：`OpportunityEngine` 编排器（入口）。
//! - [`queue`]：优先队列（按 Score 排序，支持 Top-N）。
//! - [`lifecycle`]：生命周期状态机（Created → Updated → Stable → Weak → Expired → Removed）。
//! - [`tracker`]：每个 Opportunity 的历史追踪。
//! - [`filter`]：统一过滤器（流动性 / 深度 / 成交量 / Score / 置信度 / 风险）。
//! - [`explain`]：解释引擎（为什么 Score=91）。
//! - [`statistics`]：统计累加器。
//! - [`storage`]：CSV 持久化（opportunity.csv）。
//!
//! 模拟研究专用 -- 不连接钱包 / 不真实交易 / 不签名 / 不下单。

pub mod confidence;
pub mod engine;
pub mod explain;
pub mod filter;
pub mod lifecycle;
pub mod model;
pub mod queue;
pub mod score;
pub mod statistics;
pub mod storage;
pub mod tracker;

// 重导出最常用的类型
pub use confidence::ConfidenceEngine;
pub use engine::{EngineConfig, EngineOutput, OpportunityEngine};
pub use explain::ExplainEngine;
pub use filter::{FilterConfig, FilterResult, OpportunityFilter};
pub use lifecycle::Lifecycle;
pub use model::{Opportunity, OpportunityId, OpportunityStatus, OpportunityType};
pub use queue::OpportunityQueue;
pub use score::{OpportunityScore, ScoreResult, ScoreWeights};
pub use statistics::OpportunityStatistics;
pub use storage::{
    OpportunityRecord, append_opportunities, ensure_opportunity_csv, load_opportunities,
};
pub use tracker::{HistoryTracker, OpportunityHistory};
