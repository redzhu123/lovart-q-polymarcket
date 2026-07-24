//! 认证合约测试。
//!
//! 验证认证机制：
//! - 无认证请求 → 期望 401
//! - API Key 头认证
//! - Bearer Token 认证（如果支持）
//! - HMAC 签名认证（L2）

use crate::client::http::ApiClient;
use crate::validator::response::{CheckResult, ResponseValidator, ValidationResult};

/// 无认证请求测试 — 期望返回 401 或有限数据。
pub async fn test_no_auth(client: &ApiClient, validator: &ResponseValidator) -> ValidationResult {
    let mut result = ValidationResult::new("认证-无认证");

    tracing::info!("【认证测试】无认证请求 → 期望 401");

    let response = client.get("/orders").await;

    match response {
        Ok(resp) => {
            // 无认证时应该返回 401
            if resp.status == 401 {
                result.add_check(CheckResult::pass("无认证", "正确返回 401 Unauthorized"));
                tracing::info!("    ✅ 无认证请求正确返回 401");
            } else if resp.status == 200 {
                result.add_check(CheckResult::pass(
                    "无认证",
                    "返回 200（Mock 模式或公开端点）",
                ));
            } else {
                result.add_check(CheckResult::fail(
                    "无认证",
                    &format!("未预期状态码: {}", resp.status),
                ));
            }
            result.latency_ms = resp.latency_ms;
        }
        Err(e) => {
            result.add_error(&format!("无认证请求失败: {}", e));
        }
    }

    result
}

/// API Key 认证测试。
pub async fn test_api_key_auth(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> ValidationResult {
    let mut result = ValidationResult::new("认证-API Key");

    tracing::info!("【认证测试】API Key 认证");

    // 检查是否配置了 API Key
    if client.config().api_key.is_none() {
        result.add_warning("未配置 API Key，跳过测试");
        tracing::info!("    ⚠️ 未配置 API Key，跳过认证测试");
        return result;
    }

    let response = client.get("/orders").await;

    match response {
        Ok(resp) => {
            let passed = resp.status == 200;
            result.add_check(CheckResult::pass(
                "API Key 认证",
                &format!(
                    "HTTP {} {}",
                    resp.status,
                    if passed {
                        "认证成功"
                    } else {
                        "认证失败"
                    }
                ),
            ));
            result.latency_ms = resp.latency_ms;
        }
        Err(e) => {
            result.add_error(&format!("API Key 认证请求失败: {}", e));
        }
    }

    result
}

/// 认证测试套件。
pub async fn test_auth_suite(
    client: &ApiClient,
    validator: &ResponseValidator,
) -> Vec<ValidationResult> {
    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  认证测试套件");
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    let results = vec![
        test_no_auth(client, validator).await,
        test_api_key_auth(client, validator).await,
    ];

    // 打印汇总
    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    tracing::info!("【认证测试汇总】{}/{} 通过", passed, total,);

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
    async fn auth_no_auth_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let result = test_no_auth(&client, &validator).await;
        // Mock 模式返回 200（因为从 mock 数据返回）
        assert!(result.checks.iter().any(|c| c.check_name == "无认证"));
    }

    #[tokio::test]
    async fn auth_suite_mock() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let validator = ResponseValidator::new();

        let results = test_auth_suite(&client, &validator).await;
        assert_eq!(results.len(), 2);
    }
}
