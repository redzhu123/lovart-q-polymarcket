//! Trades 合约测试。
//!
//! 验证 Polymarket GET /trades API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 成交记录合约测试。
pub async fn test_trades(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "成交记录",
        HttpMethod::Get,
        "/trades",
        true, // 需要认证
        200,
        "trades",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            validator.validate(
                &contract.name,
                &resp,
                &contract.schema_name,
                contract.expected_status,
                Some(|body: &Value| validate_trades_fields(body)),
            )
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验成交记录字段。
fn validate_trades_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Value::Array(trades) = body {
        for (i, trade) in trades.iter().enumerate() {
            if let Some(price) = trade.get("price") {
                results.push(FieldValidator::validate_price_range(
                    price,
                    &format!("trades[{}].price", i),
                ));
            }
            if let Some(size) = trade.get("size") {
                results.push(FieldValidator::validate_quantity(
                    size,
                    &format!("trades[{}].size", i),
                ));
            }
            if let Some(side) = trade.get("side") {
                let side_str = side.as_str().unwrap_or("");
                if side_str == "BUY" || side_str == "SELL" {
                    results.push(FieldCheckResult::pass(
                        &format!("trades[{}].side", i),
                        &format!("方向: {}", side_str),
                    ));
                } else {
                    results.push(FieldCheckResult::fail(
                        &format!("trades[{}].side", i),
                        &format!("无效方向: {}", side_str),
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
    async fn contract_trades_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_trades(&client, &validator).await;
        assert!(result.passed, "Trades contract test failed: {:?}", result.errors);
    }
}
