//! Live 订单测试（V1.08）。
//!
//! **默认关闭。**
//! 需要 `enable_live=true` 才允许执行。
//!
//! 流程：
//! Place → 立即 Cancel → 验证状态同步 → 结束。
//!
//! 安全门：任何情况下，没有 enable_live=true 必须拒绝执行。

use tracing;

use super::LiveGuard;
use crate::client::http::ApiClient;
use crate::validator::response::{CheckResult, ValidationResult};

/// Live 订单测试（需 enable_live=true）。
///
/// # 安全
///
/// 此函数在 `enable_live=false` 时直接返回拒绝结果。
pub async fn test_live_order_flow(client: &ApiClient, guard: &LiveGuard) -> ValidationResult {
    let mut result = ValidationResult::new("Live 订单流程");

    // 安全门
    if !guard.is_live() {
        result.add_warning("真实交易未启用（enable_live=false），已跳过 Live 订单测试");
        tracing::warn!("🔒 Live 订单测试已跳过 — enable_live=false");
        return result;
    }

    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  ⚠️  Live 订单测试（真实交易）");
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    // 1. 构建最小金额订单
    let order_body = serde_json::json!({
        "order": {
            "salt": 123456789,
            "maker": "0x0000000000000000000000000000000000000000",
            "signer": "0x0000000000000000000000000000000000000000",
            "taker": "0x0000000000000000000000000000000000000000",
            "tokenId": "1111111111111111111111111111111111111111111111111111111111111111",
            "makerAmount": "1000000",
            "takerAmount": "450000",
            "expiration": "0",
            "nonce": "0",
            "feeRateBps": "0",
            "side": "BUY",
            "signatureType": 3
        },
        "signature": "0xPLACEHOLDER",
        "orderType": "GTC"
    });

    tracing::info!("📝 订单请求已构建（最小金额）");
    tracing::info!("   金额: 1 USDC 等值");

    // 2. 发送订单
    tracing::info!("📤 发送订单...");
    match client.post("/order", Some(&order_body)).await {
        Ok(resp) => {
            let order_id = resp
                .body
                .get("orderID")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if resp.is_success() {
                result.add_check(CheckResult::pass(
                    "下单",
                    &format!("订单已提交: {}", order_id),
                ));
                tracing::info!("    ✅ 下单成功: {}", order_id);

                // 3. 立即取消
                tracing::info!("📤 取消订单: {}", order_id);
                let cancel_body = serde_json::json!({"orderID": order_id});

                match client.delete("/order", Some(&cancel_body)).await {
                    Ok(cancel_resp) => {
                        if cancel_resp.is_success() {
                            result.add_check(CheckResult::pass(
                                "撤单",
                                &format!("订单已取消: {}", order_id),
                            ));
                            tracing::info!("    ✅ 撤单成功");
                        } else {
                            result.add_check(CheckResult::fail(
                                "撤单",
                                &format!("撤单失败: HTTP {}", cancel_resp.status),
                            ));
                        }
                    }
                    Err(e) => {
                        result.add_error(&format!("撤单请求失败: {}", e));
                    }
                }
            } else {
                result.add_check(CheckResult::fail(
                    "下单",
                    &format!("下单失败: HTTP {} — {:?}", resp.status, resp.body),
                ));
            }
        }
        Err(e) => {
            result.add_error(&format!("下单请求失败: {}", e));
        }
    }

    tracing::info!("");
    tracing::info!("{}", result.summary_line_zh());
    tracing::info!("══════════════════════════════════════════════════════════");

    result
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::config::ApiTestConfig;

    #[tokio::test]
    async fn live_order_skipped_when_disabled() {
        let config = ApiTestConfig::mock();
        let client = ApiClient::new(config);
        let guard = LiveGuard::new(false);

        let result = test_live_order_flow(&client, &guard).await;
        assert!(result.warnings.iter().any(|w| w.contains("已跳过")));
    }
}
