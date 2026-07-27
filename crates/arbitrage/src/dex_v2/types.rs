use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use alloy_primitives::{Address, B256, Bytes, I256, U256};
use serde::{Deserialize, Serialize};

use super::error::{DexV2Error, DexV2Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenId {
    pub chain_id: u64,
    pub address: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PoolId {
    pub chain_id: u64,
    pub address: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    UniswapV2Compatible {
        factory: Address,
        router: Option<Address>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenMeta {
    pub id: TokenId,
    pub symbol: String,
    pub decimals: u8,
    pub anchor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V2Pool {
    pub id: PoolId,
    pub name: String,
    pub protocol: Protocol,
    pub token0: TokenId,
    pub token1: TokenId,
    pub fee_numerator: u32,
    pub fee_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2PoolState {
    pub reserve0: U256,
    pub reserve1: U256,
    pub block_number: u64,
    pub block_hash: Option<B256>,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolEdge {
    pub pool_id: PoolId,
    pub token_in: TokenId,
    pub token_out: TokenId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SwapLeg {
    pub index: u8,
    pub pool_id: PoolId,
    pub token_in: TokenId,
    pub token_out: TokenId,
}

impl From<(u8, PoolEdge)> for SwapLeg {
    fn from((index, edge): (u8, PoolEdge)) -> Self {
        Self {
            index,
            pool_id: edge.pool_id,
            token_in: edge.token_in,
            token_out: edge.token_out,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RouteId(pub B256);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    TwoHop,
    ThreeHop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArbitrageRoute {
    pub id: RouteId,
    pub chain_id: u64,
    pub kind: RouteKind,
    pub anchor_token: TokenId,
    pub legs: Vec<SwapLeg>,
    pub involved_tokens: Vec<TokenId>,
    pub involved_pools: Vec<PoolId>,
}

impl ArbitrageRoute {
    pub fn new(
        id: RouteId,
        chain_id: u64,
        anchor_token: TokenId,
        legs: Vec<SwapLeg>,
    ) -> DexV2Result<Self> {
        let kind = match legs.len() {
            2 => RouteKind::TwoHop,
            3 => RouteKind::ThreeHop,
            count => {
                return Err(DexV2Error::Route(format!(
                    "invalid hop count {count}; only 2 or 3 are supported"
                )));
            }
        };
        if anchor_token.chain_id != chain_id
            || legs.first().map(|leg| &leg.token_in) != Some(&anchor_token)
            || legs.last().map(|leg| &leg.token_out) != Some(&anchor_token)
        {
            return Err(DexV2Error::Route("route is not closed at anchor".into()));
        }
        for (position, leg) in legs.iter().enumerate() {
            if leg.index as usize != position
                || leg.pool_id.chain_id != chain_id
                || leg.token_in.chain_id != chain_id
                || leg.token_out.chain_id != chain_id
            {
                return Err(DexV2Error::Route(
                    "route chain or leg index mismatch".into(),
                ));
            }
            if position > 0 && legs[position - 1].token_out != leg.token_in {
                return Err(DexV2Error::Route("route legs are disconnected".into()));
            }
        }
        let involved_pools = legs
            .iter()
            .map(|leg| leg.pool_id.clone())
            .collect::<Vec<_>>();
        if involved_pools.iter().collect::<HashSet<_>>().len() != involved_pools.len() {
            return Err(DexV2Error::Route("route repeats a pool".into()));
        }
        let mut involved_tokens = vec![anchor_token.clone()];
        involved_tokens.extend(
            legs.iter()
                .take(legs.len() - 1)
                .map(|leg| leg.token_out.clone()),
        );
        if involved_tokens.iter().collect::<HashSet<_>>().len() != involved_tokens.len() {
            return Err(DexV2Error::Route(
                "route repeats an intermediate token".into(),
            ));
        }
        Ok(Self {
            id,
            chain_id,
            kind,
            anchor_token,
            legs,
            involved_tokens,
            involved_pools,
        })
    }

    pub fn hop_count(&self) -> usize {
        self.legs.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateVersion {
    pub block_number: u64,
    pub max_log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteCheckKey {
    pub chain_id: u64,
    pub route_id: RouteId,
    pub state_version: StateVersion,
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub chain_id: u64,
    pub target_block: u64,
    pub min_state_block: u64,
    pub max_state_block: u64,
    pub state_version: StateVersion,
    pub pools: HashMap<PoolId, Arc<V2PoolState>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapQuote {
    pub amount_in: U256,
    pub amount_out: U256,
    pub fee_amount: Option<U256>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegQuote {
    pub leg_index: u8,
    pub pool_id: PoolId,
    pub token_in: TokenId,
    pub token_out: TokenId,
    pub amount_in: U256,
    pub amount_out: U256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceImpact {
    pub leg_index: u8,
    pub expected_marginal_out: U256,
    pub actual_out: U256,
    pub impact_bps: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteQuote {
    pub route_id: RouteId,
    pub block_number: u64,
    pub amount_in: U256,
    pub amount_out: U256,
    pub leg_quotes: Vec<LegQuote>,
    pub price_impacts: Vec<PriceImpact>,
    pub gross_profit: I256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationMethod {
    TwoPoolClosedFormWithLocalSearch,
    CoarseToFine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizedRouteQuote {
    pub route_quote: RouteQuote,
    pub tested_amounts: usize,
    pub method: OptimizationMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmountBounds {
    pub min_input: U256,
    pub max_input: U256,
    pub minimum_search_step: U256,
    pub max_pool_reserve_bps: u32,
    pub seed_reserve_bps: Vec<u32>,
    pub max_quote_evaluations: usize,
    pub local_search_iterations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostEstimate {
    pub gas_units: u64,
    pub max_fee_per_gas: U256,
    pub priority_fee_per_gas: U256,
    pub native_token_cost: U256,
    pub anchor_token_cost: U256,
    pub builder_tip_anchor: U256,
    pub risk_buffer_anchor: U256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfitBreakdown {
    pub amount_in: U256,
    pub amount_out: U256,
    pub gross_profit: U256,
    pub gas_anchor: U256,
    pub tip_anchor: U256,
    pub risk_buffer: U256,
    pub net_profit: I256,
    pub roi_bps: I256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArbitrageQuote {
    pub route_id: RouteId,
    pub block_number: u64,
    pub amount_in: U256,
    pub amount_out: U256,
    pub gross_profit: U256,
    pub estimated_gas_units: u64,
    pub estimated_gas_native: U256,
    pub estimated_gas_anchor: U256,
    pub risk_buffer: U256,
    pub net_profit: I256,
    pub roi_bps: I256,
    pub leg_quotes: Vec<LegQuote>,
    pub price_impacts: Vec<PriceImpact>,
    pub optimization_method: OptimizationMethod,
    pub quote_evaluations: usize,
    pub state_min_block: u64,
    pub state_max_block: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStatus {
    Detected,
    Optimized,
    Simulated,
    Rejected,
    Submitted,
    Included,
    Reverted,
    Expired,
}

impl OpportunityStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Detected,
                Self::Optimized | Self::Rejected | Self::Expired
            ) | (
                Self::Optimized,
                Self::Simulated | Self::Rejected | Self::Expired
            ) | (
                Self::Simulated,
                Self::Submitted | Self::Rejected | Self::Expired
            ) | (Self::Submitted, Self::Included | Self::Reverted)
        )
    }
}

#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub id: B256,
    pub chain_id: u64,
    pub route: ArbitrageRoute,
    pub trigger_pool: PoolId,
    pub quote: ArbitrageQuote,
    pub simulation_result: SimulationResult,
    pub status: OpportunityStatus,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Shadow,
    SimulateOnly,
    Live,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationMode {
    #[default]
    Latest,
    Safe,
    Finalized,
}

#[derive(Debug, Clone)]
pub struct ChainLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Bytes,
    pub block_number: u64,
    pub block_hash: Option<B256>,
    pub log_index: u64,
    pub removed: bool,
}

#[derive(Debug, Clone)]
pub struct PoolUpdate {
    pub pool_id: PoolId,
    pub state: V2PoolState,
    pub log_index: u64,
}

#[derive(Debug, Clone)]
pub struct SwapRequest {
    pub pool: V2Pool,
    pub token_in: TokenId,
    pub amount_in: U256,
    pub min_amount_out: U256,
    pub recipient: Address,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedCall {
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionStep {
    pub pair: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub min_amount_out: U256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub route_id: RouteId,
    pub anchor_token: TokenId,
    pub amount_in: U256,
    pub min_profit: U256,
    pub deadline: u64,
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone)]
pub struct SimulationRequest {
    pub from: Address,
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
    pub gas: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub block_number: u64,
    pub expected_amount_out: U256,
    pub expected_profit: I256,
    pub execution: Option<ExecutionRequest>,
    pub leg_quotes: Vec<LegQuote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegExecutionResult {
    pub leg_index: u8,
    pub pool_id: PoolId,
    pub amount_in: U256,
    pub amount_out: U256,
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub success: bool,
    pub gas_used: u64,
    pub return_data: Bytes,
    pub revert_reason: Option<String>,
    pub realized_amount_out: Option<U256>,
    pub realized_profit: Option<I256>,
    pub leg_results: Vec<LegExecutionResult>,
    pub block_number: u64,
}
