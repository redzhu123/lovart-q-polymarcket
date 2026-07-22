//! Markets 合约测试。
//!
//! 验证 Polymarket GET /markets 和 GET /market API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 市场列表合约测试。
pub async fn test_markets(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "市场列表",
        HttpMethod::Get,
        "/markets",
        false,
        200,
        "markets",
    );

    // 带字段校验的测试
    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            validator.validate(
                &contract.name,
                &resp,
                &contract.schema_name,
                contract.expected_status,
                Some(|body: &Value| validate_markets_fields(body)),
            )
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 市场详情合约测试。
pub async fn test_market_detail(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "市场详情",
        HttpMethod::Get,
        "/market?condition_id=0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        false,
        200,
        "market-detail",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            validator.validate(
                &contract.name,
                &resp,
                &contract.schema_name,
                contract.expected_status,
                Some(|body: &Value| validate_market_detail_fields(body)),
            )
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验市场列表字段。
fn validate_markets_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Value::Array(markets) = body {
        for (i, market) in markets.iter().enumerate() {
            // 校验 condition_id
            if let Some(id) = market.get("condition_id") {
                results.push(FieldValidator::validate_market_id(
                    id,
                    &format!("markets[{}].condition_id", i),
                ));
            }

            // 校验 question
            if let Some(q) = market.get("question") {
                results.push(FieldValidator::validate_non_empty_string(
                    q,
                    &format!("markets[{}].question", i),
                ));
            }

            // 校验 tokens 中的 price
            if let Some(tokens) = market.get("tokens") {
                results.extend(FieldValidator::validate_array_field(
                    tokens,
                    "price",
                    &format!("markets[{}].tokens", i),
                    |v, p| FieldValidator::validate_price_range(v, p),
                ));

                // 校验 tokens 中的 outcome
                results.extend(FieldValidator::validate_array_field(
                    tokens,
                    "outcome",
                    &format!("markets[{}].tokens", i),
                    |v, p| FieldValidator::validate_outcome(v, p),
                ));
            }
        }
    }

    results
}

/// 校验市场详情字段。
fn validate_market_detail_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Some(id) = body.get("condition_id") {
        results.push(FieldValidator::validate_market_id(id, "condition_id"));
    }

    if let Some(q) = body.get("question") {
        results.push(FieldValidator::validate_non_empty_string(q, "question"));
    }

    if let Some(tokens) = body.get("tokens") {
        results.extend(FieldValidator::validate_array_field(
            tokens,
            "price",
            "tokens",
            |v, p| FieldValidator::validate_price_range(v, p),
        ));

        results.extend(FieldValidator::validate_array_field(
            tokens,
            "outcome",
            "tokens",
            |v, p| FieldValidator::validate_outcome(v, p),
        ));
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
    async fn contract_markets_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_markets(&client, &validator).await;
        assert!(result.passed, "Markets contract test failed: {:?}", result.errors);
    }

    #[tokio::test]
    async fn contract_market_detail_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_market_detail(&client, &validator).await;
        assert!(result.passed, "Market detail test failed: {:?}", result.errors);
    }
}
