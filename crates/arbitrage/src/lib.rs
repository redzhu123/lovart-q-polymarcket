//! Independent arbitrage domains for centralized and decentralized markets.
//!
//! CEX strategies consume order-book quotes. DEX strategies consume executable
//! AMM quotes. The public API intentionally provides no detector that accepts
//! both types, preventing accidental CEX-to-DEX opportunity construction.

pub mod cex;
pub mod cross_chain;
pub mod dex;
pub mod dex_router;
pub mod dex_v2;
pub mod multi_chain;

pub use cex::{CexArbitrageConfig, CexArbitrageDetector, CexArbitrageOpportunity, CexLeg};
pub use dex::{DexArbitrageConfig, DexArbitrageDetector, DexArbitrageOpportunity, DexLeg};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeAction {
    Buy,
    Sell,
}
