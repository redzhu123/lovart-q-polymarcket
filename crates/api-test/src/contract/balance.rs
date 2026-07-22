//! Balance 合约测试。
//!
//! 验证 Polymarket GET /balances API。

use crate::client::http::ApiClient;
use crate::contract::{ContractTest, HttpMethod};
use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 余额合约测试。
pub async fn test_balance(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let contract = ContractTest::new(
        "账户余额",
        HttpMethod::Get,
        "/balances",
        true, // 需要认证
        200,
        "balance",
    );

    let response = client.get(&contract.path).await;

    match response {
        Ok(resp) => {
            validator.validate(
                &contract.name,
                &resp,
                &contract.schema_name,
                contract.expected_status,
                Some(|body: &Value| validate_balance_fields(body)),
            )
        }
        Err(e) => {
            let mut result = ValidationResult::new(&contract.name);
            result.add_error(&format!("请求失败: {}", e));
            result
        }
    }
}

/// 校验余额字段。
fn validate_balance_fields(body: &Value) -> Vec<FieldCheckResult> {
    let mut results = Vec::new();

    if let Some(balance) = body.get("balance") {
        results.push(FieldValidator::validate_quantity(balance, "balance"));
    }

    if let Some(allowance) = body.get("allowance") {
        let allow_str = allowance.as_str().unwrap_or("0");
        if let Ok(val) = allow_str.parse::<f64>() {
            if val >= 0.0 {
                results.push(FieldCheckResult::pass(
                    "allowance",
                    &format!("授权额度: {}", allow_str),
                ));
            } else {
                results.push(FieldCheckResult::fail(
                    "allowance",
                    &format!("授权额度为负: {}", allow_str),
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
    async fn contract_balance_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_balance(&client, &validator).await;
        assert!(result.passed, "Balance contract test failed: {:?}", result.errors);
    }
}
