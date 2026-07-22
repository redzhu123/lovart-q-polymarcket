//! 字段级校验器（V1.08）。
//!
//! 提供字段类型、完整性、空值、范围等细粒度校验。
//! 全部输出中文日志。

use serde_json::Value;
use tracing;

/// 字段校验结果。
#[derive(Debug, Clone)]
pub struct FieldCheckResult {
    /// 字段路径（如 "tokens[0].price"）。
    pub field_path: String,
    /// 是否通过。
    pub passed: bool,
    /// 中文描述。
    pub message: String,
}

impl FieldCheckResult {
    pub fn pass(path: &str, msg: &str) -> Self {
        Self {
            field_path: path.to_string(),
            passed: true,
            message: msg.to_string(),
        }
    }

    pub fn fail(path: &str, msg: &str) -> Self {
        Self {
            field_path: path.to_string(),
            passed: false,
            message: msg.to_string(),
        }
    }
}

/// 字段校验器。
pub struct FieldValidator;

impl FieldValidator {
    /// 校验价格范围（0.0 ~ 1.0，Polymarket 价格即概率）。
    pub fn validate_price_range(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if f >= 0.0 && f <= 1.0 {
                        FieldCheckResult::pass(field_path, &format!("价格 {:.4} 在 [0, 1] 范围内", f))
                    } else {
                        FieldCheckResult::fail(
                            field_path,
                            &format!("价格 {:.4} 超出 [0, 1] 范围", f),
                        )
                    }
                } else {
                    FieldCheckResult::fail(field_path, "价格不是有效的 f64 数字")
                }
            }
            Value::String(s) => {
                // Polymarket 价格可能是字符串格式
                match s.parse::<f64>() {
                    Ok(f) => {
                        if f >= 0.0 && f <= 1.0 {
                            FieldCheckResult::pass(
                                field_path,
                                &format!("价格字符串 \"{}\" → {:.4} 在 [0, 1] 范围内", s, f),
                            )
                        } else {
                            FieldCheckResult::fail(
                                field_path,
                                &format!("价格字符串 \"{}\" → {:.4} 超出 [0, 1] 范围", s, f),
                            )
                        }
                    }
                    Err(_) => FieldCheckResult::fail(
                        field_path,
                        &format!("价格字符串 \"{}\" 无法解析为数字", s),
                    ),
                }
            }
            _ => FieldCheckResult::fail(field_path, "价格字段类型不是数字或字符串"),
        }
    }

    /// 校验数量范围（> 0）。
    pub fn validate_quantity(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if f > 0.0 {
                        FieldCheckResult::pass(field_path, &format!("数量 {:.2} > 0", f))
                    } else {
                        FieldCheckResult::fail(field_path, &format!("数量 {:.2} 不大于 0", f))
                    }
                } else {
                    FieldCheckResult::fail(field_path, "数量不是有效的 f64 数字")
                }
            }
            Value::String(s) => {
                match s.parse::<f64>() {
                    Ok(f) => {
                        if f > 0.0 {
                            FieldCheckResult::pass(
                                field_path,
                                &format!("数量字符串 \"{}\" → {:.2} > 0", s, f),
                            )
                        } else {
                            FieldCheckResult::fail(
                                field_path,
                                &format!("数量字符串 \"{}\" → {:.2} 不大于 0", s, f),
                            )
                        }
                    }
                    Err(_) => FieldCheckResult::fail(
                        field_path,
                        &format!("数量字符串 \"{}\" 无法解析为数字", s),
                    ),
                }
            }
            _ => FieldCheckResult::fail(field_path, "数量字段类型不是数字或字符串"),
        }
    }

    /// 校验非空字符串。
    pub fn validate_non_empty_string(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::String(s) => {
                if !s.is_empty() {
                    FieldCheckResult::pass(field_path, "字符串非空")
                } else {
                    FieldCheckResult::fail(field_path, "字符串为空")
                }
            }
            _ => FieldCheckResult::fail(field_path, "字段不是字符串类型"),
        }
    }

    /// 校验 Market ID（condition_id 格式：0x + 64 hex）。
    pub fn validate_market_id(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::String(s) => {
                if s.is_empty() {
                    return FieldCheckResult::fail(field_path, "Market ID 为空字符串");
                }
                if s.starts_with("0x") && s.len() == 66 {
                    let hex_part = &s[2..];
                    if hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
                        return FieldCheckResult::pass(field_path, &format!("Market ID 格式正确: {}", s));
                    }
                }
                // 宽容模式：只检查非空
                FieldCheckResult::pass(
                    field_path,
                    &format!("Market ID: {}（格式宽松通过）", s),
                )
            }
            _ => FieldCheckResult::fail(field_path, "Market ID 不是字符串类型"),
        }
    }

    /// 校验 Token ID。
    pub fn validate_token_id(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::String(s) => {
                if s.is_empty() {
                    FieldCheckResult::fail(field_path, "Token ID 为空字符串")
                } else {
                    FieldCheckResult::pass(field_path, &format!("Token ID: {}", s))
                }
            }
            Value::Number(n) => {
                FieldCheckResult::pass(field_path, &format!("Token ID (数字): {}", n))
            }
            _ => FieldCheckResult::fail(field_path, "Token ID 类型不支持"),
        }
    }

    /// 校验 Outcome（"Yes" | "No"）。
    pub fn validate_outcome(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::String(s) => {
                let upper = s.to_uppercase();
                if upper == "YES" || upper == "NO" {
                    FieldCheckResult::pass(field_path, &format!("Outcome: {}", s))
                } else {
                    FieldCheckResult::fail(
                        field_path,
                        &format!("Outcome \"{}\" 不是 Yes 或 No", s),
                    )
                }
            }
            _ => FieldCheckResult::fail(field_path, "Outcome 不是字符串类型"),
        }
    }

    /// 校验非 null。
    pub fn validate_not_null(value: &Value, field_path: &str) -> FieldCheckResult {
        if value.is_null() {
            FieldCheckResult::fail(field_path, "字段为 null")
        } else {
            FieldCheckResult::pass(field_path, "字段非 null")
        }
    }

    /// 校验时间格式（ISO 8601）。
    pub fn validate_iso8601(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::String(s) => {
                if s.is_empty() {
                    return FieldCheckResult::fail(field_path, "时间字符串为空");
                }
                // 简单检查：包含 T 或 包含 -
                if s.contains('T') || s.contains('-') {
                    FieldCheckResult::pass(field_path, &format!("时间格式: {}", s))
                } else {
                    // 可能是 Unix 时间戳
                    if s.parse::<i64>().is_ok() {
                        FieldCheckResult::pass(field_path, &format!("Unix 时间戳: {}", s))
                    } else {
                        FieldCheckResult::fail(field_path, &format!("时间格式无法识别: {}", s))
                    }
                }
            }
            Value::Number(n) => {
                FieldCheckResult::pass(field_path, &format!("Unix 时间戳 (数字): {}", n))
            }
            _ => FieldCheckResult::fail(field_path, "时间字段类型不支持"),
        }
    }

    /// 校验布尔值存在且为 true/false。
    pub fn validate_boolean(value: &Value, field_path: &str) -> FieldCheckResult {
        match value {
            Value::Bool(b) => {
                FieldCheckResult::pass(field_path, &format!("布尔值: {}", b))
            }
            _ => FieldCheckResult::fail(field_path, "字段不是布尔类型"),
        }
    }

    /// 批量校验数组中的某个字段。
    pub fn validate_array_field<F>(
        array: &Value,
        field_name: &str,
        array_path: &str,
        validator: F,
    ) -> Vec<FieldCheckResult>
    where
        F: Fn(&Value, &str) -> FieldCheckResult,
    {
        let mut results = Vec::new();

        if let Value::Array(items) = array {
            for (i, item) in items.iter().enumerate() {
                let item_path = format!("{}[{}].{}", array_path, i, field_name);
                if let Some(field_value) = item.get(field_name) {
                    results.push(validator(field_value, &item_path));
                } else {
                    results.push(FieldCheckResult::fail(
                        &item_path,
                        &format!("字段 '{}' 缺失", field_name),
                    ));
                }
            }
        }

        results
    }

    /// 打印校验结果摘要。
    pub fn print_results(results: &[FieldCheckResult]) {
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();

        tracing::info!(
            "【字段校验】总计: {} | 通过: {} | 失败: {}",
            results.len(),
            passed,
            failed,
        );

        for r in results.iter().filter(|r| !r.passed) {
            tracing::warn!("  ❌ {} — {}", r.field_path, r.message);
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_price_in_range() {
        let result = FieldValidator::validate_price_range(&Value::Number(serde_json::Number::from_f64(0.45).unwrap()), "price");
        assert!(result.passed);
    }

    #[test]
    fn validate_price_out_of_range() {
        let result = FieldValidator::validate_price_range(&Value::Number(serde_json::Number::from_f64(1.5).unwrap()), "price");
        assert!(!result.passed);
    }

    #[test]
    fn validate_price_string() {
        let result = FieldValidator::validate_price_range(&Value::String("0.45".into()), "price");
        assert!(result.passed);
    }

    #[test]
    fn validate_price_string_out_of_range() {
        let result = FieldValidator::validate_price_range(&Value::String("1.5".into()), "price");
        assert!(!result.passed);
    }

    #[test]
    fn validate_quantity_positive() {
        let result = FieldValidator::validate_quantity(&Value::Number(serde_json::Number::from_f64(100.0).unwrap()), "size");
        assert!(result.passed);
    }

    #[test]
    fn validate_quantity_zero_fails() {
        let result = FieldValidator::validate_quantity(&Value::Number(serde_json::Number::from_f64(0.0).unwrap()), "size");
        assert!(!result.passed);
    }

    #[test]
    fn validate_quantity_negative_fails() {
        let result = FieldValidator::validate_quantity(&Value::Number(serde_json::Number::from_f64(-1.0).unwrap()), "size");
        assert!(!result.passed);
    }

    #[test]
    fn validate_outcome_yes() {
        let result = FieldValidator::validate_outcome(&Value::String("Yes".into()), "outcome");
        assert!(result.passed);
    }

    #[test]
    fn validate_outcome_no() {
        let result = FieldValidator::validate_outcome(&Value::String("No".into()), "outcome");
        assert!(result.passed);
    }

    #[test]
    fn validate_outcome_invalid() {
        let result = FieldValidator::validate_outcome(&Value::String("Maybe".into()), "outcome");
        assert!(!result.passed);
    }

    #[test]
    fn validate_non_empty_string_pass() {
        let result = FieldValidator::validate_non_empty_string(&Value::String("hello".into()), "name");
        assert!(result.passed);
    }

    #[test]
    fn validate_non_empty_string_fail() {
        let result = FieldValidator::validate_non_empty_string(&Value::String("".into()), "name");
        assert!(!result.passed);
    }

    #[test]
    fn validate_market_id_hex() {
        let result = FieldValidator::validate_market_id(
            &Value::String("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".into()),
            "condition_id",
        );
        assert!(result.passed);
    }

    #[test]
    fn validate_market_id_empty_fails() {
        let result = FieldValidator::validate_market_id(&Value::String("".into()), "condition_id");
        assert!(!result.passed);
    }

    #[test]
    fn validate_token_id_non_empty() {
        let result = FieldValidator::validate_token_id(&Value::String("12345".into()), "token_id");
        assert!(result.passed);
    }

    #[test]
    fn validate_not_null_pass() {
        let result = FieldValidator::validate_not_null(&Value::String("data".into()), "field");
        assert!(result.passed);
    }

    #[test]
    fn validate_not_null_fail() {
        let result = FieldValidator::validate_not_null(&Value::Null, "field");
        assert!(!result.passed);
    }

    #[test]
    fn validate_iso8601_string() {
        let result = FieldValidator::validate_iso8601(
            &Value::String("2024-01-01T00:00:00Z".into()),
            "created_at",
        );
        assert!(result.passed);
    }

    #[test]
    fn validate_iso8601_unix_timestamp() {
        let result = FieldValidator::validate_iso8601(
            &Value::Number(serde_json::Number::from(1712345678)),
            "timestamp",
        );
        assert!(result.passed);
    }

    #[test]
    fn validate_array_field_works() {
        let array = serde_json::json!([
            {"price": "0.45"},
            {"price": "0.55"},
            {"price": "1.50"}  // 超出范围
        ]);
        let results = FieldValidator::validate_array_field(
            &array,
            "price",
            "items",
            |v, p| FieldValidator::validate_price_range(v, p),
        );
        assert_eq!(results.len(), 3);
        assert!(results[0].passed);
        assert!(results[1].passed);
        assert!(!results[2].passed);
    }
}
