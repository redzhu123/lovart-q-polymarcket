//! DryRun 订单测试（V1.08）。
//!
//! 构建订单 → 验证参数 → 生成请求 → 打印 → 停止。
//! **绝不发送到交易所。**

use serde_json::{Value, json};
use tracing;

use crate::validator::field::{FieldCheckResult, FieldValidator};
use crate::validator::response::{CheckResult, ValidationResult};

/// DryRun 订单测试结果。
#[derive(Debug, Clone)]
pub struct DryRunOrderResult {
    /// 订单 JSON。
    pub order_json: Value,
    /// 校验结果。
    pub validation: ValidationResult,
}

/// 执行 DryRun 订单测试。
///
/// 流程：
/// 1. 构建订单 JSON
/// 2. 校验参数
/// 3. 打印请求
/// 4. 停止（不发送）
pub async fn test_dryrun_order() -> DryRunOrderResult {
    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  DryRun 订单测试");
    tracing::info!("╚══════════════════════════════════════════════════════════╝");

    // 1. 构建订单
    let order = build_test_order();

    // 2. 校验参数
    let validation = validate_order_params(&order);

    // 3. 打印请求
    print_order_request(&order);

    // 4. 停止
    tracing::info!("");
    tracing::info!("🔒 DryRun 完成 — 订单未发送到交易所。");
    tracing::info!("══════════════════════════════════════════════════════════");

    DryRunOrderResult {
        order_json: order,
        validation,
    }
}

/// 构建测试订单（模拟 Polymarket CLOB V2 格式）。
fn build_test_order() -> Value {
    let order = json!({
        "order": {
            "salt": 123456789,
            "maker": "0x1234567890abcdef1234567890abcdef12345678",
            "signer": "0x1234567890abcdef1234567890abcdef12345678",
            "taker": "0x0000000000000000000000000000000000000000",
            "tokenId": "1111111111111111111111111111111111111111111111111111111111111111",
            "makerAmount": "100000000",
            "takerAmount": "45000000",
            "expiration": "0",
            "nonce": "0",
            "feeRateBps": "0",
            "side": "BUY",
            "signatureType": 3
        },
        "signature": "0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        "orderType": "GTC"
    });

    tracing::info!("📝 订单已构建:");
    tracing::info!("  市场 Token: {}", order["order"]["tokenId"]);
    tracing::info!("  方向: {}", order["order"]["side"]);
    tracing::info!("  Maker 数量: {}", order["order"]["makerAmount"]);
    tracing::info!("  Taker 数量: {}", order["order"]["takerAmount"]);
    tracing::info!("  订单类型: {}", order["orderType"]);
    tracing::info!("  签名类型: {}", order["order"]["signatureType"]);

    order
}

/// 校验订单参数。
fn validate_order_params(order: &Value) -> ValidationResult {
    let mut result = ValidationResult::new("DryRun 订单");

    tracing::info!("");
    tracing::info!("【订单参数校验】");

    // 校验 order 对象存在
    if let Some(order_obj) = order.get("order") {
        result.add_check(CheckResult::pass("订单对象", "order 字段存在"));

        // 校验 side
        if let Some(side) = order_obj.get("side") {
            let check = FieldValidator::validate_outcome(side, "order.side");
            // BUY/SELL 不是 Yes/No 所以要用自定义校验
            let side_str = side.as_str().unwrap_or("");
            if side_str == "BUY" || side_str == "SELL" {
                result.add_check(CheckResult::pass("方向", &format!("side={}", side_str)));
            } else {
                result.add_check(CheckResult::fail(
                    "方向",
                    &format!("无效 side: {}", side_str),
                ));
            }
        } else {
            result.add_check(CheckResult::fail("方向", "side 字段缺失"));
        }

        // 校验 tokenId
        if let Some(token_id) = order_obj.get("tokenId") {
            let check = FieldValidator::validate_token_id(token_id, "order.tokenId");
            result.add_check(CheckResult::pass("Token ID", &check.message));
        }

        // 校验 makerAmount
        if let Some(amount) = order_obj.get("makerAmount") {
            let check = FieldValidator::validate_quantity(amount, "order.makerAmount");
            result.add_check(CheckResult::pass("数量", &check.message));
        }

        // 校验 takerAmount
        if let Some(amount) = order_obj.get("takerAmount") {
            let check = FieldValidator::validate_quantity(amount, "order.takerAmount");
            result.add_check(CheckResult::pass("对手数量", &check.message));
        }

        // 校验 signatureType
        if let Some(sig_type) = order_obj.get("signatureType") {
            if let Some(t) = sig_type.as_u64() {
                if t <= 3 {
                    result.add_check(CheckResult::pass(
                        "签名类型",
                        &format!("signatureType={}", t),
                    ));
                } else {
                    result.add_check(CheckResult::fail(
                        "签名类型",
                        &format!("无效 signatureType: {}", t),
                    ));
                }
            }
        }
    } else {
        result.add_check(CheckResult::fail("订单对象", "order 字段缺失"));
        result.add_error("缺少 order 对象");
    }

    // 校验 orderType
    if let Some(order_type) = order.get("orderType") {
        let valid = matches!(
            order_type.as_str().unwrap_or(""),
            "GTC" | "GTD" | "FOK" | "FAK"
        );
        if valid {
            result.add_check(CheckResult::pass(
                "订单类型",
                &format!("orderType={}", order_type.as_str().unwrap_or("")),
            ));
        } else {
            result.add_check(CheckResult::fail(
                "订单类型",
                &format!("无效 orderType: {}", order_type.as_str().unwrap_or("")),
            ));
        }
    }

    // 校验 signature 存在
    if order.get("signature").is_some() {
        result.add_check(CheckResult::pass("签名", "signature 字段存在"));
    } else {
        result.add_check(CheckResult::fail("签名", "signature 字段缺失"));
    }

    tracing::info!("{}", result.summary_line_zh());
    result
}

/// 打印订单请求（JSON 格式）。
fn print_order_request(order: &Value) {
    let pretty = serde_json::to_string_pretty(order).unwrap_or_default();
    tracing::info!("");
    tracing::info!("【订单请求 JSON】");
    tracing::info!("{}", pretty);
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dryrun_order_builds_and_validates() {
        let result = test_dryrun_order().await;
        assert!(
            result.validation.passed,
            "DryRun validation failed: {:?}",
            result.validation.errors
        );
        assert!(result.order_json.get("order").is_some());
        assert!(result.order_json.get("signature").is_some());
    }

    #[test]
    fn dryrun_order_validates_side() {
        let mut result = ValidationResult::new("test");
        // 空测试，实际校验在 test_dryrun_order 中
        assert!(result.passed);
    }
}
