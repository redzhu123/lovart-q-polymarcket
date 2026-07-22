//! 报告数据类型（V1.08）。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// 测试类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestType {
    Contract,
    Live,
    WebSocket,
    Mock,
    All,
}

impl TestType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            TestType::Contract => "合约测试",
            TestType::Live => "Live 测试",
            TestType::WebSocket => "WebSocket 测试",
            TestType::Mock => "Mock 测试",
            TestType::All => "全部测试",
        }
    }
}

/// 端点测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointResult {
    /// 接口名称。
    pub name: String,
    /// 是否通过。
    pub passed: bool,
    /// HTTP 状态码。
    pub status: u16,
    /// 延迟（毫秒）。
    pub latency_ms: u64,
    /// Schema 错误。
    pub schema_errors: Vec<String>,
    /// 字段错误。
    pub field_errors: Vec<String>,
}

/// 测试错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestError {
    /// 接口名称。
    pub endpoint: String,
    /// 错误消息。
    pub message: String,
}

/// Schema 差异。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// 接口名称。
    pub endpoint: String,
    /// 字段路径。
    pub field_path: String,
    /// 差异类型。
    pub diff_type: String,
    /// 文档值。
    pub doc_value: Option<String>,
    /// 实际值。
    pub actual_value: Option<String>,
    /// 建议。
    pub suggestion: String,
}

/// 报告摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    /// 总接口数。
    pub total_endpoints: u32,
    /// 通过数。
    pub passed: u32,
    /// 失败数。
    pub failed: u32,
    /// 跳过数。
    pub skipped: u32,
    /// 平均延迟（毫秒）。
    pub avg_latency_ms: f64,
    /// 最快接口（名称, 延迟）。
    pub fastest: (String, u64),
    /// 最慢接口（名称, 延迟）。
    pub slowest: (String, u64),
    /// Schema 差异列表。
    pub schema_diffs: Vec<SchemaDiff>,
    /// 错误列表。
    pub errors: Vec<TestError>,
}

/// 完整测试报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    /// 报告 ID。
    pub run_id: String,
    /// 生成时间。
    pub timestamp: DateTime<Local>,
    /// 测试类型。
    pub test_type: TestType,
    /// 摘要。
    pub summary: ReportSummary,
    /// 各端点结果。
    pub endpoint_results: Vec<EndpointResult>,
    /// 健康评分（0-100）。
    pub health_score: u32,
}

impl TestReport {
    /// 创建新的测试报告。
    pub fn new(test_type: TestType) -> Self {
        Self {
            run_id: format!("{}", Local::now().format("%Y%m%d-%H%M%S")),
            timestamp: Local::now(),
            test_type,
            summary: ReportSummary {
                total_endpoints: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
                avg_latency_ms: 0.0,
                fastest: (String::new(), u64::MAX),
                slowest: (String::new(), 0),
                schema_diffs: Vec::new(),
                errors: Vec::new(),
            },
            endpoint_results: Vec::new(),
            health_score: 100,
        }
    }

    /// 添加端点结果。
    pub fn add_endpoint(&mut self, name: &str, passed: bool, status: u16, latency_ms: u64) {
        let result = EndpointResult {
            name: name.to_string(),
            passed,
            status,
            latency_ms,
            schema_errors: Vec::new(),
            field_errors: Vec::new(),
        };
        self.endpoint_results.push(result);
    }

    /// 计算摘要。
    pub fn finalize(&mut self) {
        let total = self.endpoint_results.len() as u32;
        let passed = self.endpoint_results.iter().filter(|r| r.passed).count() as u32;
        let failed = total - passed;

        let latencies: Vec<u64> = self.endpoint_results.iter().map(|r| r.latency_ms).collect();
        let avg_latency = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        };

        let fastest = self.endpoint_results
            .iter()
            .min_by_key(|r| r.latency_ms)
            .map(|r| (r.name.clone(), r.latency_ms))
            .unwrap_or_default();

        let slowest = self.endpoint_results
            .iter()
            .max_by_key(|r| r.latency_ms)
            .map(|r| (r.name.clone(), r.latency_ms))
            .unwrap_or_default();

        // 计算健康评分
        let mut score = 100i32;
        score -= failed as i32 * 10;
        if avg_latency > 3000.0 { score -= 10; }
        if !self.summary.errors.is_empty() { score -= 20; }
        self.health_score = score.max(0) as u32;

        self.summary = ReportSummary {
            total_endpoints: total,
            passed,
            failed,
            skipped: 0,
            avg_latency_ms: avg_latency,
            fastest,
            slowest,
            schema_diffs: std::mem::take(&mut self.summary.schema_diffs),
            errors: std::mem::take(&mut self.summary.errors),
        };
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "【测试报告】{} | 总计: {} | 通过: {} | 失败: {} | 平均延迟: {:.0}ms | 最快: {} ({}ms) | 最慢: {} ({}ms) | 健康评分: {}/100",
            self.test_type.as_zh(),
            self.summary.total_endpoints,
            self.summary.passed,
            self.summary.failed,
            self.summary.avg_latency_ms,
            self.summary.fastest.0,
            self.summary.fastest.1,
            self.summary.slowest.0,
            self.summary.slowest.1,
            self.health_score,
        )
    }
}
