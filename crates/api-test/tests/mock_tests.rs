//! Mock 测试入口（无网络依赖）。
//!
//! 运行: `cargo test -p pm-api-test --test mock_tests`
//!
//! 这些测试只使用 mock 数据，可以在 CI 中运行。

use pm_api_test::client::config::ApiTestConfig;
use pm_api_test::client::http::ApiClient;
use pm_api_test::validator::response::ResponseValidator;

fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}

/// Mock 模式客户端创建。
#[test]
fn mock_client_creates() {
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    assert!(!client.is_live());
}

/// Mock 模式 GET 返回数据。
#[tokio::test]
async fn mock_get_returns_data() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);

    let resp = client.get("/time").await.unwrap();
    assert!(resp.is_success());
}

/// Mock 模式 POST 返回数据。
#[tokio::test]
async fn mock_post_returns_data() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);

    let body = serde_json::json!({"test": true});
    let resp = client.post("/test", Some(&body)).await.unwrap();
    assert!(resp.is_success());
}

/// JSON Schema 校验器加载所有 Schema。
#[test]
fn schema_validator_loads_all_schemas() {
    let validator = ResponseValidator::new();
    let schemas = validator.schema_validator().list_schemas();
    assert!(schemas.contains(&"markets"));
    assert!(schemas.contains(&"orderbook"));
    assert!(schemas.contains(&"balance"));
    assert!(schemas.contains(&"orders"));
    assert!(schemas.contains(&"positions"));
    assert!(schemas.contains(&"server-time"));
    assert!(schemas.contains(&"trades"));
}

/// DryRun 订单构建和校验。
#[tokio::test]
async fn dryrun_order_validates() {
    init();
    let result = pm_api_test::live::order_dryrun::test_dryrun_order().await;
    assert!(result.validation.passed);
}

/// 健康检查完成。
#[tokio::test]
async fn health_check_mock() {
    init();
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    let report = pm_api_test::live::health_check::run_health_check(&client, &validator).await;
    assert!(report.total > 0);
}

/// Live 订单被安全门拒绝。
#[tokio::test]
async fn live_order_rejected_without_live() {
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let guard = pm_api_test::live::LiveGuard::new(false);

    let result = pm_api_test::live::order_live::test_live_order_flow(&client, &guard).await;
    assert!(result.warnings.iter().any(|w| w.contains("已跳过")));
}

/// RateLimit 测试在 Mock 模式跳过。
#[tokio::test]
async fn ratelimit_skipped_in_mock() {
    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);

    let report = pm_api_test::live::ratelimit::test_rate_limit(&client, 5).await;
    assert_eq!(report.total_requests, 0);
}

/// WebSocket 测试在 Mock 模式跳过。
#[tokio::test]
async fn ws_test_skipped_in_mock() {
    let config = ApiTestConfig::mock();
    let mgr = pm_api_test::live::ws::WsTestManager::new(&config);
    let results = mgr.run_all().await;
    assert!(results.iter().all(|r| r.passed));
}

/// 报告生成。
#[test]
fn report_generates_to_temp_dir() {
    use pm_api_test::report::generator::ReportGenerator;
    use pm_api_test::report::types::{TestReport, TestType};

    let mut report = TestReport::new(TestType::Mock);
    report.add_endpoint("Markets", true, 200, 100);
    report.add_endpoint("OrderBook", true, 200, 80);
    report.finalize();

    let dir = tempfile::tempdir().unwrap();
    let generator_mock = ReportGenerator::new(dir.path().to_str().unwrap());
    let paths = generator_mock.generate(&report).unwrap();

    assert!(std::path::Path::new(&paths.md_path).exists());
    assert!(std::path::Path::new(&paths.html_path).exists());
    assert!(std::path::Path::new(&paths.json_path).exists());

    // 验证 Markdown 内容
    let md_content = std::fs::read_to_string(&paths.md_path).unwrap();
    assert!(md_content.contains("Polymarket"));
    assert!(md_content.contains("Markets"));
}
