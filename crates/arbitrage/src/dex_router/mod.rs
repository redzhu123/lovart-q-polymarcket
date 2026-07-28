//! 多协议 DEX 路由、精确报价与有界循环发现。

pub mod aggregator;
pub mod curve;
pub mod graph;
pub mod quote;
pub mod types;
pub mod v3;

pub use aggregator::{RpcQuoteVerifier, ZeroXQuoteProvider};
pub use curve::{CurveGetDyQuoteProvider, CurvePoolBinding};
pub use graph::{BoundedCycleFinder, CycleSearchConfig, CycleSearchStats};
pub use quote::{ProtocolQuoteProvider, QuoteProviderRegistry};
pub use types::{
    ExactLegQuote, ExactRouteQuote, LiquidityEdge, MultiProtocolRoute, ProtocolKind, RouterError,
    RouterResult,
};
pub use v3::UniswapV3QuoterProvider;
