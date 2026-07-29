use std::sync::Arc;
use std::time::SystemTime;

use alloy_primitives::U256;
use pm_arbitrage::dex_v2::config::{PoolConfig, TokenConfig};
use pm_arbitrage::dex_v2::{
    DexV2Config, DexV2Engine, MockConnector, PoolUpdate, RouteKind, V2PoolState,
};

#[tokio::test]
async fn mock_sync_produces_deduplicated_three_hop_shadow_opportunity() {
    let mut config: DexV2Config =
        toml::from_str(include_str!("../../../dex-arbitrage.toml")).unwrap();
    config.enabled = true;
    config.routes.enable_two_hop = false;
    config.routes.enable_three_hop = true;
    config.routes.allowed_anchor_tokens = vec!["USDC".into()];
    config.routes.allowed_intermediate_tokens = vec!["WETH".into(), "USDT".into()];
    config.log_query_delay_blocks = 0;
    config.tokens = vec![
        TokenConfig {
            symbol: "USDC".into(),
            address: "0x0000000000000000000000000000000000000001".into(),
            decimals: 6,
            anchor: true,
        },
        TokenConfig {
            symbol: "WETH".into(),
            address: "0x0000000000000000000000000000000000000002".into(),
            decimals: 18,
            anchor: false,
        },
        TokenConfig {
            symbol: "USDT".into(),
            address: "0x0000000000000000000000000000000000000003".into(),
            decimals: 6,
            anchor: false,
        },
    ];
    config.pools = vec![
        test_pool("usdc_weth", 11, 1, 2),
        test_pool("weth_usdt", 12, 2, 3),
        test_pool("usdt_usdc", 13, 3, 1),
    ];
    config.optimizer.min_input = "1000".into();
    config.optimizer.max_input = "100000".into();
    config.max_fee_per_gas = "0".into();
    config.native_price_anchor = "0".into();
    config.min_gross_profit_anchor = "1".into();
    config.min_net_profit_anchor = "1".into();
    config.max_gas_anchor = "1000000000".into();
    config.min_roi_bps = 0;
    config.max_state_block_gap = 1;
    config.risk.max_leg_price_impact_bps = 10_000;
    config.risk.max_total_price_impact_bps = 10_000;
    config.risk.min_three_hop_net_profit = Some("1".into());
    config.risk.min_three_hop_roi_bps = Some(0);

    let engine = Arc::new(DexV2Engine::from_config(config).unwrap());
    let route = engine
        .routes
        .routes
        .values()
        .find(|route| route.kind == RouteKind::ThreeHop)
        .unwrap()
        .clone();
    let connector = MockConnector::new(1);
    for pool in engine.registry.pools() {
        let route_leg = route.legs.iter().find(|leg| leg.pool_id == pool.id);
        let (reserve0, reserve1) = match route_leg {
            Some(leg) if leg.token_in == pool.token0 => {
                (U256::from(1_000_000), U256::from(1_050_000))
            }
            Some(_) => (U256::from(1_050_000), U256::from(1_000_000)),
            None => (U256::from(1_000_000), U256::from(1_000_000)),
        };
        connector.set_state(
            pool.id.clone(),
            V2PoolState {
                reserve0,
                reserve1,
                block_number: 1,
                block_hash: None,
                updated_at: SystemTime::now(),
            },
        );
    }
    engine.initialize(&connector).await.unwrap();
    let startup = engine.scan_all_routes(1).await.unwrap();
    assert!(startup.iter().any(|opportunity| {
        opportunity.route.kind == RouteKind::ThreeHop && opportunity.quote.net_profit.is_positive()
    }));
    let audits = engine.drain_scan_audit().unwrap();
    let route_audit = audits
        .iter()
        .find(|audit| audit.route_id == route.id)
        .expect("selected route must have an audit record");
    let diagnostic = engine
        .route_quote_diagnostic(&route.id, 1, route_audit.quote_amount)
        .await
        .unwrap();
    assert_eq!(diagnostic.route_quote.leg_quotes.len(), 3);
    assert_eq!(
        diagnostic.route_quote.leg_quotes[0].amount_out,
        diagnostic.route_quote.leg_quotes[1].amount_in
    );
    assert_eq!(
        diagnostic.route_quote.leg_quotes[1].amount_out,
        diagnostic.route_quote.leg_quotes[2].amount_in
    );
    assert_eq!(diagnostic.gas_anchor, U256::ZERO);
    assert_eq!(
        diagnostic.expected_final_anchor + diagnostic.risk_buffer,
        diagnostic.route_quote.amount_out
    );
    assert_eq!(
        diagnostic.expected_net_profit + alloy_primitives::I256::from_raw(diagnostic.risk_buffer),
        diagnostic.route_quote.gross_profit
    );

    let trigger_leg = &route.legs[0];
    let trigger_pool = engine.registry.pool(&trigger_leg.pool_id).unwrap();
    let state = if trigger_leg.token_in == trigger_pool.token0 {
        V2PoolState {
            reserve0: U256::from(1_000_000),
            reserve1: U256::from(1_050_000),
            block_number: 2,
            block_hash: None,
            updated_at: SystemTime::now(),
        }
    } else {
        V2PoolState {
            reserve0: U256::from(1_050_000),
            reserve1: U256::from(1_000_000),
            block_number: 2,
            block_hash: None,
            updated_at: SystemTime::now(),
        }
    };
    let update = PoolUpdate {
        pool_id: trigger_pool.id.clone(),
        state,
        log_index: 1,
    };
    let opportunities = engine.process_pool_update(update.clone()).await.unwrap();
    assert!(opportunities.iter().any(|opportunity| {
        opportunity.route.kind == RouteKind::ThreeHop
            && opportunity.route.involved_pools.len() == 3
            && opportunity.quote.leg_quotes.len() == 3
            && opportunity.quote.net_profit.is_positive()
    }));

    let duplicate = engine.process_pool_update(update).await.unwrap();
    assert!(duplicate.is_empty());
    assert_eq!(engine.metrics.snapshot().pool_updates_total, 1);
    assert!(engine.metrics.snapshot().route_checks_deduplicated_total >= 1);

    // A later full scan refreshes every pool at the same target block, so a large gap from the
    // previous cache cannot produce `state block gap ... exceeds 1`.
    connector.set_block(17);
    assert!(engine.sync_once(&connector).await.is_ok());
}

fn test_pool(name: &str, address: u8, token0: u8, token1: u8) -> PoolConfig {
    let (token0, token1) = if token0 < token1 {
        (token0, token1)
    } else {
        (token1, token0)
    };
    PoolConfig {
        name: name.into(),
        address: format!("0x{address:040x}"),
        factory: "0x0000000000000000000000000000000000000099".into(),
        router: None,
        token0: format!("0x{token0:040x}"),
        token1: format!("0x{token1:040x}"),
        fee_numerator: 997,
        fee_denominator: 1000,
        enabled: true,
    }
}
