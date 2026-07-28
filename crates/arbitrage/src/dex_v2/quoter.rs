use std::sync::Arc;

use alloy_primitives::{I256, U256};

use super::adapter::PoolAdapter;
use super::error::{DexV2Error, DexV2Result};
use super::graph::PoolRegistry;
use super::types::{
    ArbitrageRoute, LegQuote, PriceImpact, RouteQuote, StateSnapshot, TokenId, V2Pool, V2PoolState,
};

pub trait RouteQuoter: Send + Sync {
    fn quote_exact_in(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        amount_in: U256,
    ) -> DexV2Result<RouteQuote>;
}

pub struct LocalRouteQuoter {
    registry: Arc<PoolRegistry>,
    adapter: Arc<dyn PoolAdapter>,
}

impl LocalRouteQuoter {
    pub fn new(registry: Arc<PoolRegistry>, adapter: Arc<dyn PoolAdapter>) -> Self {
        Self { registry, adapter }
    }
}

impl RouteQuoter for LocalRouteQuoter {
    fn quote_exact_in(
        &self,
        route: &ArbitrageRoute,
        snapshot: &StateSnapshot,
        amount_in: U256,
    ) -> DexV2Result<RouteQuote> {
        if !(2..=4).contains(&route.hop_count()) {
            return Err(DexV2Error::Route(format!(
                "route {} has unsupported hop count {}",
                route.id.0,
                route.hop_count()
            )));
        }
        let mut current_amount = amount_in;
        let mut leg_quotes = Vec::with_capacity(route.hop_count());
        let mut price_impacts = Vec::with_capacity(route.hop_count());
        for leg in &route.legs {
            if current_amount.is_zero() {
                leg_quotes.push(LegQuote {
                    leg_index: leg.index,
                    pool_id: leg.pool_id.clone(),
                    token_in: leg.token_in.clone(),
                    token_out: leg.token_out.clone(),
                    amount_in: U256::ZERO,
                    amount_out: U256::ZERO,
                });
                price_impacts.push(PriceImpact {
                    leg_index: leg.index,
                    expected_marginal_out: U256::ZERO,
                    actual_out: U256::ZERO,
                    impact_bps: 0,
                });
                continue;
            }
            let pool = self.registry.pool(&leg.pool_id)?;
            let state = snapshot.pools.get(&leg.pool_id).ok_or_else(|| {
                DexV2Error::PoolState(format!(
                    "route {} missing pool {} at block {}",
                    route.id.0, leg.pool_id.address, snapshot.target_block
                ))
            })?;
            let quoted = self
                .adapter
                .quote_exact_in(pool, state, &leg.token_in, current_amount)?;
            let expected_marginal_out = marginal_out(pool, state, &leg.token_in, current_amount)?;
            let impact_bps =
                if expected_marginal_out.is_zero() || quoted.amount_out >= expected_marginal_out {
                    0
                } else {
                    let lost = expected_marginal_out - quoted.amount_out;
                    let bps = lost
                        .checked_mul(U256::from(10_000))
                        .ok_or_else(|| DexV2Error::Quote("price impact overflow".into()))?
                        / expected_marginal_out;
                    u32::try_from(bps).unwrap_or(u32::MAX)
                };
            leg_quotes.push(LegQuote {
                leg_index: leg.index,
                pool_id: leg.pool_id.clone(),
                token_in: leg.token_in.clone(),
                token_out: leg.token_out.clone(),
                amount_in: current_amount,
                amount_out: quoted.amount_out,
            });
            price_impacts.push(PriceImpact {
                leg_index: leg.index,
                expected_marginal_out,
                actual_out: quoted.amount_out,
                impact_bps,
            });
            current_amount = quoted.amount_out;
        }
        let gross_profit = if current_amount >= amount_in {
            I256::from_raw(current_amount - amount_in)
        } else {
            -I256::from_raw(amount_in - current_amount)
        };
        Ok(RouteQuote {
            route_id: route.id.clone(),
            block_number: snapshot.target_block,
            amount_in,
            amount_out: current_amount,
            leg_quotes,
            price_impacts,
            gross_profit,
        })
    }
}

pub(crate) fn oriented_reserves(
    pool: &V2Pool,
    state: &V2PoolState,
    token_in: &TokenId,
) -> DexV2Result<(U256, U256)> {
    if token_in == &pool.token0 {
        Ok((state.reserve0, state.reserve1))
    } else if token_in == &pool.token1 {
        Ok((state.reserve1, state.reserve0))
    } else {
        Err(DexV2Error::Quote(format!(
            "token {} is not in pool {}",
            token_in.address, pool.id.address
        )))
    }
}

fn marginal_out(
    pool: &V2Pool,
    state: &V2PoolState,
    token_in: &TokenId,
    amount_in: U256,
) -> DexV2Result<U256> {
    let (reserve_in, reserve_out) = oriented_reserves(pool, state, token_in)?;
    if reserve_in.is_zero() {
        return Err(DexV2Error::Quote("zero input reserve".into()));
    }
    amount_in
        .checked_mul(U256::from(pool.fee_numerator))
        .and_then(|value| value.checked_mul(reserve_out))
        .ok_or_else(|| DexV2Error::Quote("marginal quote overflow".into()))
        .map(|numerator| numerator / U256::from(pool.fee_denominator) / reserve_in)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::SystemTime;

    use alloy_primitives::Address;

    use super::super::adapter::UniswapV2Adapter;
    use super::super::graph::stable_route_id;
    use super::super::types::{PoolId, Protocol, SwapLeg, TokenMeta};
    use super::*;

    #[test]
    fn three_hop_quote_chains_exact_integer_outputs() {
        let ids = [1u8, 2, 3].map(|last| TokenId {
            chain_id: 1,
            address: Address::with_last_byte(last),
        });
        let pairs = [(0usize, 1usize), (1, 2), (2, 0)];
        let pools = pairs
            .iter()
            .enumerate()
            .map(|(index, (a, b))| V2Pool {
                id: PoolId {
                    chain_id: 1,
                    address: Address::with_last_byte(10 + index as u8),
                },
                name: format!("P{index}"),
                protocol: Protocol::UniswapV2Compatible {
                    factory: Address::ZERO,
                    router: None,
                },
                token0: ids[*a].clone(),
                token1: ids[*b].clone(),
                fee_numerator: 997,
                fee_denominator: 1000,
            })
            .collect::<Vec<_>>();
        let registry = Arc::new(
            PoolRegistry::new(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| TokenMeta {
                        id: id.clone(),
                        symbol: format!("T{i}"),
                        decimals: 18,
                        anchor: i == 0,
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
                token_in: ids[pairs[index].0].clone(),
                token_out: ids[pairs[index].1].clone(),
            })
            .collect::<Vec<_>>();
        let route =
            ArbitrageRoute::new(stable_route_id(1, &ids[0], &legs), 1, ids[0].clone(), legs)
                .unwrap();
        let states = pools
            .iter()
            .map(|pool| {
                (
                    pool.id.clone(),
                    Arc::new(V2PoolState {
                        reserve0: U256::from(1_000_000),
                        reserve1: U256::from(1_010_000),
                        block_number: 1,
                        block_hash: None,
                        updated_at: SystemTime::now(),
                    }),
                )
            })
            .collect::<HashMap<_, _>>();
        let snapshot = StateSnapshot {
            chain_id: 1,
            target_block: 1,
            min_state_block: 1,
            max_state_block: 1,
            state_version: super::super::types::StateVersion {
                block_number: 1,
                max_log_index: 0,
            },
            pools: states,
        };
        let adapter = Arc::new(UniswapV2Adapter::new());
        let quoter = LocalRouteQuoter::new(registry.clone(), adapter.clone());
        let quote = quoter
            .quote_exact_in(&route, &snapshot, U256::from(10_000))
            .unwrap();
        let mut expected = U256::from(10_000);
        for (index, pool) in pools.iter().enumerate() {
            expected = adapter
                .quote_exact_in(
                    pool,
                    &snapshot.pools[&pool.id],
                    &ids[pairs[index].0],
                    expected,
                )
                .unwrap()
                .amount_out;
            assert_eq!(quote.leg_quotes[index].amount_out, expected);
        }
        assert_eq!(quote.amount_out, expected);
    }
}
