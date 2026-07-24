//! REST API Live 测试（V1.08）。
//!
//! 所有只读 Live 测试。
//! 每个端点一个测试函数，可独立运行。

use crate::client::http::ApiClient;
use crate::validator::response::{ResponseValidator, ValidationResult};
use tracing;

/// 运行所有 REST Live 测试。
pub async fn run_all_rest_tests(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> Vec<ValidationResult> {
    if !client.is_live() {
        tracing::warn!("当前为 Mock 模式，跳过所有 Live 测试");
        return Vec::new();
    }

    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  REST API Live 测试（只读）");
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    let results = vec![
        live_server_time(client, validator).await,
        live_markets(client, validator).await,
        live_orderbook(client, validator).await,
    ];

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    tracing::info!("【REST Live 测试汇总】{}/{} 通过", passed, total);

    results
}

/// Live: 服务器时间。
pub async fn live_server_time(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    tracing::info!("【Live 测试】服务器时间");

    let response = client.get("/time").await;
    match response {
        Ok(resp) => validator.validate_simple("服务器时间(Live)", &resp, "server-time", 200),
        Err(e) => {
            let mut result = ValidationResult::new("服务器时间(Live)");
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// Live: 市场列表。
pub async fn live_markets(client: &ApiClient, validator: &ResponseValidator) -> ValidationResult {
    tracing::info!("【Live 测试】市场列表");

    let response = client.get("/markets").await;
    match response {
        Ok(resp) => validator.validate_simple("市场列表(Live)", &resp, "markets", 200),
        Err(e) => {
            let mut result = ValidationResult::new("市场列表(Live)");
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// Live: 订单簿。
pub async fn live_orderbook(client: &ApiClient, validator: &ResponseValidator) -> ValidationResult {
    tracing::info!("【Live 测试】订单簿");

    // 使用 mock 数据中的 token_id 作为测试
    let token_id = "1111111111111111111111111111111111111111111111111111111111111111";
    let path = format!("/book?token_id={}", token_id);

    let response = client.get(&path).await;
    match response {
        Ok(resp) => validator.validate_simple("订单簿(Live)", &resp, "orderbook", 200),
        Err(e) => {
            let mut result = ValidationResult::new("订单簿(Live)");
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::config::ApiTestConfig;

    #[tokio::test]
    async fn live_tests_skip_in_mock_mode() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let results = run_all_rest_tests(&client, &validator).await;
        // Mock 模式应该跳过所有测试
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn live_server_time_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = live_server_time(&client, &validator).await;
        assert!(result.passed);
    }
}
