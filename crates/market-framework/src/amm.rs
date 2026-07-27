//! DEX and AMM-specific market data.
//!
//! AMM liquidity is not an order book. Providers expose pool state for replay
//! and produce executable quotes for a concrete quantity before a strategy
//! evaluates an opportunity.

use serde::{Deserialize, Serialize};

use crate::quote::CanonicalInstrument;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmmState {
    ConstantProduct {
        reserve_base: f64,
        reserve_quote: f64,
    },
    ConcentratedLiquidity {
        sqrt_price: f64,
        active_liquidity: f64,
        tick: i32,
    },
    StableSwap {
        balances: Vec<f64>,
        amplification: f64,
    },
    ProtocolSpecific {
        protocol: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmmPoolState {
    pub venue: String,
    pub chain_id: u64,
    pub pool_id: String,
    pub instrument: CanonicalInstrument,
    pub fee_bps: f64,
    pub block_number: u64,
    pub timestamp_ms: i64,
    pub state: AmmState,
}

/// Executable prices for buying or selling one concrete base quantity.
///
/// LP fees and price impact are included in `buy_cost` and `sell_proceeds`.
/// Network cost remains explicit because it is paid independently per swap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DexPoolQuote {
    pub venue: String,
    pub chain_id: u64,
    pub pool_id: String,
    pub instrument: CanonicalInstrument,
    pub base_quantity: f64,
    pub buy_cost: f64,
    pub sell_proceeds: f64,
    pub gas_cost_quote: f64,
    pub block_number: u64,
    pub timestamp_ms: i64,
}

impl DexPoolQuote {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.venue.trim().is_empty() || self.pool_id.trim().is_empty() {
            return Err("venue and pool_id are required");
        }
        if self.chain_id == 0 {
            return Err("chain_id must be non-zero");
        }
        if !self.base_quantity.is_finite() || self.base_quantity <= 0.0 {
            return Err("base quantity must be finite and positive");
        }
        if !self.buy_cost.is_finite() || self.buy_cost <= 0.0 {
            return Err("buy cost must be finite and positive");
        }
        if !self.sell_proceeds.is_finite() || self.sell_proceeds <= 0.0 {
            return Err("sell proceeds must be finite and positive");
        }
        if !self.gas_cost_quote.is_finite() || self.gas_cost_quote < 0.0 {
            return Err("gas cost must be finite and non-negative");
        }
        Ok(())
    }

    pub fn is_fresh(&self, now_ms: i64, max_age_ms: i64) -> bool {
        max_age_ms >= 0 && now_ms >= self.timestamp_ms && now_ms - self.timestamp_ms <= max_age_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_executable_pool_quote() {
        let quote = DexPoolQuote {
            venue: "uniswap-v3".into(),
            chain_id: 1,
            pool_id: "0xpool".into(),
            instrument: CanonicalInstrument::spot("ETH", "USDC"),
            base_quantity: 1.0,
            buy_cost: 2_000.0,
            sell_proceeds: 1_995.0,
            gas_cost_quote: 2.0,
            block_number: 20_000_000,
            timestamp_ms: 1_000,
        };

        assert!(quote.validate().is_ok());
        assert!(quote.is_fresh(1_500, 1_000));
        assert!(!quote.is_fresh(2_001, 1_000));
    }
}
