use std::collections::HashSet;
use std::str::FromStr;

use alloy_primitives::Address;
use futures_util::{StreamExt, TryStreamExt, stream};

use super::config::{DexV2Config, PoolConfig};
use super::connector::ChainConnector;
use super::error::{DexV2Error, DexV2Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiscoveryStats {
    pub factories_queried: usize,
    pub pairs_queried: usize,
    pub pools_discovered: usize,
    pub duplicate_pools: usize,
}

pub async fn discover_configured_v2_pools(
    config: &mut DexV2Config,
    connector: &dyn ChainConnector,
) -> DexV2Result<DiscoveryStats> {
    let mut stats = DiscoveryStats::default();
    let mut registered = config
        .pools
        .iter()
        .map(|pool| parse_address(&pool.address))
        .collect::<DexV2Result<HashSet<_>>>()?;

    #[derive(Clone)]
    struct PairQuery {
        order: usize,
        factory_name: String,
        factory_address_text: String,
        factory_address: Address,
        router: Option<String>,
        fee_numerator: u32,
        fee_denominator: u32,
        left_symbol: String,
        left_address: Address,
        right_symbol: String,
        right_address: Address,
    }

    let mut queries = Vec::new();
    for factory in config.factories.iter().filter(|factory| factory.enabled) {
        stats.factories_queried += 1;
        let factory_address = parse_address(&factory.address)?;
        let mut queried_for_factory = 0usize;
        'pairs: for left in 0..config.tokens.len() {
            for right in left + 1..config.tokens.len() {
                if queried_for_factory >= factory.max_pairs {
                    break 'pairs;
                }
                queried_for_factory += 1;
                stats.pairs_queried += 1;
                let left_token = &config.tokens[left];
                let right_token = &config.tokens[right];
                let left_address = parse_address(&left_token.address)?;
                let right_address = parse_address(&right_token.address)?;
                queries.push(PairQuery {
                    order: queries.len(),
                    factory_name: factory.name.clone(),
                    factory_address_text: factory.address.clone(),
                    factory_address,
                    router: factory.router.clone(),
                    fee_numerator: factory.fee_numerator,
                    fee_denominator: factory.fee_denominator,
                    left_symbol: left_token.symbol.clone(),
                    left_address,
                    right_symbol: right_token.symbol.clone(),
                    right_address,
                });
            }
        }
    }

    // Public RPC latency dominates startup. Bound concurrency to avoid both the previous
    // sequential startup stall and aggressive bursts that trigger free-tier rate limits.
    let mut results = stream::iter(queries.into_iter().map(|query| async move {
        let pair = connector
            .v2_pair(
                query.factory_address,
                query.left_address,
                query.right_address,
            )
            .await?;
        Ok::<_, DexV2Error>((query, pair))
    }))
    .buffer_unordered(4)
    .try_collect::<Vec<_>>()
    .await?;
    results.sort_by_key(|(query, _)| query.order);

    for (query, pair) in results {
        if pair.is_zero() {
            continue;
        }
        if !registered.insert(pair) {
            stats.duplicate_pools += 1;
            continue;
        }
        let (token0, token1) = if query.left_address < query.right_address {
            (query.left_address, query.right_address)
        } else {
            (query.right_address, query.left_address)
        };
        config.pools.push(PoolConfig {
            name: format!(
                "{}_{}_{}",
                query.factory_name.to_ascii_lowercase(),
                query.left_symbol.to_ascii_lowercase(),
                query.right_symbol.to_ascii_lowercase()
            ),
            address: format!("{pair:#x}"),
            factory: query.factory_address_text,
            router: query.router,
            token0: format!("{token0:#x}"),
            token1: format!("{token1:#x}"),
            fee_numerator: query.fee_numerator,
            fee_denominator: query.fee_denominator,
            enabled: true,
        });
        stats.pools_discovered += 1;
    }
    Ok(stats)
}

fn parse_address(value: &str) -> DexV2Result<Address> {
    Address::from_str(value)
        .map_err(|error| DexV2Error::Configuration(format!("invalid address {value}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex_v2::MockConnector;

    #[tokio::test]
    async fn discovers_factory_pairs_and_deduplicates_existing_pools() {
        let mut config: DexV2Config =
            toml::from_str(include_str!("../../../../dex-arbitrage.toml")).unwrap();
        config.factories.truncate(1);
        config.tokens.truncate(3);
        let connector = MockConnector::new(1);
        let factory = parse_address(&config.factories[0].address).unwrap();
        let token_a = parse_address(&config.tokens[0].address).unwrap();
        let token_b = parse_address(&config.tokens[2].address).unwrap();
        let pair = Address::from([9u8; 20]);
        connector.set_v2_pair(factory, token_a, token_b, pair);

        let stats = discover_configured_v2_pools(&mut config, &connector)
            .await
            .unwrap();
        assert_eq!(stats.pools_discovered, 1);
        assert!(
            config
                .pools
                .iter()
                .any(|pool| pool.address == format!("{pair:#x}"))
        );
    }
}
