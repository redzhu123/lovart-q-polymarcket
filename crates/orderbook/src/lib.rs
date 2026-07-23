//! pm-orderbook：订单簿分析引擎（V1.03 市场微观结构）。
//!
//! 本 crate 负责所有订单簿相关的分析计算，不涉及数据拉取（数据拉取由 pm-scanner 的
//! `MarketDataProvider` 系列负责）。Strategy 层可直接使用本 crate 的分析结果，
//! 无需修改数据层。
//!
//! 模块：
//! - [`validator`]：订单簿数据合法性校验（V1.03 第十一节）。
//! - [`spread`]：价差分析（V1.03 第四节）。
//! - [`liquidity`]：流动性分析（V1.03 第五节）。
//! - [`depth`]：深度分析（V1.03 第六节）。
//! - [`statistics`]：市场统计累加器（V1.03 第十节）。
//! - [`snapshot`]：订单簿快照（V1.03 第七节）-- Phase 3。
//! - [`visual`]：ASCII 可视化（V1.03 第十二节）-- Phase 3。
//!
//! 模拟研究专用 -- 不连接钱包 / 不真实交易 / 不签名 / 不下单。

pub mod depth;
pub mod liquidity;
pub mod snapshot;
pub mod spread;
pub mod statistics;
pub mod validator;
pub mod visual;

pub use depth::{DepthAnalyzer, DepthReport, DepthSummary};
pub use liquidity::{LiquidityAnalyzer, LiquidityReport};
pub use snapshot::OrderBookSnapshot;
pub use spread::{SpreadAnalyzer, SpreadReport, SpreadSummary};
pub use statistics::MarketStatistics;
pub use validator::OrderBookValidator;
pub use visual::OrderBookVisualizer;
