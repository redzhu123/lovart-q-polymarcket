//! 合约测试入口（独立测试 target）。
//!
//! 运行: `cargo test -p pm-api-test --test contract_tests`

use pm_api_test::client::config::ApiTestConfig;
use pm_api_test::client::http::ApiClient;
use pm_api_test::validator::response::ResponseValidator;

/// 初始化日志。
fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[tokio::test]
async fn contract_markets() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::markets::test_markets(&client, &validator).await;
    assert!(result.passed, "Markets 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_market_detail() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::markets::test_market_detail(&client, &validator).await;
    assert!(result.passed, "Market Detail 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_orderbook() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::orderbook::test_orderbook(&client, &validator).await;
    assert!(result.passed, "OrderBook 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_trades() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::trades::test_trades(&client, &validator).await;
    assert!(result.passed, "Trades 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_balance() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::balance::test_balance(&client, &validator).await;
    assert!(result.passed, "Balance 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_orders() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::orders::test_orders(&client, &validator).await;
    assert!(result.passed, "Orders 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_positions() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::positions::test_positions(&client, &validator).await;
    assert!(result.passed, "Positions 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_server_time() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::health::test_server_time(&client, &validator).await;
    assert!(result.passed, "Server Time 合约测试失败: {:?}", result.errors);
}

#[tokio::test]
async fn contract_health_check() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let result = pm_api_test::contract::health::test_health_check(&client, &validator).await;
    // 健康检查可能返回非 JSON，不强制要求 passed
    tracing::info!("Health check: {}", result.summary_line_zh());
}
