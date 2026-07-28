use std::collections::HashSet;
use std::sync::Arc;

use alloy_primitives::{I256, U256, aliases::U512};

use super::error::{DexV2Error, DexV2Result};
use super::graph::PoolRegistry;
use super::quoter::{RouteQuoter, oriented_reserves};
use super::types::{
    AmountBounds, ArbitrageRoute, OptimizationMethod, OptimizedRouteQuote, RouteKind, RouteQuote,
    StateSnapshot,
};

pub fn has_marginal_edge(
    registry: &PoolRegistry,
    route: &ArbitrageRoute,
    snapshot: &StateSnapshot,
    minimum_edge_bps: u32,
) -> DexV2Result<bool> {
    let mut numerator = U512::from(1u64);
    let mut denominator = U512::from(1u64);
    for leg in &route.legs {
        let pool = registry.pool(&leg.pool_id)?;
        let state = snapshot
            .pools
            .get(&leg.pool_id)
            .ok_or_else(|| DexV2Error::PoolState("route pool missing from snapshot".into()))?;
        let (reserve_in, reserve_out) = oriented_reserves(pool, state, &leg.token_in)?;
        numerator = numerator
            .checked_mul(U512::from(pool.fee_numerator))
            .and_then(|value| value.checked_mul(U512::from(reserve_out)))
            .ok_or_else(|| DexV2Error::Quote("marginal numerator overflow".into()))?;
        denominator = denominator
            .checked_mul(U512::from(pool.fee_denominator))
            .and_then(|value| value.checked_mul(U512::from(reserve_in)))
            .ok_or_else(|| DexV2Error::Quote("marginal denominator overflow".into()))?;
    }
    let left = numerator
        .checked_mul(U512::from(10_000u64))
        .ok_or_else(|| DexV2Error::Quote("marginal comparison overflow".into()))?;
    let right = denominator
        .checked_mul(U512::from(10_000u64 + u64::from(minimum_edge_bps)))
        .ok_or_else(|| DexV2Error::Quote("marginal comparison overflow".into()))?;
    Ok(left > right)
}

pub trait AmountOptimizer: Send + Sync {
    fn optimize(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        bounds: &AmountBounds,
    ) -> DexV2Result<OptimizedRouteQuote>;
}

pub struct IntegerSearchOptimizer {
    registry: Arc<PoolRegistry>,
    quoter: Arc<dyn RouteQuoter>,
}

impl IntegerSearchOptimizer {
    pub fn new(registry: Arc<PoolRegistry>, quoter: Arc<dyn RouteQuoter>) -> Self {
        Self { registry, quoter }
    }

    fn quote(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        input: U256,
        evaluations: &mut usize,
        budget: usize,
    ) -> DexV2Result<RouteQuote> {
        if *evaluations >= budget {
            return Err(DexV2Error::Optimization(format!(
                "route {} hop_count {} quote budget {budget} exceeded at amount {input}",
                route.id.0,
                route.hop_count()
            )));
        }
        *evaluations += 1;
        self.quoter.quote_exact_in(route, snapshot, input)
    }

    fn effective_max(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        bounds: &AmountBounds,
    ) -> DexV2Result<U256> {
        let first = &route.legs[0];
        let pool = self.registry.pool(&first.pool_id)?;
        let state = snapshot
            .pools
            .get(&first.pool_id)
            .ok_or_else(|| DexV2Error::PoolState("first pool missing from snapshot".into()))?;
        let (reserve_in, _) = oriented_reserves(pool, state, &first.token_in)?;
        let reserve_cap = reserve_in
            .checked_mul(U256::from(bounds.max_pool_reserve_bps))
            .ok_or_else(|| DexV2Error::Optimization("reserve cap overflow".into()))?
            / U256::from(10_000);
        let mut candidate = bounds.max_input.min(reserve_cap);
        // Propagate the candidate through every leg and shrink anchor input if
        // a downstream pool would exceed its own configured reserve share.
        for _ in 0..route.hop_count() {
            let quote = self.quoter.quote_exact_in(route, snapshot, candidate)?;
            let mut adjusted = candidate;
            for (leg, leg_quote) in route.legs.iter().zip(&quote.leg_quotes) {
                let pool = self.registry.pool(&leg.pool_id)?;
                let state = snapshot.pools.get(&leg.pool_id).ok_or_else(|| {
                    DexV2Error::PoolState("downstream pool missing from snapshot".into())
                })?;
                let (leg_reserve_in, _) = oriented_reserves(pool, state, &leg.token_in)?;
                let leg_cap = leg_reserve_in
                    .checked_mul(U256::from(bounds.max_pool_reserve_bps))
                    .ok_or_else(|| {
                        DexV2Error::Optimization("downstream reserve cap overflow".into())
                    })?
                    / U256::from(10_000);
                if !leg_quote.amount_in.is_zero() && leg_quote.amount_in > leg_cap {
                    adjusted = adjusted.min(
                        candidate.checked_mul(leg_cap).ok_or_else(|| {
                            DexV2Error::Optimization("downstream cap scaling overflow".into())
                        })? / leg_quote.amount_in,
                    );
                }
            }
            if adjusted >= candidate {
                break;
            }
            candidate = adjusted;
        }
        Ok(candidate)
    }

    pub fn has_profitable_seed(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        bounds: &AmountBounds,
    ) -> DexV2Result<bool> {
        let max = self.effective_max(route, snapshot, bounds)?;
        if max < bounds.min_input {
            return Ok(false);
        }
        let mut seeds = vec![bounds.min_input, max];
        for bps in &bounds.seed_reserve_bps {
            let candidate = max
                .checked_mul(U256::from(*bps))
                .ok_or_else(|| DexV2Error::Optimization("seed overflow".into()))?
                / U256::from(10_000);
            if candidate >= bounds.min_input {
                seeds.push(candidate);
            }
        }
        seeds.sort_unstable();
        seeds.dedup();
        for input in seeds {
            if self
                .quoter
                .quote_exact_in(route, snapshot, input)?
                .gross_profit
                > I256::ZERO
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl AmountOptimizer for IntegerSearchOptimizer {
    fn optimize(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        bounds: &AmountBounds,
    ) -> DexV2Result<OptimizedRouteQuote> {
        if bounds.max_quote_evaluations == 0 {
            return Err(DexV2Error::Optimization("quote budget is zero".into()));
        }
        let max_input = self.effective_max(route, snapshot, bounds)?;
        if max_input < bounds.min_input {
            return Err(DexV2Error::Optimization(format!(
                "route {} reserve cap is below min_input {}",
                route.id.0, bounds.min_input
            )));
        }
        let method = match route.kind {
            RouteKind::TwoHop => OptimizationMethod::TwoPoolClosedFormWithLocalSearch,
            RouteKind::ThreeHop => OptimizationMethod::CoarseToFine,
            RouteKind::FourHop => OptimizationMethod::CoarseToFine,
        };
        let mut candidate_set = HashSet::new();
        candidate_set.insert(bounds.min_input);
        candidate_set.insert(max_input);
        for bps in &bounds.seed_reserve_bps {
            let candidate = max_input
                .checked_mul(U256::from(*bps))
                .ok_or_else(|| DexV2Error::Optimization("candidate overflow".into()))?
                / U256::from(10_000);
            if candidate >= bounds.min_input && candidate <= max_input {
                candidate_set.insert(candidate);
            }
        }
        let mut evaluations = 0usize;
        let mut best: Option<RouteQuote> = None;
        for candidate in candidate_set {
            let quote = self.quote(
                route,
                snapshot,
                candidate,
                &mut evaluations,
                bounds.max_quote_evaluations,
            )?;
            if best
                .as_ref()
                .is_none_or(|current| quote.gross_profit > current.gross_profit)
            {
                best = Some(quote);
            }
        }
        let mut best = best.ok_or_else(|| DexV2Error::Optimization("no seed amounts".into()))?;

        let mut span = (max_input - bounds.min_input) / U256::from(4);
        let minimum_span = bounds.minimum_search_step.max(U256::from(1));
        for _ in 0..bounds.local_search_iterations {
            if evaluations + 2 > bounds.max_quote_evaluations || span < minimum_span {
                break;
            }
            let left = best.amount_in.saturating_sub(span).max(bounds.min_input);
            let right = best.amount_in.saturating_add(span).min(max_input);
            for candidate in [left, right] {
                let quote = self.quote(
                    route,
                    snapshot,
                    candidate,
                    &mut evaluations,
                    bounds.max_quote_evaluations,
                )?;
                if quote.gross_profit > best.gross_profit {
                    best = quote;
                }
            }
            span /= U256::from(2);
        }
        for offset in [1u64, 2, 4] {
            if evaluations >= bounds.max_quote_evaluations {
                break;
            }
            let delta = bounds
                .minimum_search_step
                .checked_mul(U256::from(offset))
                .ok_or_else(|| DexV2Error::Optimization("local search overflow".into()))?;
            for candidate in [
                best.amount_in.saturating_sub(delta),
                best.amount_in.saturating_add(delta),
            ] {
                if candidate < bounds.min_input
                    || candidate > max_input
                    || evaluations >= bounds.max_quote_evaluations
                {
                    continue;
                }
                let quote = self.quote(
                    route,
                    snapshot,
                    candidate,
                    &mut evaluations,
                    bounds.max_quote_evaluations,
                )?;
                if quote.gross_profit > best.gross_profit {
                    best = quote;
                }
            }
        }
        if best.gross_profit <= I256::ZERO {
            return Err(DexV2Error::Optimization(format!(
                "route {} hop_count {} has no profitable input after {evaluations} quotes",
                route.id.0,
                route.hop_count()
            )));
        }
        // Requote the chosen point so the returned result never depends on a stale temporary.
        if evaluations < bounds.max_quote_evaluations {
            best = self.quote(
                route,
                snapshot,
                best.amount_in,
                &mut evaluations,
                bounds.max_quote_evaluations,
            )?;
        }
        Ok(OptimizedRouteQuote {
            route_quote: best,
            tested_amounts: evaluations,
            method,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::SystemTime;

    use alloy_primitives::Address;

    use super::super::adapter::UniswapV2Adapter;
    use super::super::graph::stable_route_id;
    use super::super::quoter::LocalRouteQuoter;
    use super::super::types::{
        PoolId, Protocol, StateVersion, SwapLeg, TokenId, TokenMeta, V2Pool, V2PoolState,
    };
    use super::*;

    fn fixture(final_anchor_reserve: u64) -> (Arc<PoolRegistry>, ArbitrageRoute, StateSnapshot) {
        let tokens = [1u8, 2, 3].map(|last| TokenId {
            chain_id: 1,
            address: Address::with_last_byte(last),
        });
        let pairs = [(0usize, 1usize), (1, 2), (2, 0)];
        let pools = pairs
            .iter()
            .enumerate()
            .map(|(index, (token0, token1))| V2Pool {
                id: PoolId {
                    chain_id: 1,
                    address: Address::with_last_byte(10 + index as u8),
                },
                name: format!("P{index}"),
                protocol: Protocol::UniswapV2Compatible {
                    factory: Address::ZERO,
                    router: None,
                },
                token0: tokens[*token0].clone(),
                token1: tokens[*token1].clone(),
                fee_numerator: if index == 1 { 998 } else { 997 },
                fee_denominator: 1000,
            })
            .collect::<Vec<_>>();
        let registry = Arc::new(
            PoolRegistry::new(
                tokens
                    .iter()
                    .enumerate()
                    .map(|(index, token)| TokenMeta {
                        id: token.clone(),
                        symbol: format!("T{index}"),
                        decimals: if index == 0 { 6 } else { 18 },
                        anchor: index == 0,
                    })
                    .collect(),
                pools.clone(),
            )
            .unwrap(),
        );
        let legs = pools
            .iter()
            .enumerate()
            .map(|(index, pool)| SwapLeg {
                index: index as u8,
                pool_id: pool.id.clone(),
                token_in: tokens[pairs[index].0].clone(),
                token_out: tokens[pairs[index].1].clone(),
            })
            .collect::<Vec<_>>();
        let route = ArbitrageRoute::new(
            stable_route_id(1, &tokens[0], &legs),
            1,
            tokens[0].clone(),
            legs,
        )
        .unwrap();
        let states = pools
            .iter()
            .enumerate()
            .map(|(index, pool)| {
                (
                    pool.id.clone(),
                    Arc::new(V2PoolState {
                        reserve0: U256::from(1_000_000),
                        reserve1: U256::from(if index == 2 {
                            final_anchor_reserve
                        } else {
                            1_000_000
                        }),
                        block_number: 100,
                        block_hash: None,
                        updated_at: SystemTime::now(),
                    }),
                )
            })
            .collect::<HashMap<_, _>>();
        (
            registry,
            route,
            StateSnapshot {
                chain_id: 1,
                target_block: 100,
                min_state_block: 100,
                max_state_block: 100,
                state_version: StateVersion {
                    block_number: 100,
                    max_log_index: 1,
                },
                pools: states,
            },
        )
    }

    fn bounds(min: u64, max: u64, budget: usize) -> AmountBounds {
        AmountBounds {
            min_input: U256::from(min),
            max_input: U256::from(max),
            minimum_search_step: U256::from(1),
            max_pool_reserve_bps: 10_000,
            seed_reserve_bps: vec![1, 3, 10, 30, 100, 300, 1000, 3000, 5000],
            max_quote_evaluations: budget,
            local_search_iterations: 24,
        }
    }

    fn optimizer(registry: Arc<PoolRegistry>) -> IntegerSearchOptimizer {
        let adapter = Arc::new(UniswapV2Adapter::new());
        IntegerSearchOptimizer::new(
            registry.clone(),
            Arc::new(LocalRouteQuoter::new(registry, adapter)),
        )
    }

    #[test]
    fn finds_profitable_three_hop_with_mixed_fees_and_decimals() {
        let (registry, route, snapshot) = fixture(1_080_000);
        assert!(has_marginal_edge(&registry, &route, &snapshot, 1).unwrap());
        let result = optimizer(registry)
            .optimize(&route, &snapshot, &bounds(10, 200_000, 64))
            .unwrap();
        assert!(result.route_quote.gross_profit > I256::ZERO);
        assert!(result.route_quote.amount_in > U256::from(10));
        assert!(result.route_quote.amount_in < U256::from(200_000));
        assert!(result.tested_amounts <= 64);
        assert_eq!(result.method, OptimizationMethod::CoarseToFine);
    }

    #[test]
    fn balanced_triangle_has_no_edge_or_profitable_input() {
        let (registry, route, snapshot) = fixture(1_000_000);
        assert!(!has_marginal_edge(&registry, &route, &snapshot, 0).unwrap());
        assert!(
            optimizer(registry)
                .optimize(&route, &snapshot, &bounds(1, 100_000, 32))
                .is_err()
        );
    }

    #[test]
    fn optimizer_checks_min_and_max_boundaries() {
        for fixed in [1_000u64, 10_000] {
            let (registry, route, snapshot) = fixture(1_080_000);
            let result = optimizer(registry)
                .optimize(&route, &snapshot, &bounds(fixed, fixed, 8))
                .unwrap();
            assert_eq!(result.route_quote.amount_in, U256::from(fixed));
            assert!(result.tested_amounts <= 8);
        }
    }

    #[test]
    fn skips_route_when_reserve_cap_is_below_minimum_input() {
        let (registry, route, snapshot) = fixture(1_080_000);
        assert!(
            !optimizer(registry)
                .has_profitable_seed(&route, &snapshot, &bounds(2_000_000, 3_000_000, 8))
                .unwrap()
        );
    }

    #[test]
    fn excessive_input_reduces_profit_and_budget_is_hard_limited() {
        let (registry, route, snapshot) = fixture(1_080_000);
        let quoter = LocalRouteQuoter::new(registry.clone(), Arc::new(UniswapV2Adapter::new()));
        let small = quoter
            .quote_exact_in(&route, &snapshot, U256::from(10_000))
            .unwrap();
        let large = quoter
            .quote_exact_in(&route, &snapshot, U256::from(500_000))
            .unwrap();
        assert!(small.gross_profit > large.gross_profit);
        let result = optimizer(registry)
            .optimize(&route, &snapshot, &bounds(10, 500_000, 12))
            .unwrap();
        assert!(result.tested_amounts <= 12);
    }

    #[test]
    fn integer_rounding_can_remove_marginal_opportunity() {
        let (registry, route, snapshot) = fixture(1_010_000);
        let quote = LocalRouteQuoter::new(registry, Arc::new(UniswapV2Adapter::new()))
            .quote_exact_in(&route, &snapshot, U256::from(1))
            .unwrap();
        assert!(quote.amount_out <= quote.amount_in);
    }
}
