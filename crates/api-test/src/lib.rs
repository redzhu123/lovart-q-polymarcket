//! pm-api-test：Polymarket API 自动化测试框架（V1.08）。
//!
//! 独立于业务模块的 API 测试 crate。
//! 负责：
//! - 统一 API Client（Mock / Live 双模式）
//! - 响应校验（HTTP / JSON / Schema / 字段）
//! - 合约测试（每个 API 端点）
//! - Live 测试（只读，需网络）
//! - DryRun 订单测试（构建→验证→打印，不发送）
//! - WebSocket 测试
//! - RateLimit 测试
//! - 认证测试
//! - 健康检查
//! - 测试报告生成（Markdown / HTML / JSON）
//!
//! # 安全
//!
//! - 默认 Mock 模式，无网络请求
//! - Live 模式需网络 + `HTTPS_PROXY`（中国用户）
//! - Live 订单测试需 `enable_live=true`
//! - 所有日志使用中文
//!
//! # 快速开始
//!
//! ```ignore
//! use pm_api_test::prelude::*;
//!
//! // Mock 模式
//! let config = ApiTestConfig::mock();
//! let client = ApiClient::new(config);
//! let validator = ResponseValidator::new();
//!
//! // 运行合约测试
//! let result = pm_api_test::contract::markets::test_markets(&client, &validator).await;
//! println!("{}", result.summary_line_zh());
//! ```

pub mod client;
pub mod contract;
pub mod live;
pub mod report;
pub mod utils;
pub mod validator;

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::client::config::{ApiTestConfig, ClientMode};
    pub use crate::client::http::{ApiClient, ApiResponse};
    pub use crate::validator::field::FieldValidator;
    pub use crate::validator::response::{ResponseValidator, ValidationResult};
    pub use crate::validator::schema::JsonSchemaValidator;
    pub use crate::contract::{ContractTest, HttpMethod};
    pub use crate::live::LiveGuard;
    pub use crate::report::generator::ReportGenerator;
    pub use crate::report::types::TestReport;
}

// ============================================================================
// 便利函数
// ============================================================================

/// 创建 Mock 模式的测试环境。
pub fn mock_env() -> (crate::client::http::ApiClient, crate::validator::response::ResponseValidator) {
    use crate::client::config::ApiTestConfig;
    use crate::client::http::ApiClient;
    use crate::validator::response::ResponseValidator;

    // 初始化日志
    crate::utils::logging::init_logging_with_level("info");

    let config = ApiTestConfig::mock();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    (client, validator)
}

/// 创建 Live 模式的测试环境。
pub fn live_env() -> (crate::client::http::ApiClient, crate::validator::response::ResponseValidator) {
    use crate::client::config::ApiTestConfig;
    use crate::client::http::ApiClient;
    use crate::validator::response::ResponseValidator;

    // 初始化日志
    crate::utils::logging::init_logging_with_level("info");

    let config = ApiTestConfig::live();
    let client = ApiClient::new(config);
    let validator = ResponseValidator::new();

    (client, validator)
}

/// 运行所有合约测试（Mock 模式）。
pub async fn run_all_contract_tests(
    client: &crate::client::http::ApiClient,
    validator: &crate::validator::response::ResponseValidator,
) -> Vec<crate::validator::response::ValidationResult> {
    use crate::utils::logging;

    logging::log_section("合约测试套件（Mock 模式）");

    let results = vec![
        contract::health::test_server_time(client, validator).await,
        contract::health::test_health_check(client, validator).await,
        contract::markets::test_markets(client, validator).await,
        contract::markets::test_market_detail(client, validator).await,
        contract::orderbook::test_orderbook(client, validator).await,
        contract::trades::test_trades(client, validator).await,
        contract::balance::test_balance(client, validator).await,
        contract::orders::test_orders(client, validator).await,
        contract::positions::test_positions(client, validator).await,
    ];

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();

    tracing::info!("");
    tracing::info!("═══════════════════════════════════════════════════════════");
    tracing::info!("  合约测试完成: {}/{} 通过", passed, total);
    tracing::info!("═══════════════════════════════════════════════════════════");

    results
}

/// 运行完整测试套件并生成报告。
pub async fn run_full_suite_and_report(
    client: &crate::client::http::ApiClient,
    validator: &crate::validator::response::ResponseValidator,
    report_dir: &str,
) -> anyhow::Result<crate::report::types::TestReport> {
    use crate::report::generator::ReportGenerator;
    use crate::report::types::TestType;

    let mut report = crate::report::types::TestReport::new(TestType::All);

    // 运行所有合约测试
    let results = run_all_contract_tests(client, validator).await;

    // 将结果添加到报告
    for r in &results {
        report.add_endpoint(&r.endpoint, r.passed, 200, r.latency_ms);
    }

    // 运行健康检查
    let health = live::health_check::run_health_check(client, validator).await;
    report.health_score = health.score;

    // 计算摘要
    report.finalize();

    // 生成报告
    let generator = ReportGenerator::new(report_dir);
    generator.generate(&report)?;

    tracing::info!("{}", report.summary_zh());

    Ok(report)
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::client::config::ApiTestConfig;
    use crate::client::http::ApiClient;
    use crate::validator::response::ResponseValidator;

    /// 完整合约测试套件（Mock）。
    #[tokio::test]
    async fn all_contract_tests_mock() {
        crate::utils::logging::init_logging_with_level("warn");

        let (client, validator) = mock_env();
        let results = run_all_contract_tests(&client, &validator).await;

        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert!(
            failed.is_empty(),
            "{} 个合约测试失败:\n{}",
            failed.len(),
            failed
                .iter()
                .map(|r| format!("  - {}: {:?}", r.endpoint, r.errors))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// 健康检查测试（Mock）。
    #[tokio::test]
    async fn health_check_completes() {
        crate::utils::logging::init_logging_with_level("warn");

        let (client, validator) = mock_env();
        let report = live::health_check::run_health_check(&client, &validator).await;

        assert!(report.total > 0);
        assert!(report.score > 0);
    }

    /// DryRun 订单测试。
    #[tokio::test]
    async fn dryrun_order_completes() {
        crate::utils::logging::init_logging_with_level("warn");

        let result = live::order_dryrun::test_dryrun_order().await;
        assert!(result.validation.passed);
    }

    /// Live 订单被安全门拒绝。
    #[tokio::test]
    async fn live_order_rejected_when_disabled() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let guard = crate::live::LiveGuard::new(false);

        let result = live::order_live::test_live_order_flow(&client, &guard).await;
        assert!(result.warnings.iter().any(|w| w.contains("已跳过")));
    }

    /// 报告生成（Mock 数据）。
    #[test]
    fn report_generates_in_temp_dir() {
        use crate::report::generator::ReportGenerator;
        use crate::report::types::{TestReport, TestType};

        let mut report = TestReport::new(TestType::Mock);
        report.add_endpoint("Markets", true, 200, 100);
        report.add_endpoint("OrderBook", true, 200, 80);
        report.finalize();

        let dir = tempfile::tempdir().unwrap();
        let generator = ReportGenerator::new(dir.path().to_str().unwrap());
        let paths = generator.generate(&report).unwrap();

        assert!(std::path::Path::new(&paths.md_path).exists());
        assert!(std::path::Path::new(&paths.html_path).exists());
        assert!(std::path::Path::new(&paths.json_path).exists());
    }

    /// 认证测试套件（Mock）。
    #[tokio::test]
    async fn auth_suite_completes() {
        let (client, validator) = mock_env();
        let results = contract::auth::test_auth_suite(&client, &validator).await;
        assert_eq!(results.len(), 2);
    }

    /// RateLimit 测试（Mock 跳过）。
    #[tokio::test]
    async fn rate_limit_skipped_in_mock() {
        let (client, _) = mock_env();
        let report = live::ratelimit::test_rate_limit(&client, 5).await;
        assert_eq!(report.total_requests, 0);
    }

    /// WebSocket 测试（Mock 跳过）。
    #[tokio::test]
    async fn ws_test_skipped_in_mock() {
        let config = ApiTestConfig::mock();
        let mgr = live::ws::WsTestManager::new(&config);
        let results = mgr.run_all().await;
        assert!(results.iter().all(|r| r.passed));
    }
}
