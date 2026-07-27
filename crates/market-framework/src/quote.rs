//! Shared instrument identity and executable order-book quotes.
//!
//! These types form the boundary between order-book adapters and strategies. A
//! strategy must not depend on a Binance symbol or prediction-market token id;
//! adapters normalize them into this model first. DEX pools use `DexPoolQuote`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Where a quote originates.
///
/// The kind is deliberately not part of instrument identity: BTC/USDT remains
/// the same product everywhere. Strategy domains must still enforce their own
/// venue-kind boundary; identity equality does not imply execution equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueKind {
    Cex,
    Dex,
    Prediction,
    Broker,
    Simulated,
}

/// Product semantics that must match before two prices are comparable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Spot,
    Perpetual,
    Future { expiry: String },
    Prediction { outcome: String },
    Other(String),
}

/// Stable, venue-independent identity used to join quotes across markets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalInstrument {
    pub base: String,
    pub quote: String,
    pub product: ProductKind,
}

impl CanonicalInstrument {
    pub fn spot(base: impl Into<String>, quote: impl Into<String>) -> Self {
        Self {
            base: base.into().to_uppercase(),
            quote: quote.into().to_uppercase(),
            product: ProductKind::Spot,
        }
    }

    pub fn canonical_id(&self) -> String {
        let product = match &self.product {
            ProductKind::Spot => "spot".to_string(),
            ProductKind::Perpetual => "perp".to_string(),
            ProductKind::Future { expiry } => format!("future:{expiry}"),
            ProductKind::Prediction { outcome } => format!("prediction:{outcome}"),
            ProductKind::Other(value) => value.clone(),
        };
        format!("{}:{}:{}", product, self.base, self.quote)
    }
}

impl fmt::Display for CanonicalInstrument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical_id())
    }
}

/// Executable best bid/ask after a venue adapter has normalized units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueQuote {
    /// Router key, for example `binance`, `okx` or `polymarket`.
    pub venue: String,
    pub venue_kind: VenueKind,
    pub instrument: CanonicalInstrument,
    /// Native market/pool id passed back to the venue when placing an order.
    pub market_id: String,
    pub bid: f64,
    pub ask: f64,
    pub bid_size: f64,
    pub ask_size: f64,
    /// Expected taker fee in basis points.
    pub taker_fee_bps: f64,
    /// Fixed transaction/network cost in quote currency for this leg.
    pub fixed_cost: f64,
    /// Unix epoch milliseconds assigned at ingestion time.
    pub timestamp_ms: i64,
}

impl VenueQuote {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.venue_kind == VenueKind::Dex {
            return Err("DEX liquidity must use DexPoolQuote");
        }
        if self.venue.trim().is_empty() || self.market_id.trim().is_empty() {
            return Err("venue and market_id are required");
        }
        if !self.bid.is_finite() || !self.ask.is_finite() || self.bid <= 0.0 || self.ask <= 0.0 {
            return Err("bid and ask must be finite and positive");
        }
        if self.bid > self.ask {
            return Err("bid must not exceed ask");
        }
        if self.bid_size < 0.0 || self.ask_size < 0.0 {
            return Err("sizes must not be negative");
        }
        if !self.taker_fee_bps.is_finite() || self.taker_fee_bps < 0.0 {
            return Err("fee must be finite and non-negative");
        }
        if !self.fixed_cost.is_finite() || self.fixed_cost < 0.0 {
            return Err("fixed cost must be finite and non-negative");
        }
        Ok(())
    }

    pub fn is_fresh(&self, now_ms: i64, max_age_ms: i64) -> bool {
        now_ms >= self.timestamp_ms && now_ms - self.timestamp_ms <= max_age_ms
    }
}
