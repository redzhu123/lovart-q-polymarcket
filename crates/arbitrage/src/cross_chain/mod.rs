//! 跨链双边库存纸面套利。跨链桥接不视为原子执行腿。

pub mod bridge;
pub mod inventory;
pub mod paper;
pub mod path;

pub use bridge::{AcrossQuoteProvider, BridgeQuote, BridgeQuoteProvider, LiFiQuoteProvider};
pub use inventory::InventoryLedger;
pub use paper::{
    CrossChainMarketSnapshot, CrossChainPaperConfig, CrossChainPaperDetector,
    CrossChainPaperOpportunity,
};
pub use path::{
    CrossChainOptimalRoute, CrossChainPathConfig, CrossChainPathScanner, CrossChainStepQuote,
    SuperEdgeKind, SuperGraphNode,
};
