//! OrderBook 合约测试。
//!
//! 验证 Polymarket GET /book API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 订单簿合约测试。
pub async fn test_orderbook(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "订单簿",
        HttpMethod::Get,
        "/book?token_id=1111111111111111111111111111111111111111111111111111111111111111",
        false,
        200,
        "orderbook",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            validator.validate(
                &contract.name,
                &resp,
                &contract.schema_name,
                contract.expected_status,
                Some(|body: &Value| validate_orderbook_fields(body)),
            )
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验订单簿字段。
fn validate_orderbook_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    // 校验 bids
    if let Some(bids) = body.get("bids") {
        results.extend(FieldValidator::validate_array_field(
            bids,
            "price",
            "bids",
            |v, p| FieldValidator::validate_price_range(v, p),
        ));

        results.extend(FieldValidator::validate_array_field(
            bids,
            "size",
            "bids",
            |v, p| FieldValidator::validate_quantity(v, p),
        ));
    }

    // 校验 asks
    if let Some(asks) = body.get("asks") {
        results.extend(FieldValidator::validate_array_field(
            asks,
            "price",
            "asks",
            |v, p| FieldValidator::validate_price_range(v, p),
        ));

        results.extend(FieldValidator::validate_array_field(
            asks,
            "size",
            "asks",
            |v, p| FieldValidator::validate_quantity(v, p),
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
    async fn contract_orderbook_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_orderbook(&client, &validator).await;
        assert!(result.passed, "OrderBook contract test failed: {:?}", result.errors);
    }
}
