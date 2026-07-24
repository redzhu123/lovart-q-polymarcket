//! 统一响应校验器（V1.08）。
//!
//! 校验流程：
//! HTTP Status → Content-Type → JSON Parse → Schema → Field Checks
//!
//! 每一步失败都会输出中文日志。

use std::time::Instant;

use serde_json::Value;
use tracing;

use super::field::FieldValidator;
use super::schema::JsonSchemaValidator;
use crate::client::http::ApiResponse;

// ============================================================================
// ValidationResult
// ============================================================================

/// 校验步骤结果。
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// 步骤名称（中文）。
    pub check_name: String,
    /// 是否通过。
    pub passed: bool,
    /// 详细信息（中文）。
    pub detail: String,
}

impl CheckResult {
    pub fn pass(name: &str, detail: &str) -> Self {
        Self {
            check_name: name.to_string(),
            passed: true,
            detail: detail.to_string(),
        }
    }

    pub fn fail(name: &str, detail: &str) -> Self {
        Self {
            check_name: name.to_string(),
            passed: false,
            detail: detail.to_string(),
        }
    }
}

/// 完整校验结果。
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 接口名称。
    pub endpoint: String,
    /// 整体是否通过。
    pub passed: bool,
    /// 各步骤校验结果。
    pub checks: Vec<CheckResult>,
    /// 总耗时（毫秒）。
    pub latency_ms: u64,
    /// 错误列表。
    pub errors: Vec<String>,
    /// 警告列表。
    pub warnings: Vec<String>,
    /// 原始响应体（用于 diff）。
    pub raw_body: Option<String>,
}

impl ValidationResult {
    /// 创建空的校验结果。
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            passed: true,
            checks: Vec::new(),
            latency_ms: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            raw_body: None,
        }
    }

    /// 添加一个检查步骤。
    pub fn add_check(&mut self, check: CheckResult) {
        if !check.passed {
            self.passed = false;
        }
        self.checks.push(check);
    }

    /// 添加错误。
    pub fn add_error(&mut self, error: &str) {
        self.passed = false;
        self.errors.push(error.to_string());
    }

    /// 添加警告。
    pub fn add_warning(&mut self, warning: &str) {
        self.warnings.push(warning.to_string());
    }

    /// 中文单行摘要。
    pub fn summary_line_zh(&self) -> String {
        let status_icon = if self.passed { "✅" } else { "❌" };
        let checks_str: Vec<String> = self
            .checks
            .iter()
            .map(|c| {
                let icon = if c.passed { "✅" } else { "❌" };
                format!("{}:{}", c.check_name, icon)
            })
            .collect();

        format!(
            "【接口验证】接口: {} | {} | 耗时: {}ms | 结果: {}",
            self.endpoint,
            checks_str.join(" | "),
            self.latency_ms,
            if self.passed { "通过" } else { "失败" },
        )
    }

    /// 中文详细报告。
    pub fn detailed_report_zh(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!(
            "═══════════════════════════════════════════════════════════\n"
        ));
        report.push_str(&format!("【接口验证报告】{}\n", self.endpoint));
        report.push_str(&format!(
            "───────────────────────────────────────────────────────────\n"
        ));

        for check in &self.checks {
            let icon = if check.passed { "✅" } else { "❌" };
            report.push_str(&format!(
                "  {} {}: {}\n",
                icon, check.check_name, check.detail
            ));
        }

        if !self.errors.is_empty() {
            report.push_str(&format!(
                "───────────────────────────────────────────────────────────\n"
            ));
            report.push_str("错误列表:\n");
            for error in &self.errors {
                report.push_str(&format!("  ❌ {}\n", error));
            }
        }

        if !self.warnings.is_empty() {
            report.push_str("警告:\n");
            for warning in &self.warnings {
                report.push_str(&format!("  ⚠️ {}\n", warning));
            }
        }

        report.push_str(&format!(
            "───────────────────────────────────────────────────────────\n"
        ));
        report.push_str(&format!(
            "总耗时: {}ms | 结果: {}\n",
            self.latency_ms,
            if self.passed {
                "✅ 通过"
            } else {
                "❌ 失败"
            },
        ));
        report.push_str(&format!(
            "═══════════════════════════════════════════════════════════\n"
        ));

        report
    }
}

// ============================================================================
// ResponseValidator
// ============================================================================

/// 统一响应校验器。
///
/// 按顺序执行：HTTP 状态 → Content-Type → JSON 解析 → Schema → 字段校验。
pub struct ResponseValidator {
    /// Schema 校验器。
    schema_validator: JsonSchemaValidator,
}

impl ResponseValidator {
    /// 创建新的响应校验器。
    pub fn new() -> Self {
        Self {
            schema_validator: JsonSchemaValidator::new(),
        }
    }

    /// 完整校验 API 响应。
    ///
    /// # 参数
    ///
    /// - `endpoint`: 接口名称（如 "Markets"）。
    /// - `response`: API 响应。
    /// - `schema_name`: JSON Schema 名称（如 "markets"）。
    /// - `expected_status`: 期望的 HTTP 状态码。
    /// - `field_checks`: 可选的字段级校验闭包。
    pub fn validate<F>(
        &self,
        endpoint: &str,
        response: &ApiResponse,
        schema_name: &str,
        expected_status: u16,
        field_checks: Option<F>,
    ) -> ValidationResult
    where
        F: FnOnce(&Value) -> Vec<super::field::FieldCheckResult>,
    {
        let start = Instant::now();
        let mut result = ValidationResult::new(endpoint);

        // 保存原始响应体
        result.raw_body = Some(serde_json::to_string_pretty(&response.body).unwrap_or_default());

        tracing::info!("");
        tracing::info!("┌──────────────────────────────────────────────────────────┐");
        tracing::info!("│  【接口验证】{}", endpoint);
        tracing::info!("└──────────────────────────────────────────────────────────┘");

        // 1. HTTP 状态码检查
        self.check_http_status(&mut result, response, expected_status);

        // 2. Content-Type 检查
        self.check_content_type(&mut result, response);

        // 3. JSON 解析检查（已在 ApiResponse 中解析，这里检查是否为有效 JSON）
        self.check_json_valid(&mut result, response);

        // 4. JSON Schema 校验
        self.check_schema(&mut result, schema_name, &response.body);

        // 5. 字段级校验
        if let Some(checks) = field_checks {
            self.check_fields(&mut result, &response.body, checks);
        }

        result.latency_ms = start.elapsed().as_millis() as u64;

        // 打印摘要
        tracing::info!("{}", result.summary_line_zh());

        result
    }

    /// 1. HTTP 状态码检查。
    fn check_http_status(
        &self,
        result: &mut ValidationResult,
        response: &ApiResponse,
        expected: u16,
    ) {
        let passed = response.status == expected;
        let detail = format!("期望 {} → 实际 {}", expected, response.status,);

        if passed {
            tracing::info!("    ✅ HTTP 状态: {}", detail);
            result.add_check(CheckResult::pass("HTTP 状态", &detail));
        } else {
            tracing::warn!("    ❌ HTTP 状态: {}", detail);
            result.add_check(CheckResult::fail("HTTP 状态", &detail));
        }
    }

    /// 2. Content-Type 检查。
    fn check_content_type(&self, result: &mut ValidationResult, response: &ApiResponse) {
        let content_type = response
            .headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == "content-type")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        let passed = content_type.contains("application/json") || content_type.is_empty();
        // 空 content-type 可能是 Mock 模式，宽容处理

        if passed {
            let detail = if content_type.is_empty() {
                "Content-Type 未设置（Mock 模式）".to_string()
            } else {
                format!("Content-Type: {}", content_type)
            };
            tracing::info!("    ✅ Content-Type: {}", detail);
            result.add_check(CheckResult::pass("Content-Type", &detail));
        } else {
            let detail = format!("期望 application/json，实际: {}", content_type);
            tracing::warn!("    ❌ Content-Type: {}", detail);
            result.add_check(CheckResult::fail("Content-Type", &detail));
        }
    }

    /// 3. JSON 有效性检查。
    fn check_json_valid(&self, result: &mut ValidationResult, response: &ApiResponse) {
        // ApiResponse.body 已经是解析后的 JSON Value
        // 如果解析失败，body 会是 Value::String(原始文本)
        match &response.body {
            Value::String(s) if !s.is_empty() => {
                // 尝试重新解析
                match serde_json::from_str::<Value>(s) {
                    Ok(_) => {
                        tracing::info!("    ✅ JSON: 解析成功");
                        result.add_check(CheckResult::pass("JSON 解析", "JSON 格式有效"));
                    }
                    Err(e) => {
                        let detail = format!("JSON 解析失败: {}", e);
                        tracing::warn!("    ❌ JSON: {}", detail);
                        result.add_check(CheckResult::fail("JSON 解析", &detail));
                    }
                }
            }
            _ => {
                // 已经是解析后的 JSON
                tracing::info!("    ✅ JSON: 解析成功");
                result.add_check(CheckResult::pass("JSON 解析", "JSON 格式有效"));
            }
        }
    }

    /// 4. JSON Schema 校验。
    fn check_schema(&self, result: &mut ValidationResult, schema_name: &str, body: &Value) {
        let schema_result = self.schema_validator.validate(schema_name, body);

        if schema_result.passed {
            tracing::info!("    ✅ Schema: 通过 ({}ms)", schema_result.duration_ms);
            result.add_check(CheckResult::pass(
                "Schema",
                &format!(
                    "符合 {}.schema.json ({}ms)",
                    schema_name, schema_result.duration_ms
                ),
            ));
        } else {
            let detail = format!("{} 个错误", schema_result.errors.len());
            tracing::warn!("    ❌ Schema: {}", detail);
            for error in &schema_result.errors {
                tracing::warn!("      ↳ {}", error);
            }
            result.add_check(CheckResult::fail("Schema", &detail));
            for error in &schema_result.errors {
                result.add_error(error);
            }
        }
    }

    /// 5. 字段级校验。
    fn check_fields(
        &self,
        result: &mut ValidationResult,
        body: &Value,
        field_checks: impl FnOnce(&Value) -> Vec<super::field::FieldCheckResult>,
    ) {
        let checks = field_checks(body);
        let passed_count = checks.iter().filter(|c| c.passed).count();
        let total = checks.len();

        if passed_count == total {
            tracing::info!("    ✅ 字段: {}/{} 通过", passed_count, total);
            result.add_check(CheckResult::pass(
                "字段",
                &format!("{}/{} 个字段校验通过", passed_count, total),
            ));
        } else {
            tracing::warn!("    ❌ 字段: {}/{} 通过", passed_count, total);
            result.add_check(CheckResult::fail(
                "字段",
                &format!(
                    "{}/{} 通过，{} 个失败",
                    passed_count,
                    total,
                    total - passed_count
                ),
            ));
        }

        for check in &checks {
            if !check.passed {
                result.add_error(&format!("[{}] {}", check.field_path, check.message));
            }
        }
    }

    /// 简化版校验（无字段校验闭包）。
    pub fn validate_simple(
        &self,
        endpoint: &str,
        response: &ApiResponse,
        schema_name: &str,
        expected_status: u16,
    ) -> ValidationResult {
        type FieldCheckFn = fn(&Value) -> Vec<super::field::FieldCheckResult>;
        self.validate::<FieldCheckFn>(endpoint, response, schema_name, expected_status, None)
    }

    /// 获取 Schema 校验器引用。
    pub fn schema_validator(&self) -> &JsonSchemaValidator {
        &self.schema_validator
    }
}

impl Default for ResponseValidator {
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
    use crate::client::http::ApiResponse;

    fn mock_response(status: u16, body: Value) -> ApiResponse {
        ApiResponse {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body,
            latency_ms: 50,
            url: "/test".into(),
        }
    }

    #[test]
    fn validation_result_new() {
        let result = ValidationResult::new("测试接口");
        assert!(result.passed);
        assert_eq!(result.endpoint, "测试接口");
    }

    #[test]
    fn validation_result_add_check_failure() {
        let mut result = ValidationResult::new("测试");
        result.add_check(CheckResult::fail("HTTP", "400 错误"));
        assert!(!result.passed);
    }

    #[test]
    fn validation_result_summary_zh() {
        let mut result = ValidationResult::new("Markets");
        result.add_check(CheckResult::pass("HTTP 状态", "200"));
        result.add_check(CheckResult::pass("Schema", "通过"));
        result.latency_ms = 132;
        let summary = result.summary_line_zh();
        assert!(summary.contains("Markets"));
        assert!(summary.contains("132ms"));
    }

    #[test]
    fn validation_result_detailed_report() {
        let mut result = ValidationResult::new("测试");
        result.add_check(CheckResult::pass("HTTP 状态", "200 ✅"));
        result.add_check(CheckResult::fail("Schema", "缺少必填字段"));
        result.add_error("condition_id 缺失");
        let report = result.detailed_report_zh();
        assert!(report.contains("测试"));
        assert!(report.contains("失败"));
    }

    #[test]
    fn response_validator_creates() {
        let validator = ResponseValidator::new();
        assert!(validator.schema_validator().has_schema("markets"));
    }

    #[test]
    fn validate_success_response() {
        let validator = ResponseValidator::new();
        let resp = mock_response(200, serde_json::json!({"timestamp": 1712345678}));
        let result = validator.validate(
            "Time",
            &resp,
            "server-time",
            200,
            None::<fn(&Value) -> Vec<crate::validator::field::FieldCheckResult>>,
        );
        assert!(result.passed);
        assert!(result.summary_line_zh().contains("Time"));
    }

    #[test]
    fn validate_wrong_status_fails() {
        let validator = ResponseValidator::new();
        let resp = mock_response(500, serde_json::json!({"timestamp": 1712345678}));
        let result = validator.validate(
            "Time",
            &resp,
            "server-time",
            200,
            None::<fn(&Value) -> Vec<crate::validator::field::FieldCheckResult>>,
        );
        assert!(!result.passed);
    }
}
