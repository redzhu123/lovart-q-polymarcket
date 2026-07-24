//! Gateway 集成测试（P2-03）。
//!
//! 验证 Mock 和 Polymarket Gateway 的完整生命周期。
//! 所有测试在 Mock 模式下运行（无网络访问）。

use chrono::Local;
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::{
    GatewayConfig, Market, OrderBook, OrderRequest, OrderType, TimeInForce, create_gateway,
    create_mock_gateway, create_polymarket_gateway,
};

// ============================================================================
// 测试辅助
// ============================================================================

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}

fn test_order() -> OrderRequest {
    OrderRequest::new(
        "mkt-1",
        Direction::Yes,
        Side::Buy,
        0.45,
        100.0,
        "S1",
        "R1",
        "O1",
    )
}

// ============================================================================
// Mock Gateway 完整生命周期
// ============================================================================

#[tokio::test]
async fn mock_gateway_full_lifecycle() {
    init_logging();
    let gw = create_mock_gateway();

    // 1. connect
    gw.connect().await.unwrap();

    // 2. 信息
    let info = gw.info();
    assert_eq!(info.gateway_type, "mock");
    assert!(!info.live_enabled);

    // 3. ping
    assert!(gw.ping().await);

    // 4. 健康
    let h = gw.health().await;
    assert!(h.healthy);

    // 5. 余额
    let balance = gw.get_balance().await.unwrap();
    assert!(balance.total > 0.0);

    // 6. 持仓（初始空）
    let positions = gw.get_positions().await.unwrap();
    assert!(positions.is_empty());

    // 7. 下单
    let req = test_order();
    let result = gw.submit_order(&req, Local::now()).await;
    assert!(!result.gateway_order_id.is_empty());

    // 8. 列表订单
    let orders = gw.list_orders().await;
    assert!(!orders.is_empty());

    // 9. 取消
    let cancel = gw.cancel_order(&result.gateway_order_id).await;
    assert!(cancel.success);

    // 10. disconnect
    gw.disconnect().await.unwrap();
}

#[tokio::test]
async fn mock_gateway_get_markets() {
    init_logging();
    let gw = create_mock_gateway();
    gw.connect().await.unwrap();

    let markets = gw.get_markets().await.unwrap();
    assert!(markets.len() >= 3);

    // 验证第一个市场的字段
    let m = &markets[0];
    assert!(!m.market_id.is_empty());
    assert!(!m.question.is_empty());
    assert!(m.yes_price.is_some());

    // 验证市场是 Mock 数据
    assert!(
        m.question.contains("BTC") || m.question.contains("ETH") || m.question.contains("美联储")
    );
}

#[tokio::test]
async fn mock_gateway_get_orderbook() {
    init_logging();
    let gw = create_mock_gateway();
    gw.connect().await.unwrap();

    let ob = gw.get_orderbook("0xmock_market_001").await.unwrap();
    assert_eq!(ob.market_id, "0xmock_market_001");
    assert!(!ob.bids.is_empty());
    assert!(!ob.asks.is_empty());

    let spread = ob.spread().unwrap();
    assert!(spread > 0.0);
    assert!(spread < 0.1);
}

#[tokio::test]
async fn mock_gateway_subscribe_unsubscribe() {
    init_logging();
    let gw = create_mock_gateway();
    gw.connect().await.unwrap();

    gw.subscribe("book:0x1234").await.unwrap();
    gw.unsubscribe("book:0x1234").await.unwrap();
}

#[tokio::test]
async fn mock_gateway_replace_order() {
    init_logging();
    let gw = create_mock_gateway();
    gw.connect().await.unwrap();

    let req = test_order();
    let replace = gw.replace_order("MOCK-OLD-123", &req, Local::now()).await;
    assert!(!replace.gateway_order_id.is_empty());
}

// ============================================================================
// Polymarket Gateway 测试（Mock 模式 + DryRun）
// ============================================================================

#[tokio::test]
async fn polymarket_gateway_dry_run_blocks_orders() {
    init_logging();
    let gw = create_polymarket_gateway();
    assert!(!gw.live_enabled());

    let req = test_order();
    let result = gw.submit_order(&req, Local::now()).await;
    assert!(!result.success);
    assert!(result.message.contains("DryRun"));
}

#[tokio::test]
async fn polymarket_gateway_full_lifecycle_mock() {
    init_logging();
    let cfg = GatewayConfig {
        gateway_type: "polymarket".into(),
        enable_live: false,
        ..GatewayConfig::default()
    };
    let gw = create_gateway(&cfg);

    gw.connect().await.unwrap();

    let info = gw.info();
    assert_eq!(info.gateway_type, "polymarket");

    let markets = gw.get_markets().await.unwrap();
    // Mock 模式会返回真实数据（如果 fixtures 存在）或空
    tracing::info!("Markets count: {}", markets.len());

    let ob = gw.get_orderbook("test-token").await.unwrap();
    // Mock 模式可能返回空数据
    tracing::info!("Orderbook: {}", ob.summary_zh());

    gw.disconnect().await.unwrap();
}

#[tokio::test]
async fn polymarket_gateway_subscribe_unsubscribe() {
    init_logging();
    let gw = create_polymarket_gateway();
    gw.connect().await.unwrap();

    gw.subscribe("book:0xtest").await.unwrap();
    gw.unsubscribe("book:0xtest").await.unwrap();
    gw.disconnect().await.unwrap();
}

// ============================================================================
// Trait 默认实现测试
// ============================================================================

#[tokio::test]
async fn default_trait_implementations() {
    init_logging();

    // 测试 Market summary
    let market = Market {
        market_id: "test-1".into(),
        question: "Test market".into(),
        closed: false,
        yes_price: Some(0.5),
        no_price: Some(0.5),
        volume: 1000.0,
        liquidity: 500.0,
        status: "开放".into(),
    };
    let s = market.summary_zh();
    assert!(s.contains("Test market"));
    assert!(s.contains("0.5000"));

    // 测试 OrderBook summary
    let ob = OrderBook {
        market_id: "test-1".into(),
        bids: vec![pm_gateway::types::BookLevel {
            price: 0.44,
            size: 100.0,
        }],
        asks: vec![pm_gateway::types::BookLevel {
            price: 0.46,
            size: 100.0,
        }],
        tick_size: 0.01,
        updated_at: Some(Local::now()),
    };
    let s = ob.summary_zh();
    assert!(s.contains("0.4400"));
    assert!(s.contains("0.4600"));
    assert!(s.contains("0.0200"));
}

#[tokio::test]
async fn order_request_builders() {
    init_logging();

    let req = OrderRequest::new(
        "mkt-1",
        Direction::Yes,
        Side::Buy,
        0.45,
        100.0,
        "S",
        "R",
        "O",
    )
    .with_order_type(OrderType::Market)
    .with_time_in_force(TimeInForce::Ioc)
    .with_client_order_id("custom-id");

    assert_eq!(req.order_type, OrderType::Market);
    assert_eq!(req.time_in_force, TimeInForce::Ioc);
    assert_eq!(req.client_order_id, "custom-id");
}
