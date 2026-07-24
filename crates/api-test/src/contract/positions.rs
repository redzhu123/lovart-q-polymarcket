//! Positions 合约测试。
//!
//! 验证 Polymarket GET /positions (Data API) 响应。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 持仓列表合约测试。
pub async fn test_positions(client: &ApiClient, validator: &ResponseValidator) -> ValidationResult {
    let contract = ContractTest::new(
        "持仓列表",
        HttpMethod::Get,
        "/positions",
        true,
        200,
        "positions",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => validator.validate(
            &contract.name,
            &resp,
            &contract.schema_name,
            contract.expected_status,
            Some(|body: &Value| validate_positions_fields(body)),
        ),
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验持仓列表字段。
fn validate_positions_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Value::Array(positions) = body {
        for (i, pos) in positions.iter().enumerate() {
            if let Some(size) = pos.get("size") {
                results.push(FieldValidator::validate_quantity(
                    size,
                    &format!("positions[{}].size", i),
                ));
            }
            if let Some(price) = pos.get("current_price") {
                results.push(FieldValidator::validate_price_range(
                    price,
                    &format!("positions[{}].current_price", i),
                ));
            }
            if let Some(avg) = pos.get("avg_price") {
                results.push(FieldValidator::validate_price_range(
                    avg,
                    &format!("positions[{}].avg_price", i),
                ));
            }
        }
    }

    results
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::config::ApiTestConfig;

    #[tokio::test]
    async fn contract_positions_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_positions(&client, &validator).await;
        assert!(
            result.passed,
            "Positions contract test failed: {:?}",
            result.errors
        );
    }
}
