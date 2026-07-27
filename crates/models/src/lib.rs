//! pm-models：跨 crate 共享数据模型（DTO）。
//!
//! 仅放**行为轻量、被多个 crate 共享**的数据类型，按子模块组织：
//! - [`market`]：Gamma API 市场 `Market` 与单轮机会快照 `OppSnapshot`。
//! - [`opportunity`]：机会生命周期 `OpportunityState` / `TrackUpdate` / `FinishedOpportunity` / `ReplayOpportunity`。
//! - [`config`]：`Config` 及各子段，`Config::load` 读取 `config.toml`。
//! - [`datasource`]：V1.02 统一数据源 DTO -- `UnifiedMarket` / `OrderBook` / `ProviderCapability` 等。
//!
//! **不放**：带业务行为的 engine struct（归各 engine crate）与由 engine 类型转换的 CSV record
//! （否则 models->engine 而 engine->models 形成环）。此划分是"无环依赖 + 数据与行为同 crate"的必然结果。

pub mod config;
pub mod datasource;
pub mod market;
pub mod opportunity;

pub use config::{
    ArbitrageRawConfig, CexArbitrageRawConfig, Config, DataSourceConfig, DexArbitrageRawConfig,
    GatewayRawConfig, LogLevel, LoggingConfig,
};
pub use datasource::{
    MarketStatus, OrderBook, PriceLevel, PriceQuote, ProviderCapability, UnifiedMarket,
};
pub use market::{Market, OppSnapshot};
pub use opportunity::{FinishedOpportunity, OpportunityState, ReplayOpportunity, TrackUpdate};
