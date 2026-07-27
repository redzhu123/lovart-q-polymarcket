use std::collections::HashSet;
use std::sync::Arc;

use alloy_primitives::{I256, U256};

use super::error::{DexV2Error, DexV2Result};
use super::graph::PoolRegistry;
use super::types::{ArbitrageQuote, ArbitrageRoute, RouteKind, StateSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    NoTheoreticalEdge,
    NoProfitableSeedAmount,
    GrossProfitTooLow,
    NetProfitTooLow,
    RoiTooLow,
    InsufficientLiquidity,
    StateStale,
    BlockMismatch,
    GasTooHigh,
    RouteExpired,
    SimulationFailed,
    TokenNotAllowed,
    PoolNotAllowed,
    InvalidRouteLength,
    RouteNotClosed,
    DisconnectedLegs,
    DuplicatePool,
    DuplicateToken,
    IntermediateTokenNotAllowed,
    TooManyRoutes,
    QuoteBudgetExceeded,
    PriceImpactTooHigh,
}

pub trait RiskGuard: Send + Sync {
    fn validate_route(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
    ) -> Result<(), RejectionReason>;
    fn validate_quote(
        &self,
        route: &ArbitrageRoute,
        quote: &ArbitrageQuote,
    ) -> Result<(), RejectionReason>;
}

pub struct DefaultRiskGuard {
    registry: Arc<PoolRegistry>,
    allowed_intermediates: HashSet<super::types::TokenId>,
    pub min_net_profit: U256,
    pub min_three_hop_net_profit: U256,
    pub min_gross_profit: U256,
    pub max_gas_anchor: U256,
    pub min_roi_bps: i64,
    pub min_three_hop_roi_bps: i64,
    pub max_state_block_gap: u64,
    pub max_opportunity_age_blocks: u64,
    pub minimum_pool_liquidity: U256,
    pub max_leg_price_impact_bps: u32,
    pub max_total_price_impact_bps: u32,
    pub max_quote_evaluations: usize,
}

impl DefaultRiskGuard {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<PoolRegistry>,
        allowed_intermediates: HashSet<super::types::TokenId>,
        min_net_profit: U256,
        min_three_hop_net_profit: U256,
        min_gross_profit: U256,
        max_gas_anchor: U256,
        min_roi_bps: i64,
        min_three_hop_roi_bps: i64,
        max_state_block_gap: u64,
        max_opportunity_age_blocks: u64,
        minimum_pool_liquidity: U256,
        max_leg_price_impact_bps: u32,
        max_total_price_impact_bps: u32,
        max_quote_evaluations: usize,
    ) -> Self {
        Self {
            registry,
            allowed_intermediates,
            min_net_profit,
            min_three_hop_net_profit,
            min_gross_profit,
            max_gas_anchor,
            min_roi_bps,
            min_three_hop_roi_bps,
            max_state_block_gap,
            max_opportunity_age_blocks,
            minimum_pool_liquidity,
            max_leg_price_impact_bps,
            max_total_price_impact_bps,
            max_quote_evaluations,
        }
    }
}

impl RiskGuard for DefaultRiskGuard {
    fn validate_route(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
    ) -> Result<(), RejectionReason> {
        if route.hop_count() != 2 && route.hop_count() != 3 {
            return Err(RejectionReason::InvalidRouteLength);
        }
        if route.anchor_token.chain_id != snapshot.chain_id || route.chain_id != snapshot.chain_id {
            return Err(RejectionReason::BlockMismatch);
        }
        if route.legs.first().map(|leg| &leg.token_in) != Some(&route.anchor_token)
            || route.legs.last().map(|leg| &leg.token_out) != Some(&route.anchor_token)
        {
            return Err(RejectionReason::RouteNotClosed);
        }
        let mut pools = HashSet::new();
        let mut intermediates = HashSet::new();
        for (index, leg) in route.legs.iter().enumerate() {
            if index > 0 && route.legs[index - 1].token_out != leg.token_in {
                return Err(RejectionReason::DisconnectedLegs);
            }
            if !pools.insert(&leg.pool_id) {
                return Err(RejectionReason::DuplicatePool);
            }
            if index + 1 < route.hop_count() && !intermediates.insert(&leg.token_out) {
                return Err(RejectionReason::DuplicateToken);
            }
            if index + 1 < route.hop_count()
                && !self.allowed_intermediates.is_empty()
                && !self.allowed_intermediates.contains(&leg.token_out)
            {
                return Err(RejectionReason::IntermediateTokenNotAllowed);
            }
            if self.registry.pool(&leg.pool_id).is_err() {
                return Err(RejectionReason::PoolNotAllowed);
            }
            let state = snapshot
                .pools
                .get(&leg.pool_id)
                .ok_or(RejectionReason::StateStale)?;
            if state.reserve0 < self.minimum_pool_liquidity
                || state.reserve1 < self.minimum_pool_liquidity
            {
                return Err(RejectionReason::InsufficientLiquidity);
            }
        }
        if snapshot
            .max_state_block
            .saturating_sub(snapshot.min_state_block)
            > self.max_state_block_gap
        {
            return Err(RejectionReason::StateStale);
        }
        Ok(())
    }

    fn validate_quote(
        &self,
        route: &ArbitrageRoute,
        quote: &ArbitrageQuote,
    ) -> Result<(), RejectionReason> {
        if quote.gross_profit < self.min_gross_profit {
            return Err(RejectionReason::GrossProfitTooLow);
        }
        if quote.estimated_gas_anchor > self.max_gas_anchor {
            return Err(RejectionReason::GasTooHigh);
        }
        let (min_net, min_roi) = match route.kind {
            RouteKind::TwoHop => (self.min_net_profit, self.min_roi_bps),
            RouteKind::ThreeHop => (
                self.min_three_hop_net_profit.max(self.min_net_profit),
                self.min_three_hop_roi_bps.max(self.min_roi_bps),
            ),
        };
        if quote.net_profit < I256::from_raw(min_net) {
            return Err(RejectionReason::NetProfitTooLow);
        }
        if quote.roi_bps < I256::try_from(min_roi).unwrap_or(I256::MAX) {
            return Err(RejectionReason::RoiTooLow);
        }
        if quote.quote_evaluations > self.max_quote_evaluations {
            return Err(RejectionReason::QuoteBudgetExceeded);
        }
        let total_impact = quote.price_impacts.iter().try_fold(0u32, |total, impact| {
            if impact.impact_bps > self.max_leg_price_impact_bps {
                return Err(RejectionReason::PriceImpactTooHigh);
            }
            total
                .checked_add(impact.impact_bps)
                .ok_or(RejectionReason::PriceImpactTooHigh)
        })?;
        if total_impact > self.max_total_price_impact_bps {
            return Err(RejectionReason::PriceImpactTooHigh);
        }
        Ok(())
    }
}

impl From<RejectionReason> for DexV2Error {
    fn from(value: RejectionReason) -> Self {
        DexV2Error::Risk(format!("{value:?}"))
    }
}

pub fn ensure_supported_decimals(decimals: u8) -> DexV2Result<()> {
    if decimals > 36 {
        Err(DexV2Error::Risk("unsupported token decimals".into()))
    } else {
        Ok(())
    }
}
