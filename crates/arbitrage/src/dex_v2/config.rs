use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use alloy_primitives::{Address, U256};
use serde::Deserialize;

use super::error::{DexV2Error, DexV2Result};
use super::graph::RouteGenerationConfig;
use super::types::{
    AmountBounds, ConfirmationMode, ExecutionMode, PoolId, Protocol, TokenId, TokenMeta, V2Pool,
};

#[derive(Debug, Clone, Deserialize)]
pub struct DexV2Config {
    #[serde(default)]
    pub enabled: bool,
    pub chain_id: u64,
    pub rpc_http_url: String,
    #[serde(default)]
    pub rpc_ws_url: Option<String>,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default)]
    pub confirmation_mode: ConfirmationMode,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_resync_blocks")]
    pub resync_interval_blocks: u64,
    #[serde(default)]
    pub log_query_delay_blocks: u64,
    #[serde(default = "default_workers")]
    pub worker_count: usize,
    #[serde(default = "default_queue")]
    pub queue_capacity: usize,
    #[serde(default = "default_age")]
    pub max_opportunity_age_blocks: u64,
    #[serde(default = "default_block_gap")]
    pub max_state_block_gap: u64,
    #[serde(default = "default_gas_units")]
    pub gas_units_fallback: u64,
    #[serde(default = "default_gas_price")]
    pub max_fee_per_gas: String,
    #[serde(default = "default_native_price")]
    pub native_price_anchor: String,
    #[serde(default = "default_min_profit")]
    pub min_net_profit_anchor: String,
    #[serde(default = "default_min_gross_profit")]
    pub min_gross_profit_anchor: String,
    #[serde(default = "default_max_gas")]
    pub max_gas_anchor: String,
    #[serde(default = "default_roi")]
    pub min_roi_bps: i64,
    #[serde(default = "default_risk_buffer")]
    pub risk_buffer_bps: u32,
    #[serde(default)]
    pub routes: RoutesConfig,
    pub optimizer: OptimizerConfig,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub gas: GasConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub tokens: Vec<TokenConfig>,
    #[serde(default)]
    pub pools: Vec<PoolConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoutesConfig {
    #[serde(default = "default_true")]
    pub enable_two_hop: bool,
    #[serde(default)]
    pub enable_three_hop: bool,
    #[serde(default = "default_max_hops")]
    pub max_route_hops: usize,
    #[serde(default = "default_max_routes")]
    pub max_routes_total: usize,
    #[serde(default = "default_max_routes_per_anchor")]
    pub max_routes_per_anchor: usize,
    #[serde(default = "default_max_edges")]
    pub max_edges_per_token_pair: usize,
    #[serde(default)]
    pub allowed_anchor_tokens: Vec<String>,
    #[serde(default)]
    pub allowed_intermediate_tokens: Vec<String>,
}

impl Default for RoutesConfig {
    fn default() -> Self {
        Self {
            enable_two_hop: true,
            enable_three_hop: false,
            max_route_hops: 2,
            max_routes_total: default_max_routes(),
            max_routes_per_anchor: default_max_routes_per_anchor(),
            max_edges_per_token_pair: default_max_edges(),
            allowed_anchor_tokens: Vec::new(),
            allowed_intermediate_tokens: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptimizerConfig {
    pub min_input: String,
    pub max_input: String,
    #[serde(default = "default_step")]
    pub minimum_search_step: String,
    #[serde(default = "default_reserve_bps")]
    pub max_pool_reserve_bps: u32,
    #[serde(default = "default_seed_bps")]
    pub seed_reserve_bps: Vec<u32>,
    #[serde(default = "default_quote_budget")]
    pub max_quote_evaluations: usize,
    #[serde(default = "default_local_iterations")]
    pub local_search_iterations: usize,
    #[serde(default = "default_theoretical_edge")]
    pub minimum_theoretical_edge_bps: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    #[serde(default = "default_impact")]
    pub max_leg_price_impact_bps: u32,
    #[serde(default = "default_total_impact")]
    pub max_total_price_impact_bps: u32,
    #[serde(default = "default_min_liquidity")]
    pub minimum_pool_liquidity: String,
    #[serde(default)]
    pub min_three_hop_net_profit: Option<String>,
    #[serde(default)]
    pub min_three_hop_roi_bps: Option<i64>,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_leg_price_impact_bps: default_impact(),
            max_total_price_impact_bps: default_total_impact(),
            minimum_pool_liquidity: default_min_liquidity(),
            min_three_hop_net_profit: None,
            min_three_hop_roi_bps: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GasConfig {
    #[serde(default = "default_two_hop_gas")]
    pub two_hop_fallback_gas: u64,
    #[serde(default = "default_three_hop_gas")]
    pub three_hop_fallback_gas: u64,
    #[serde(default = "default_gas_buffer")]
    pub gas_units_buffer_bps: u32,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            two_hop_fallback_gas: default_two_hop_gas(),
            three_hop_fallback_gas: default_three_hop_gas(),
            gas_units_buffer_bps: default_gas_buffer(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_max_hops")]
    pub max_steps: usize,
    #[serde(default = "default_leg_slippage")]
    pub default_leg_slippage_bps: u32,
    #[serde(default = "default_three_hop_slippage")]
    pub three_hop_leg_slippage_bps: u32,
    #[serde(default = "default_deadline")]
    pub deadline_seconds: u64,
    #[serde(default)]
    pub executor_address: Option<String>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_steps: 2,
            default_leg_slippage_bps: default_leg_slippage(),
            three_hop_leg_slippage_bps: default_three_hop_slippage(),
            deadline_seconds: default_deadline(),
            executor_address: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenConfig {
    pub symbol: String,
    pub address: String,
    pub decimals: u8,
    #[serde(default)]
    pub anchor: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    pub name: String,
    pub address: String,
    pub factory: String,
    #[serde(default)]
    pub router: Option<String>,
    pub token0: String,
    pub token1: String,
    #[serde(default = "default_fee_numerator")]
    pub fee_numerator: u32,
    #[serde(default = "default_fee_denominator")]
    pub fee_denominator: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_workers() -> usize {
    1
}
fn default_poll_interval() -> u64 {
    1_000
}
fn default_resync_blocks() -> u64 {
    100
}
fn default_queue() -> usize {
    1024
}
fn default_age() -> u64 {
    1
}
fn default_block_gap() -> u64 {
    0
}
fn default_gas_units() -> u64 {
    220_000
}
fn default_gas_price() -> String {
    "30000000000".into()
}
fn default_native_price() -> String {
    "3000000000".into()
}
fn default_min_profit() -> String {
    "500000".into()
}
fn default_min_gross_profit() -> String {
    "1000000".into()
}
fn default_max_gas() -> String {
    "100000000".into()
}
fn default_roi() -> i64 {
    5
}
fn default_risk_buffer() -> u32 {
    20
}
fn default_step() -> String {
    "1000".into()
}
fn default_reserve_bps() -> u32 {
    100
}
fn default_seed_bps() -> Vec<u32> {
    vec![1, 3, 10, 30, 100]
}
fn default_quote_budget() -> usize {
    128
}
fn default_local_iterations() -> usize {
    24
}
fn default_theoretical_edge() -> u32 {
    1
}
fn default_max_hops() -> usize {
    3
}
fn default_max_routes() -> usize {
    100_000
}
fn default_max_routes_per_anchor() -> usize {
    20_000
}
fn default_max_edges() -> usize {
    10
}
fn default_impact() -> u32 {
    100
}
fn default_total_impact() -> u32 {
    200
}
fn default_min_liquidity() -> String {
    "1".into()
}
fn default_two_hop_gas() -> u64 {
    180_000
}
fn default_three_hop_gas() -> u64 {
    260_000
}
fn default_gas_buffer() -> u32 {
    1_500
}
fn default_leg_slippage() -> u32 {
    10
}
fn default_three_hop_slippage() -> u32 {
    15
}
fn default_deadline() -> u64 {
    30
}
fn default_fee_numerator() -> u32 {
    997
}
fn default_fee_denominator() -> u32 {
    1000
}
fn default_true() -> bool {
    true
}

impl DexV2Config {
    pub fn load(path: impl AsRef<Path>) -> DexV2Result<Self> {
        let text = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            DexV2Error::Configuration(format!("read {}: {error}", path.as_ref().display()))
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| {
            DexV2Error::Configuration(format!("parse {}: {error}", path.as_ref().display()))
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> DexV2Result<()> {
        if self.chain_id == 0 || self.rpc_http_url.trim().is_empty() {
            return Err(DexV2Error::Configuration(
                "chain_id and rpc_http_url are required".into(),
            ));
        }
        if self.worker_count == 0
            || self.queue_capacity == 0
            || self.poll_interval_ms == 0
            || self.resync_interval_blocks == 0
        {
            return Err(DexV2Error::Configuration("worker_count, queue_capacity, poll_interval_ms and resync_interval_blocks must be positive".into()));
        }
        if self.execution_mode == ExecutionMode::Live {
            return Err(DexV2Error::Configuration(
                "live broadcast remains disabled; use shadow or simulate_only".into(),
            ));
        }
        if self.execution_mode == ExecutionMode::SimulateOnly
            && self.execution.executor_address.is_none()
        {
            return Err(DexV2Error::Configuration(
                "simulate_only requires execution.executor_address".into(),
            ));
        }
        if !self.routes.enable_two_hop && !self.routes.enable_three_hop {
            return Err(DexV2Error::Configuration(
                "at least one route kind must be enabled".into(),
            ));
        }
        if !(2..=3).contains(&self.routes.max_route_hops)
            || self.routes.max_routes_total == 0
            || self.routes.max_routes_per_anchor == 0
            || self.routes.max_edges_per_token_pair == 0
            || self.execution.max_steps < self.routes.max_route_hops
        {
            return Err(DexV2Error::Configuration(
                "invalid route or execution limits".into(),
            ));
        }
        if self.optimizer.max_quote_evaluations < 3
            || self.optimizer.local_search_iterations == 0
            || self.optimizer.seed_reserve_bps.is_empty()
            || self
                .optimizer
                .seed_reserve_bps
                .iter()
                .any(|bps| *bps == 0 || *bps > 10_000)
            || self.risk.max_leg_price_impact_bps > 10_000
            || self.risk.max_total_price_impact_bps > 10_000
            || self.gas.gas_units_buffer_bps > 10_000
            || self.execution.default_leg_slippage_bps > 10_000
            || self.execution.three_hop_leg_slippage_bps > 10_000
        {
            return Err(DexV2Error::Configuration(
                "invalid optimizer, gas, risk or slippage settings".into(),
            ));
        }
        let mut tokens = HashSet::new();
        let mut has_anchor = false;
        for token in &self.tokens {
            let address = parse_address(&token.address)?;
            if token.decimals > 36 || !tokens.insert(address) {
                return Err(DexV2Error::Configuration(format!(
                    "invalid or duplicate token {}",
                    token.symbol
                )));
            }
            has_anchor |= token.anchor;
        }
        if !has_anchor {
            return Err(DexV2Error::Configuration(
                "at least one anchor token is required".into(),
            ));
        }
        let mut pools = HashSet::new();
        for pool in self.pools.iter().filter(|pool| pool.enabled) {
            let address = parse_address(&pool.address)?;
            if !pools.insert(address)
                || pool.fee_denominator == 0
                || pool.fee_numerator >= pool.fee_denominator
            {
                return Err(DexV2Error::Configuration(format!(
                    "invalid or duplicate pool {}",
                    pool.name
                )));
            }
            let token0 = parse_address(&pool.token0)?;
            let token1 = parse_address(&pool.token1)?;
            if token0 == token1 || !tokens.contains(&token0) || !tokens.contains(&token1) {
                return Err(DexV2Error::Configuration(format!(
                    "pool {} references invalid tokens",
                    pool.name
                )));
            }
        }
        self.resolve_tokens(&self.routes.allowed_anchor_tokens)?;
        self.resolve_tokens(&self.routes.allowed_intermediate_tokens)?;
        let bounds = self.amount_bounds()?;
        if bounds.min_input.is_zero()
            || bounds.min_input > bounds.max_input
            || bounds.minimum_search_step.is_zero()
            || bounds.max_pool_reserve_bps > 10_000
        {
            return Err(DexV2Error::Configuration("invalid optimizer bounds".into()));
        }
        let _ = self.minimum_pool_liquidity()?;
        if let Some(address) = &self.execution.executor_address {
            let _ = parse_address(address)?;
        }
        Ok(())
    }

    fn token_lookup(&self) -> DexV2Result<HashMap<String, TokenId>> {
        let mut lookup = HashMap::new();
        for token in &self.tokens {
            let id = TokenId {
                chain_id: self.chain_id,
                address: parse_address(&token.address)?,
            };
            lookup.insert(token.symbol.to_ascii_uppercase(), id.clone());
            lookup.insert(token.address.to_ascii_lowercase(), id);
        }
        Ok(lookup)
    }

    fn resolve_tokens(&self, values: &[String]) -> DexV2Result<HashSet<TokenId>> {
        let lookup = self.token_lookup()?;
        values
            .iter()
            .map(|value| {
                lookup
                    .get(&value.to_ascii_uppercase())
                    .or_else(|| lookup.get(&value.to_ascii_lowercase()))
                    .cloned()
                    .ok_or_else(|| {
                        DexV2Error::Configuration(format!("unknown route token {value}"))
                    })
            })
            .collect()
    }

    pub fn route_generation_config(&self) -> DexV2Result<RouteGenerationConfig> {
        let mut anchors = self.resolve_tokens(&self.routes.allowed_anchor_tokens)?;
        if anchors.is_empty() {
            anchors = self
                .token_meta()?
                .into_iter()
                .filter(|token| token.anchor)
                .map(|token| token.id)
                .collect();
        }
        let mut intermediates = self.resolve_tokens(&self.routes.allowed_intermediate_tokens)?;
        if intermediates.is_empty() {
            intermediates = self
                .token_meta()?
                .into_iter()
                .map(|token| token.id)
                .collect();
        }
        Ok(RouteGenerationConfig {
            enable_two_hop: self.routes.enable_two_hop,
            enable_three_hop: self.routes.enable_three_hop,
            max_route_hops: self.routes.max_route_hops,
            max_routes_total: self.routes.max_routes_total,
            max_routes_per_anchor: self.routes.max_routes_per_anchor,
            max_edges_per_token_pair: self.routes.max_edges_per_token_pair,
            allowed_anchor_tokens: anchors,
            allowed_intermediate_tokens: intermediates,
        })
    }

    pub fn token_meta(&self) -> DexV2Result<Vec<TokenMeta>> {
        self.tokens
            .iter()
            .map(|token| {
                Ok(TokenMeta {
                    id: TokenId {
                        chain_id: self.chain_id,
                        address: parse_address(&token.address)?,
                    },
                    symbol: token.symbol.clone(),
                    decimals: token.decimals,
                    anchor: token.anchor,
                })
            })
            .collect()
    }

    pub fn v2_pools(&self) -> DexV2Result<Vec<V2Pool>> {
        self.pools
            .iter()
            .filter(|pool| pool.enabled)
            .map(|pool| {
                Ok(V2Pool {
                    id: PoolId {
                        chain_id: self.chain_id,
                        address: parse_address(&pool.address)?,
                    },
                    name: pool.name.clone(),
                    protocol: Protocol::UniswapV2Compatible {
                        factory: parse_address(&pool.factory)?,
                        router: pool.router.as_deref().map(parse_address).transpose()?,
                    },
                    token0: TokenId {
                        chain_id: self.chain_id,
                        address: parse_address(&pool.token0)?,
                    },
                    token1: TokenId {
                        chain_id: self.chain_id,
                        address: parse_address(&pool.token1)?,
                    },
                    fee_numerator: pool.fee_numerator,
                    fee_denominator: pool.fee_denominator,
                })
            })
            .collect()
    }

    pub fn amount_bounds(&self) -> DexV2Result<AmountBounds> {
        Ok(AmountBounds {
            min_input: parse_u256(&self.optimizer.min_input)?,
            max_input: parse_u256(&self.optimizer.max_input)?,
            minimum_search_step: parse_u256(&self.optimizer.minimum_search_step)?,
            max_pool_reserve_bps: self.optimizer.max_pool_reserve_bps,
            seed_reserve_bps: self.optimizer.seed_reserve_bps.clone(),
            max_quote_evaluations: self.optimizer.max_quote_evaluations,
            local_search_iterations: self.optimizer.local_search_iterations,
        })
    }
    pub fn min_net_profit(&self) -> DexV2Result<U256> {
        parse_u256(&self.min_net_profit_anchor)
    }
    pub fn min_three_hop_net_profit(&self) -> DexV2Result<U256> {
        self.risk
            .min_three_hop_net_profit
            .as_deref()
            .map(parse_u256)
            .transpose()
            .map(|value| value.unwrap_or_else(|| U256::from(0)))
    }
    pub fn min_gross_profit(&self) -> DexV2Result<U256> {
        parse_u256(&self.min_gross_profit_anchor)
    }
    pub fn max_gas_anchor(&self) -> DexV2Result<U256> {
        parse_u256(&self.max_gas_anchor)
    }
    pub fn max_fee_per_gas(&self) -> DexV2Result<U256> {
        parse_u256(&self.max_fee_per_gas)
    }
    pub fn native_price_anchor(&self) -> DexV2Result<U256> {
        parse_u256(&self.native_price_anchor)
    }
    pub fn minimum_pool_liquidity(&self) -> DexV2Result<U256> {
        parse_u256(&self.risk.minimum_pool_liquidity)
    }
    pub fn executor_address(&self) -> DexV2Result<Option<Address>> {
        self.execution
            .executor_address
            .as_deref()
            .map(parse_address)
            .transpose()
    }
}

fn parse_address(value: &str) -> DexV2Result<Address> {
    Address::from_str(value)
        .map_err(|error| DexV2Error::Configuration(format!("invalid address {value}: {error}")))
}
fn parse_u256(value: &str) -> DexV2Result<U256> {
    U256::from_str(value)
        .map_err(|error| DexV2Error::Configuration(format!("invalid integer {value}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_enables_bounded_three_hop_and_is_shadow_safe() {
        let config: DexV2Config =
            toml::from_str(include_str!("../../../../dex-arbitrage.toml")).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.execution_mode, ExecutionMode::Shadow);
        assert!(config.routes.enable_two_hop);
        assert!(!config.routes.enable_three_hop);
        assert_eq!(config.routes.max_route_hops, 3);
        assert_eq!(config.log_query_delay_blocks, 5);
    }

    #[test]
    fn live_mode_and_invalid_route_depth_are_rejected() {
        let mut config: DexV2Config =
            toml::from_str(include_str!("../../../../dex-arbitrage.toml")).unwrap();
        config.execution_mode = ExecutionMode::Live;
        assert!(config.validate().is_err());
        config.execution_mode = ExecutionMode::Shadow;
        config.routes.max_route_hops = 4;
        assert!(config.validate().is_err());
    }
}
