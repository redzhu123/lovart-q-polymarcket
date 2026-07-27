//! 同链 Uniswap V2 兼容 AMM 的有界两跳/三跳循环套利。

pub mod adapter;
pub mod config;
pub mod connector;
pub mod error;
pub mod execution;
pub mod gas;
pub mod graph;
pub mod metrics;
pub mod optimizer;
pub mod profit;
pub mod quoter;
pub mod repository;
pub mod risk;
pub mod runtime;
pub mod simulator;
pub mod state;
pub mod types;

pub use adapter::{PoolAdapter, UniswapV2Adapter};
pub use config::DexV2Config;
pub use connector::{ChainConnector, JsonRpcConnector, MockConnector};
pub use error::{DexV2Error, DexV2Result};
pub use execution::ExecutionRequestBuilder;
pub use gas::{GasEstimate, GasEstimateSource, GasEstimator, HopGasEstimator};
pub use graph::{
    BoundedRouteGenerator, PoolRegistry, RouteGenerationConfig, RouteGenerator, RouteIndex,
    TokenPoolGraph,
};
pub use optimizer::{AmountOptimizer, IntegerSearchOptimizer};
pub use profit::{FixedNativePriceOracle, NativePriceOracle, ProfitEngine, V2ProfitEngine};
pub use quoter::{LocalRouteQuoter, RouteQuoter};
pub use repository::{InMemoryOpportunityRepository, OpportunityRepository};
pub use risk::{DefaultRiskGuard, RejectionReason, RiskGuard};
pub use runtime::{DexV2Engine, RuntimeHandle};
pub use simulator::{EthCallSimulator, LocalShadowSimulator, SimulationEngine};
pub use state::PoolStateCache;
pub use types::*;
