//! Orders 合约测试。
//!
//! 验证 Polymarket GET /orders API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 订单列表合约测试。
pub async fn test_orders(client: &ApiClient, validator: &ResponseValidator) -> ValidationResult {
    let contract = ContractTest::new("订单列表", HttpMethod::Get, "/orders", true, 200, "orders");

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => validator.validate(
            &contract.name,
            &resp,
            &contract.schema_name,
            contract.expected_status,
            Some(|body: &Value| validate_orders_fields(body)),
        ),
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验订单列表字段。
fn validate_orders_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Value::Array(orders) = body {
        for (i, order) in orders.iter().enumerate() {
            if let Some(price) = order.get("price") {
                results.push(FieldValidator::validate_price_range(
                    price,
                    &format!("orders[{}].price", i),
                ));
            }
            if let Some(size) = order.get("original_size") {
                results.push(FieldValidator::validate_quantity(
                    size,
                    &format!("orders[{}].original_size", i),
                ));
            }
            if let Some(status) = order.get("status") {
                let valid = matches!(
                    status.as_str().unwrap_or(""),
                    "LIVE" | "MATCHED" | "CANCELLED" | "EXPIRED" | "CLOSED"
                );
                if valid {
                    results.push(FieldCheckResult::pass(
                        &format!("orders[{}].status", i),
                        &format!("状态: {}", status.as_str().unwrap_or("")),
                    ));
                } else {
                    results.push(FieldCheckResult::fail(
                        &format!("orders[{}].status", i),
                        &format!("无效状态: {}", status.as_str().unwrap_or("")),
                    ));
                }
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
    async fn contract_orders_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_orders(&client, &validator).await;
        assert!(
            result.passed,
            "Orders contract test failed: {:?}",
            result.errors
        );
    }
}
