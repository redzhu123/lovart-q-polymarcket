//! Live 测试入口（需网络 + 代理）。
//!
//! 运行: `cargo test -p pm-api-test --test live_tests -- --ignored`
//!
//! 所有测试默认 `#[ignore]`，显式运行。
//! 需要设置环境变量:
//! - `HTTPS_PROXY=http://127.0.0.1:7890`（中国用户）
//! - `PM_API_TEST_MODE=live`
//! - `POLYMARKET_API_KEY=<key>`（认证测试）

use pm_api_test::client::config::ApiTestConfig;
use pm_api_test::client::http::ApiClient;
use pm_api_test::validator::response::ResponseValidator;

fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

fn live_client() -> ApiClient {
    let config = ApiTestConfig::live();
    ApiClient::new(config)
}

// ---- REST API Live Tests ----

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_server_time() {
    init();
    let client = live_client();
    let validator = ResponseValidator::new();

    let result = pm_api_test::live::rest::live_server_time(&client, &validator).await;
    assert!(result.passed, "Server time 测试失败: {:?}", result.errors);
}

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_markets() {
    init();
    let client = live_client();
    let validator = ResponseValidator::new();

    let result = pm_api_test::live::rest::live_markets(&client, &validator).await;
    assert!(result.passed, "Markets 测试失败: {:?}", result.errors);
}

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_orderbook() {
    init();
    let client = live_client();
    let validator = ResponseValidator::new();

    let result = pm_api_test::live::rest::live_orderbook(&client, &validator).await;
    assert!(result.passed, "OrderBook 测试失败: {:?}", result.errors);
}

// ---- 健康检查 ----

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_health_check() {
    init();
    let client = live_client();
    let validator = ResponseValidator::new();

    let report = pm_api_test::live::health_check::run_health_check(&client, &validator).await;
    tracing::info!("健康评分: {}/100", report.score);
}

// ---- RateLimit ----

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_rate_limit() {
    init();
    let client = live_client();

    let report = pm_api_test::live::ratelimit::test_rate_limit(&client, 20).await;
    assert!(report.total_requests > 0);
}

// ---- WebSocket ----

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接（可能不支持代理）"]
async fn live_ws_connect() {
    init();
    let config = ApiTestConfig::live();
    let mgr = pm_api_test::live::ws::WsTestManager::new(&config);

    let result = mgr.test_connect().await;
    assert!(result.passed, "WebSocket 连接测试失败: {}", result.detail);
}

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接（可能不支持代理）"]
async fn live_ws_subscribe() {
    init();
    let config = ApiTestConfig::live();
    let mgr = pm_api_test::live::ws::WsTestManager::new(&config);

    let result = mgr.test_subscribe_and_receive().await;
    assert!(result.passed, "WebSocket 订阅测试失败: {}", result.detail);
}

// ---- 报告生成（Live 数据） ----

#[tokio::test]
#[ignore = "Live 测试 — 需要网络连接和 HTTPS_PROXY"]
async fn live_generate_report() {
    init();
    let client = live_client();
    let validator = ResponseValidator::new();

    let report = pm_api_test::run_full_suite_and_report(
        &client,
        &validator,
        "crates/api-test/reports",
    )
    .await
    .unwrap();

    assert!(report.summary.total_endpoints > 0);
    tracing::info!("{}", report.summary_zh());
}
