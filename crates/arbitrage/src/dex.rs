use pm_market_framework::DexPoolQuote;
use serde::{Deserialize, Serialize};

use crate::TradeAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexArbitrageConfig {
    pub min_profit_bps: f64,
    pub min_profit_quote: f64,
    pub max_quote_age_ms: i64,
    pub quantity_tolerance: f64,
}

impl Default for DexArbitrageConfig {
    fn default() -> Self {
        Self {
            min_profit_bps: 10.0,
            min_profit_quote: 0.10,
            max_quote_age_ms: 12_000,
            quantity_tolerance: 1e-9,
        }
    }
}

impl From<&pm_models::config::ArbitrageRawConfig> for DexArbitrageConfig {
    fn from(raw: &pm_models::config::ArbitrageRawConfig) -> Self {
        Self {
            min_profit_bps: raw.dex.min_profit_bps,
            min_profit_quote: raw.dex.min_profit_quote,
            max_quote_age_ms: raw.dex.max_quote_age_ms,
            quantity_tolerance: raw.dex.quantity_tolerance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DexLeg {
    pub venue: String,
    pub chain_id: u64,
    pub pool_id: String,
    pub action: TradeAction,
    pub base_quantity: f64,
    pub quote_amount: f64,
    pub gas_cost_quote: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DexArbitrageOpportunity {
    pub id: String,
    pub instrument_id: String,
    pub buy: DexLeg,
    pub sell: DexLeg,
    pub estimated_profit: f64,
    pub estimated_profit_bps: f64,
}

pub struct DexArbitrageDetector {
    config: DexArbitrageConfig,
}

impl DexArbitrageDetector {
    pub fn new(config: DexArbitrageConfig) -> Self {
        Self { config }
    }

    pub fn detect(&self, quotes: &[DexPoolQuote], now_ms: i64) -> Vec<DexArbitrageOpportunity> {
        let valid = quotes
            .iter()
            .filter(|q| q.validate().is_ok() && q.is_fresh(now_ms, self.config.max_quote_age_ms))
            .collect::<Vec<_>>();
        let mut found = Vec::new();
        for buy in &valid {
            for sell in &valid {
                if same_pool(buy, sell)
                    || buy.instrument != sell.instrument
                    || !same_quantity(
                        buy.base_quantity,
                        sell.base_quantity,
                        self.config.quantity_tolerance,
                    )
                {
                    continue;
                }
                let profit =
                    sell.sell_proceeds - buy.buy_cost - buy.gas_cost_quote - sell.gas_cost_quote;
                let profit_bps = profit / buy.buy_cost * 10_000.0;
                if profit < self.config.min_profit_quote || profit_bps < self.config.min_profit_bps
                {
                    continue;
                }
                let instrument_id = buy.instrument.canonical_id();
                found.push(DexArbitrageOpportunity {
                    id: format!(
                        "dex-arb:{instrument_id}:{}:{}:{now_ms}",
                        buy.venue, sell.venue
                    ),
                    instrument_id,
                    buy: to_leg(buy, TradeAction::Buy, buy.buy_cost),
                    sell: to_leg(sell, TradeAction::Sell, sell.sell_proceeds),
                    estimated_profit: profit,
                    estimated_profit_bps: profit_bps,
                });
            }
        }
        found.sort_by(|a, b| b.estimated_profit.total_cmp(&a.estimated_profit));
        found
    }
}

fn same_pool(left: &DexPoolQuote, right: &DexPoolQuote) -> bool {
    left.chain_id == right.chain_id
        && left.venue.eq_ignore_ascii_case(&right.venue)
        && left.pool_id.eq_ignore_ascii_case(&right.pool_id)
}

fn same_quantity(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance * left.abs().max(right.abs()).max(1.0)
}

fn to_leg(q: &DexPoolQuote, action: TradeAction, quote_amount: f64) -> DexLeg {
    DexLeg {
        venue: q.venue.clone(),
        chain_id: q.chain_id,
        pool_id: q.pool_id.clone(),
        action,
        base_quantity: q.base_quantity,
        quote_amount,
        gas_cost_quote: q.gas_cost_quote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_market_framework::CanonicalInstrument;

    fn q(venue: &str, buy: f64, sell: f64, gas: f64, now: i64) -> DexPoolQuote {
        DexPoolQuote {
            venue: venue.into(),
            chain_id: 1,
            pool_id: venue.into(),
            instrument: CanonicalInstrument::spot("ETH", "USDC"),
            base_quantity: 1.0,
            buy_cost: buy,
            sell_proceeds: sell,
            gas_cost_quote: gas,
            block_number: 1,
            timestamp_ms: now,
        }
    }

    #[test]
    fn detects_dex_spread_after_gas() {
        let now = 1000;
        let detector = DexArbitrageDetector::new(DexArbitrageConfig::default());
        let found = detector.detect(
            &[
                q("uni", 2000.0, 1998.0, 1.0, now),
                q("curve", 2010.0, 2010.0, 1.0, now),
            ],
            now,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].estimated_profit, 8.0);
    }

    #[test]
    fn allows_distinct_pools_on_same_dex() {
        let now = 1000;
        let detector = DexArbitrageDetector::new(DexArbitrageConfig::default());
        let mut first = q("uni", 2000.0, 1998.0, 1.0, now);
        first.pool_id = "pool-a".into();
        let mut second = q("uni", 2010.0, 2010.0, 1.0, now);
        second.pool_id = "pool-b".into();

        assert_eq!(detector.detect(&[first, second], now).len(), 1);
    }
}
