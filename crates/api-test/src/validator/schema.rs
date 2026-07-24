//! JSON Schema 校验器（V1.08）。
//!
//! 使用 `jsonschema` crate 进行 JSON Schema Draft 7 校验。
//! 所有 Schema 在编译时通过 `include_str!` 嵌入二进制。

use std::collections::HashMap;

use jsonschema::{Draft, ValidationError, Validator};
use serde_json::Value;
use tracing;

/// Schema 校验结果。
#[derive(Debug, Clone)]
pub struct SchemaResult {
    /// Schema 名称。
    pub schema_name: String,
    /// 是否通过。
    pub passed: bool,
    /// 错误列表。
    pub errors: Vec<String>,
    /// 校验耗时（毫秒）。
    pub duration_ms: u64,
}

impl SchemaResult {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        if self.passed {
            format!(
                "Schema '{}' ✅ 通过 ({}ms)",
                self.schema_name, self.duration_ms
            )
        } else {
            format!(
                "Schema '{}' ❌ 失败 — {} 个错误 ({}ms)",
                self.schema_name,
                self.errors.len(),
                self.duration_ms,
            )
        }
    }
}

/// JSON Schema 校验器。
///
/// 在首次使用时编译所有 Schema（懒加载）。
pub struct JsonSchemaValidator {
    /// 已编译的 Schema 缓存。
    schemas: HashMap<String, Validator>,
}

impl JsonSchemaValidator {
    /// 创建新的 Schema 校验器，加载所有内嵌 Schema。
    pub fn new() -> Self {
        let start = std::time::Instant::now();
        let mut schemas = HashMap::new();

        // 加载并编译所有 Schema
        let schema_sources: &[(&str, &str)] = &[
            ("markets", include_str!("../../schemas/markets.schema.json")),
            (
                "market-detail",
                include_str!("../../schemas/market-detail.schema.json"),
            ),
            (
                "orderbook",
                include_str!("../../schemas/orderbook.schema.json"),
            ),
            ("trades", include_str!("../../schemas/trades.schema.json")),
            ("balance", include_str!("../../schemas/balance.schema.json")),
            ("orders", include_str!("../../schemas/orders.schema.json")),
            (
                "positions",
                include_str!("../../schemas/positions.schema.json"),
            ),
            ("error", include_str!("../../schemas/error.schema.json")),
            (
                "server-time",
                include_str!("../../schemas/server-time.schema.json"),
            ),
        ];

        for (name, source) in schema_sources {
            match Self::compile_schema(source) {
                Ok(schema) => {
                    schemas.insert(name.to_string(), schema);
                    tracing::debug!(schema = %name, "JSON Schema 已加载并编译");
                }
                Err(e) => {
                    tracing::error!(schema = %name, error = %e, "JSON Schema 编译失败");
                }
            }
        }

        let elapsed = start.elapsed().as_millis();
        tracing::info!(
            count = %schemas.len(),
            duration_ms = %elapsed,
            "JSON Schema 校验器已初始化"
        );

        Self { schemas }
    }

    /// 编译单个 Schema。
    fn compile_schema(source: &str) -> Result<Validator, String> {
        let schema_value: Value =
            serde_json::from_str(source).map_err(|e| format!("Schema JSON 解析失败: {}", e))?;

        // jsonschema 0.26 uses Validator::new with draft selection
        let validator = Validator::options()
            .with_draft(Draft::Draft7)
            .build(&schema_value)
            .map_err(|e| format!("Schema 编译失败: {}", e))?;
        Ok(validator)
    }

    /// 校验 JSON 数据是否符合指定 Schema。
    pub fn validate(&self, schema_name: &str, data: &Value) -> SchemaResult {
        let start = std::time::Instant::now();

        match self.schemas.get(schema_name) {
            Some(schema) => {
                let validation_result = schema.validate(data);
                let errors: Vec<String> = match validation_result {
                    Ok(_) => Vec::new(),
                    Err(err) => vec![Self::format_error(&err)],
                };

                let duration_ms = start.elapsed().as_millis() as u64;
                let passed = errors.is_empty();

                if passed {
                    tracing::debug!(
                        schema = %schema_name,
                        duration_ms = %duration_ms,
                        "Schema 校验通过"
                    );
                } else {
                    tracing::warn!(
                        schema = %schema_name,
                        error_count = %errors.len(),
                        duration_ms = %duration_ms,
                        "Schema 校验失败"
                    );
                    for error in &errors {
                        tracing::warn!("  {}", error);
                    }
                }

                SchemaResult {
                    schema_name: schema_name.to_string(),
                    passed,
                    errors,
                    duration_ms,
                }
            }
            None => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let error = format!("Schema '{}' 未注册", schema_name);
                tracing::error!("{}", error);

                SchemaResult {
                    schema_name: schema_name.to_string(),
                    passed: false,
                    errors: vec![error],
                    duration_ms,
                }
            }
        }
    }

    /// 格式化校验错误为中文。
    fn format_error(error: &ValidationError) -> String {
        // jsonschema 0.26 uses Display for human-readable messages
        let message = error.to_string();

        // 将常见关键字转为中文
        let message_zh = if message.contains("required") {
            format!("缺少必填字段: {}", message)
        } else if message.contains("type") {
            format!("类型不匹配: {}", message)
        } else if message.contains("pattern") {
            format!("格式不匹配: {}", message)
        } else if message.contains("enum") {
            format!("枚举值不合法: {}", message)
        } else if message.contains("minimum") {
            format!("小于最小值: {}", message)
        } else if message.contains("maximum") {
            format!("大于最大值: {}", message)
        } else if message.contains("minLength") {
            format!("长度不足: {}", message)
        } else if message.contains("maxLength") {
            format!("长度超出: {}", message)
        } else if message.contains("additionalProperties") {
            format!("包含未定义的额外字段: {}", message)
        } else {
            message
        };

        message_zh
    }

    /// 列出所有已注册的 Schema 名称。
    pub fn list_schemas(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
    }

    /// 检查 Schema 是否已注册。
    pub fn has_schema(&self, name: &str) -> bool {
        self.schemas.contains_key(name)
    }
}

impl Default for JsonSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建一个简单的测试 Schema。
    fn test_schema_json() -> Value {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id", "name"],
            "properties": {
                "id": { "type": "string", "minLength": 1 },
                "name": { "type": "string", "minLength": 1 },
                "price": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
            }
        })
    }

    /// 创建符合 Schema 的测试数据。
    fn valid_test_data() -> Value {
        serde_json::json!({
            "id": "test-001",
            "name": "测试项目",
            "price": 0.45
        })
    }

    /// 创建不符合 Schema 的测试数据。
    fn invalid_test_data() -> Value {
        serde_json::json!({
            "id": "",
            "name": "测试",
            "price": 1.5
        })
    }

    #[test]
    fn validator_loads_all_schemas() {
        let validator = JsonSchemaValidator::new();
        let schemas = validator.list_schemas();
        assert!(schemas.contains(&"markets"));
        assert!(schemas.contains(&"orderbook"));
        assert!(schemas.contains(&"balance"));
        assert!(schemas.contains(&"server-time"));
    }

    #[test]
    fn validate_server_time_schema() {
        let validator = JsonSchemaValidator::new();
        let data = serde_json::json!({"timestamp": 1712345678});
        let result = validator.validate("server-time", &data);
        assert!(
            result.passed,
            "Server time schema should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn validate_invalid_server_time() {
        let validator = JsonSchemaValidator::new();
        let data = serde_json::json!({"timestamp": "not-a-number"});
        let result = validator.validate("server-time", &data);
        assert!(!result.passed);
    }

    #[test]
    fn validate_balance_schema() {
        let validator = JsonSchemaValidator::new();
        let data = serde_json::json!({
            "balance": "10000000000",
            "allowance": "10000000000"
        });
        let result = validator.validate("balance", &data);
        assert!(
            result.passed,
            "Balance schema should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn validate_orderbook_schema() {
        let validator = JsonSchemaValidator::new();
        let data = serde_json::json!({
            "bids": [{"price": "0.43", "size": "100.0"}],
            "asks": [{"price": "0.57", "size": "200.0"}],
            "tick_size": "0.01",
            "neg_risk": false,
            "hash": "0x0000000000000000000000000000000000000000000000000000000000000000"
        });
        let result = validator.validate("orderbook", &data);
        assert!(
            result.passed,
            "OrderBook schema should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn validate_missing_schema_returns_error() {
        let validator = JsonSchemaValidator::new();
        let result = validator.validate("nonexistent", &serde_json::json!({}));
        assert!(!result.passed);
        assert!(result.errors[0].contains("未注册"));
    }

    #[test]
    fn schema_result_summary_zh() {
        let result = SchemaResult {
            schema_name: "test".into(),
            passed: true,
            errors: vec![],
            duration_ms: 10,
        };
        assert!(result.summary_zh().contains("✅"));
    }

    #[test]
    fn list_schemas_returns_all() {
        let validator = JsonSchemaValidator::new();
        let schemas = validator.list_schemas();
        assert!(schemas.len() >= 8);
        assert!(validator.has_schema("markets"));
        assert!(validator.has_schema("orderbook"));
    }
}
