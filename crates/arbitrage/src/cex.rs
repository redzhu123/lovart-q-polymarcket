use pm_market_framework::{VenueKind, VenueQuote};
use serde::{Deserialize, Serialize};

use crate::TradeAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CexArbitrageConfig {
    pub min_profit_bps: f64,
    pub min_profit_quote: f64,
    pub max_quote_age_ms: i64,
    pub slippage_buffer_bps: f64,
    pub max_notional: f64,
    pub min_quantity: f64,
}

impl Default for CexArbitrageConfig {
    fn default() -> Self {
        Self {
            min_profit_bps: 10.0,
            min_profit_quote: 0.10,
            max_quote_age_ms: 2_000,
            slippage_buffer_bps: 3.0,
            max_notional: 1_000.0,
            min_quantity: 0.000_001,
        }
    }
}

impl From<&pm_models::config::ArbitrageRawConfig> for CexArbitrageConfig {
    fn from(raw: &pm_models::config::ArbitrageRawConfig) -> Self {
        Self {
            min_profit_bps: raw.cex.min_profit_bps,
            min_profit_quote: raw.cex.min_profit_quote,
            max_quote_age_ms: raw.cex.max_quote_age_ms,
            slippage_buffer_bps: raw.cex.slippage_buffer_bps,
            max_notional: raw.cex.max_notional,
            min_quantity: raw.cex.min_quantity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CexLeg {
    pub venue: String,
    pub market_id: String,
    pub action: TradeAction,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CexArbitrageOpportunity {
    pub id: String,
    pub instrument_id: String,
    pub buy: CexLeg,
    pub sell: CexLeg,
    pub estimated_profit: f64,
    pub estimated_profit_bps: f64,
}

pub struct CexArbitrageDetector {
    config: CexArbitrageConfig,
}

impl CexArbitrageDetector {
    pub fn new(config: CexArbitrageConfig) -> Self {
        Self { config }
    }

    pub fn detect(&self, quotes: &[VenueQuote], now_ms: i64) -> Vec<CexArbitrageOpportunity> {
        let valid = quotes
            .iter()
            .filter(|q| {
                q.venue_kind == VenueKind::Cex
                    && q.validate().is_ok()
                    && q.is_fresh(now_ms, self.config.max_quote_age_ms)
            })
            .collect::<Vec<_>>();
        let mut found = Vec::new();
        for buy in &valid {
            for sell in &valid {
                if buy.venue.eq_ignore_ascii_case(&sell.venue)
                    || buy.instrument != sell.instrument
                    || sell.bid <= buy.ask
                {
                    continue;
                }
                let quantity = buy
                    .ask_size
                    .min(sell.bid_size)
                    .min(self.config.max_notional / buy.ask);
                if quantity < self.config.min_quantity {
                    continue;
                }
                let buy_value = buy.ask * quantity;
                let sell_value = sell.bid * quantity;
                let fees = buy_value * buy.taker_fee_bps / 10_000.0
                    + sell_value * sell.taker_fee_bps / 10_000.0;
                let impact = (buy_value + sell_value) * self.config.slippage_buffer_bps / 10_000.0;
                let profit =
                    sell_value - buy_value - fees - buy.fixed_cost - sell.fixed_cost - impact;
                let profit_bps = profit / buy_value * 10_000.0;
                if profit < self.config.min_profit_quote || profit_bps < self.config.min_profit_bps
                {
                    continue;
                }
                let instrument_id = buy.instrument.canonical_id();
                found.push(CexArbitrageOpportunity {
                    id: format!(
                        "cex-arb:{instrument_id}:{}:{}:{now_ms}",
                        buy.venue, sell.venue
                    ),
                    instrument_id,
                    buy: CexLeg {
                        venue: buy.venue.clone(),
                        market_id: buy.market_id.clone(),
                        action: TradeAction::Buy,
                        price: buy.ask,
                        quantity,
                    },
                    sell: CexLeg {
                        venue: sell.venue.clone(),
                        market_id: sell.market_id.clone(),
                        action: TradeAction::Sell,
                        price: sell.bid,
                        quantity,
                    },
                    estimated_profit: profit,
                    estimated_profit_bps: profit_bps,
                });
            }
        }
        found.sort_by(|a, b| b.estimated_profit.total_cmp(&a.estimated_profit));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_market_framework::{CanonicalInstrument, VenueKind};

    fn q(venue: &str, bid: f64, ask: f64, now: i64) -> VenueQuote {
        VenueQuote {
            venue: venue.into(),
            venue_kind: VenueKind::Cex,
            instrument: CanonicalInstrument::spot("BTC", "USDT"),
            market_id: venue.into(),
            bid,
            ask,
            bid_size: 10.0,
            ask_size: 10.0,
            taker_fee_bps: 2.0,
            fixed_cost: 0.0,
            timestamp_ms: now,
        }
    }

    #[test]
    fn detects_net_cex_spread() {
        let now = 1000;
        let detector = CexArbitrageDetector::new(CexArbitrageConfig::default());
        let found = detector.detect(&[q("a", 99.9, 100.0, now), q("b", 101.0, 101.1, now)], now);
        assert_eq!(found.len(), 1);
        assert!(found[0].estimated_profit > 0.0);
    }

    #[test]
    fn ignores_non_cex_quotes() {
        let now = 1000;
        let detector = CexArbitrageDetector::new(CexArbitrageConfig::default());
        let cex = q("cex", 99.9, 100.0, now);
        let mut dex = q("dex", 101.0, 101.1, now);
        dex.venue_kind = VenueKind::Dex;

        assert!(detector.detect(&[cex, dex], now).is_empty());
    }
}
