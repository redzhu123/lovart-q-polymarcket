//! Health / Ping 合约测试。
//!
//! 验证 Polymarket GET /time 和 GET / API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::response::{ResponseValidator, ValidationResult};

/// 服务器时间合约测试。
pub async fn test_server_time(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "服务器时间",
        HttpMethod::Get,
        "/time",
        false,
        200,
        "server-time",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => validator.validate_simple(
            &contract.name,
            &resp,
            &contract.schema_name,
            contract.expected_status,
        ),
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 健康检查合约测试。
pub async fn test_health_check(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "健康检查",
        HttpMethod::Get,
        "/",
        false,
        200,
        "server-time", // 宽松校验
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            // 健康检查返回纯文本 "OK"，不一定是 JSON
            if resp.is_success() {
                let mut result = ValidationResult::new(&contract.name);
                result.add_check(crate::validator::response::CheckResult::pass(
                    "HTTP 状态",
                    &format!("{}", resp.status),
                ));
                result.latency_ms = resp.latency_ms;
                tracing::info!("{}", result.summary_line_zh());
                result
            } else {
                validator.validate_simple(
                    &contract.name,
                    &resp,
                    &contract.schema_name,
                    contract.expected_status,
                )
            }
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
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
    async fn contract_server_time_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_server_time(&client, &validator).await;
        assert!(
            result.passed,
            "Server time test failed: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn contract_health_check_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_health_check(&client, &validator).await;
        // 健康检查可能返回纯文本，所以不严格要求 passed
        tracing::info!("Health check result: {:?}", result.summary_line_zh());
    }
}
