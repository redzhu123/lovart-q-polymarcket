use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use alloy_primitives::{Address, U256};
use pm_arbitrage::dex_v2::{
    AmountBounds, AmountOptimizer, ArbitrageRoute, BoundedRouteGenerator, IntegerSearchOptimizer,
    LocalRouteQuoter, PoolAdapter, PoolId, PoolRegistry, Protocol, RouteGenerationConfig,
    RouteGenerator, RouteQuoter, StateSnapshot, StateVersion, SwapLeg, TokenId, TokenMeta,
    TokenPoolGraph, UniswapV2Adapter, V2Pool, V2PoolState,
};

fn token(last: u8, anchor: bool) -> TokenMeta {
    TokenMeta {
        id: TokenId {
            chain_id: 1,
            address: Address::with_last_byte(last),
        },
        symbol: format!("T{last}"),
        decimals: 18,
        anchor,
    }
}

fn pool(number: u16, a: &TokenId, b: &TokenId) -> V2Pool {
    let mut raw = [0u8; 20];
    raw[18..].copy_from_slice(&number.to_be_bytes());
    V2Pool {
        id: PoolId {
            chain_id: 1,
            address: Address::from(raw),
        },
        name: format!("P{number}"),
        protocol: Protocol::UniswapV2Compatible {
            factory: Address::ZERO,
            router: None,
        },
        token0: a.clone(),
        token1: b.clone(),
        fee_numerator: 997,
        fee_denominator: 1000,
    }
}

fn generation_benchmark(edges_per_pair: u16) {
    let a = token(1, true);
    let b = token(2, false);
    let c = token(3, false);
    let mut pools = Vec::new();
    for index in 0..edges_per_pair {
        pools.push(pool(100 + index, &a.id, &b.id));
        pools.push(pool(200 + index, &b.id, &c.id));
        pools.push(pool(300 + index, &c.id, &a.id));
    }
    let registry = PoolRegistry::new(vec![a.clone(), b.clone(), c.clone()], pools).unwrap();
    let graph = TokenPoolGraph::from_registry(&registry);
    let config = RouteGenerationConfig {
        enable_two_hop: false,
        enable_three_hop: true,
        enable_four_hop: false,
        max_route_hops: 3,
        max_routes_total: 100_000,
        max_routes_per_anchor: 100_000,
        max_edges_per_token_pair: edges_per_pair as usize,
        allowed_anchor_tokens: HashSet::from([a.id.clone()]),
        allowed_intermediate_tokens: HashSet::from([b.id.clone(), c.id.clone()]),
    };
    let started = Instant::now();
    let routes = BoundedRouteGenerator::new(&registry)
        .generate_three_hop_routes(&graph, &config)
        .unwrap();
    println!(
        "生成 {} 条三跳路径耗时 {:?}",
        routes.len(),
        started.elapsed()
    );
    black_box(routes);
}

fn quote_fixture() -> (Arc<PoolRegistry>, ArbitrageRoute, StateSnapshot) {
    let ids = [1u8, 2, 3].map(|last| token(last, last == 1));
    let pools = vec![
        pool(10, &ids[0].id, &ids[1].id),
        pool(11, &ids[1].id, &ids[2].id),
        pool(12, &ids[2].id, &ids[0].id),
    ];
    let registry = Arc::new(PoolRegistry::new(ids.to_vec(), pools.clone()).unwrap());
    let legs = pools
        .iter()
        .enumerate()
        .map(|(index, pool)| SwapLeg {
            index: index as u8,
            pool_id: pool.id.clone(),
            token_in: ids[index].id.clone(),
            token_out: ids[(index + 1) % 3].id.clone(),
        })
        .collect();
    let route = ArbitrageRoute::new(Default::default(), 1, ids[0].id.clone(), legs).unwrap();
    let states = pools
        .iter()
        .enumerate()
        .map(|(index, pool)| {
            (
                pool.id.clone(),
                Arc::new(V2PoolState {
                    reserve0: U256::from(1_000_000_000u64),
                    reserve1: U256::from(if index == 2 {
                        1_080_000_000u64
                    } else {
                        1_000_000_000u64
                    }),
                    block_number: 1,
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
            target_block: 1,
            min_state_block: 1,
            max_state_block: 1,
            state_version: StateVersion {
                block_number: 1,
                max_log_index: 0,
            },
            pools: states,
        },
    )
}

fn main() {
    generation_benchmark(8);
    generation_benchmark(18);
    let (registry, route, snapshot) = quote_fixture();
    let adapter: Arc<dyn PoolAdapter> = Arc::new(UniswapV2Adapter::new());
    let quoter: Arc<dyn RouteQuoter> = Arc::new(LocalRouteQuoter::new(registry.clone(), adapter));
    let started = Instant::now();
    for _ in 0..10_000 {
        black_box(
            quoter
                .quote_exact_in(&route, &snapshot, U256::from(10_000))
                .unwrap(),
        );
    }
    println!("10000 次三跳精确报价耗时：{:?}", started.elapsed());
    let optimizer = IntegerSearchOptimizer::new(registry, quoter);
    let bounds = AmountBounds {
        min_input: U256::from(100),
        max_input: U256::from(100_000_000),
        minimum_search_step: U256::from(10),
        max_pool_reserve_bps: 1000,
        seed_reserve_bps: vec![1, 3, 10, 30, 100, 300, 1000],
        max_quote_evaluations: 64,
        local_search_iterations: 16,
    };
    let started = Instant::now();
    for _ in 0..1_000 {
        black_box(
            optimizer
                .has_profitable_seed(&route, &snapshot, &bounds)
                .unwrap(),
        );
    }
    println!("1000 次三跳种子金额过滤耗时：{:?}", started.elapsed());
    let started = Instant::now();
    for _ in 0..100 {
        black_box(optimizer.optimize(&route, &snapshot, &bounds).unwrap());
    }
    println!("100 次完整三跳优化耗时：{:?}", started.elapsed());
    let started = Instant::now();
    for _ in 0..1_000 {
        black_box(optimizer.optimize(&route, &snapshot, &bounds).unwrap());
    }
    println!(
        "1000 条受影响三跳路径扫描（过滤和优化）耗时：{:?}",
        started.elapsed()
    );
}
