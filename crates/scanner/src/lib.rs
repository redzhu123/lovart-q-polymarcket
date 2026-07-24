//! pm-scanner：扫描子系统（库）。
//!
//! 模块：
//! - [`datasource`]：V1.02 统一数据源层 -- `MarketDataProvider` Trait + `GammaProvider` /
//!   `MockProvider` + `DataSourceManager`。Scanner 只依赖 Trait，不直接访问 HTTP。
//! - [`market`]：从 `UnifiedMarket` 列表识别潜在套利机会（HTTP 拉取已迁至 `datasource`）。
//! - [`driver`]：扫描循环 [`driver::run_scan`] -- 拉取 -> 跟踪 -> 调用 `Strategy` 各 hook ->
//!   写 CSV -> 更新 `Metrics`。被 `apps/scanner` 与 `apps/cli::scan` 共用，无 app->app 依赖。
//! - [`display`]：仪表盘与明细渲染。
//! - [`stats`]：V1.0.1 可观测性数据结构（HTTP/市场分析/累计统计）。
//! - [`pipeline`]：V1.01 统一模块计时与 Pipeline Timeline。
//! - [`health`]：V1.01 启动健康检查（Config/CSV/Storage/Clock/Memory/API/JSON）。
//! - [`diagnostics`]：V1.01 诊断模式 [`diagnostics::run_diagnose`]（单次扫描 + 完整诊断报告）。
//!
//! Simulation Only -- 套利判定沿用 SUM<阈值；Gamma outcomePrices 归一化（YES+NO≡1.0），
//! 常态下无新机会，真实套利需后续接入 CLOB Provider（届时只换数据源）。

pub mod datasource;
pub mod diagnostics;
pub mod display;
pub mod driver;
pub mod health;
pub mod market;
pub mod pipeline;
pub mod stats;

pub use datasource::run_datasource_diagnose;
pub use datasource::{
    ClobProvider, DataSourceManager, GammaProvider, MarketDataProvider, MockProvider,
};
pub use diagnostics::run_diagnose;
pub use driver::run_scan;
