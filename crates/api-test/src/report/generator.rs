//! 报告生成器（V1.08）。
//!
//! 生成 Markdown / HTML / JSON 三种格式的测试报告。

use std::fs;
use std::path::Path;

use anyhow::Result;
use tracing;

use super::types::TestReport;

/// 生成的文件路径。
#[derive(Debug, Clone)]
pub struct GeneratedPaths {
    pub md_path: String,
    pub html_path: String,
    pub json_path: String,
}

/// 报告生成器。
pub struct ReportGenerator {
    /// 输出目录。
    output_dir: String,
}

impl ReportGenerator {
    /// 创建新的报告生成器。
    pub fn new(output_dir: &str) -> Self {
        // 确保输出目录存在
        if let Err(e) = fs::create_dir_all(output_dir) {
            tracing::warn!("创建报告目录失败: {}", e);
        }

        Self {
            output_dir: output_dir.to_string(),
        }
    }

    /// 生成所有格式的报告。
    pub fn generate(&self, report: &TestReport) -> Result<GeneratedPaths> {
        let ts = report.timestamp.format("%Y%m%d_%H%M%S");
        let base = format!("{}/api-report-{}", self.output_dir, ts);

        let md_path = format!("{}.md", base);
        let html_path = format!("{}.html", base);
        let json_path = format!("{}.json", base);

        self.generate_markdown(report, &md_path)?;
        self.generate_html(report, &html_path)?;
        self.generate_json(report, &json_path)?;

        tracing::info!("");
        tracing::info!("【报告已生成】");
        tracing::info!("  Markdown: {}", md_path);
        tracing::info!("  HTML:     {}", html_path);
        tracing::info!("  JSON:     {}", json_path);

        Ok(GeneratedPaths {
            md_path,
            html_path,
            json_path,
        })
    }

    /// 生成 Markdown 报告。
    fn generate_markdown(&self, report: &TestReport, path: &str) -> Result<()> {
        let mut md = String::new();

        // 标题
        md.push_str(&format!(
            "# Polymarket API 测试报告\n\n\
             **测试类型**: {}  \n\
             **运行时间**: {}  \n\
             **报告 ID**: {}  \n\n",
            report.test_type.as_zh(),
            report.timestamp.format("%Y-%m-%d %H:%M:%S"),
            report.run_id,
        ));

        // 摘要
        md.push_str("## 摘要\n\n");
        md.push_str("| 指标 | 值 |\n|------|----|\n");
        md.push_str(&format!(
            "| 总接口数 | {} |\n",
            report.summary.total_endpoints
        ));
        md.push_str(&format!("| 通过 | {} |\n", report.summary.passed));
        md.push_str(&format!("| 失败 | {} |\n", report.summary.failed));
        md.push_str(&format!(
            "| 平均延迟 | {:.0}ms |\n",
            report.summary.avg_latency_ms
        ));
        md.push_str(&format!(
            "| 最快接口 | {} ({}ms) |\n",
            report.summary.fastest.0, report.summary.fastest.1
        ));
        md.push_str(&format!(
            "| 最慢接口 | {} ({}ms) |\n",
            report.summary.slowest.0, report.summary.slowest.1
        ));
        md.push_str(&format!(
            "| 健康评分 | **{}/100** |\n\n",
            report.health_score
        ));

        // 各端点结果
        md.push_str("## 端点结果\n\n");
        md.push_str("| 接口 | 结果 | HTTP | 延迟 |\n|------|------|------|------|\n");
        for ep in &report.endpoint_results {
            let result = if ep.passed {
                "✅ 通过"
            } else {
                "❌ 失败"
            };
            md.push_str(&format!(
                "| {} | {} | {} | {}ms |\n",
                ep.name, result, ep.status, ep.latency_ms
            ));
        }
        md.push_str("\n");

        // 错误列表
        if !report.summary.errors.is_empty() {
            md.push_str("## 错误\n\n");
            for err in &report.summary.errors {
                md.push_str(&format!("- **{}**: {}\n", err.endpoint, err.message));
            }
            md.push_str("\n");
        }

        // Schema 差异
        if !report.summary.schema_diffs.is_empty() {
            md.push_str("## Schema 差异\n\n");
            for diff in &report.summary.schema_diffs {
                md.push_str(&format!(
                    "- **{}** `{}`: {} → {}\n",
                    diff.endpoint, diff.field_path, diff.diff_type, diff.suggestion
                ));
            }
            md.push_str("\n");
        }

        // 脚注
        md.push_str("---\n\n*由 pm-api-test 自动生成*\n");

        fs::write(path, &md)?;
        tracing::info!("Markdown 报告已写入: {}", path);
        Ok(())
    }

    /// 生成 HTML 报告。
    fn generate_html(&self, report: &TestReport, path: &str) -> Result<()> {
        let mut html = String::new();

        html.push_str(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Polymarket API 测试报告</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 960px; margin: 40px auto; padding: 0 20px; background: #f5f5f5; color: #333; }
        h1 { color: #1a1a2e; border-bottom: 3px solid #6366f1; padding-bottom: 10px; }
        h2 { color: #1a1a2e; margin-top: 30px; }
        table { width: 100%; border-collapse: collapse; margin: 15px 0; background: white; border-radius: 8px; overflow: hidden; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        th { background: #6366f1; color: white; padding: 12px; text-align: left; }
        td { padding: 10px 12px; border-bottom: 1px solid #eee; }
        tr:hover { background: #f8f8ff; }
        .pass { color: #22c55e; font-weight: bold; }
        .fail { color: #ef4444; font-weight: bold; }
        .score { font-size: 2em; font-weight: bold; color: #6366f1; }
        .summary-card { background: white; border-radius: 8px; padding: 20px; margin: 15px 0; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        .summary-card .score { text-align: center; }
        .error-list { background: #fef2f2; border-left: 4px solid #ef4444; padding: 10px 15px; margin: 10px 0; }
        .footer { text-align: center; color: #999; margin-top: 40px; font-size: 0.9em; }
    </style>
</head>
<body>
"#);

        // 标题
        html.push_str(&format!(
            "<h1>Polymarket API 测试报告</h1>\n\
             <p><strong>测试类型</strong>: {} | <strong>运行时间</strong>: {} | <strong>ID</strong>: {}</p>\n",
            report.test_type.as_zh(),
            report.timestamp.format("%Y-%m-%d %H:%M:%S"),
            report.run_id,
        ));

        // 健康评分
        html.push_str(&format!(
            "<div class=\"summary-card\">\n\
             <div class=\"score\">{}/100</div>\n\
             <p style=\"text-align:center\">健康评分</p>\n\
             </div>\n",
            report.health_score
        ));

        // 摘要表
        html.push_str("<h2>摘要</h2>\n<table>\n");
        html.push_str(&format!(
            "<tr><td>总接口数</td><td>{}</td></tr>\n",
            report.summary.total_endpoints
        ));
        html.push_str(&format!(
            "<tr><td>通过</td><td class=\"pass\">{}</td></tr>\n",
            report.summary.passed
        ));
        html.push_str(&format!(
            "<tr><td>失败</td><td class=\"fail\">{}</td></tr>\n",
            report.summary.failed
        ));
        html.push_str(&format!(
            "<tr><td>平均延迟</td><td>{:.0}ms</td></tr>\n",
            report.summary.avg_latency_ms
        ));
        html.push_str(&format!(
            "<tr><td>最快</td><td>{} ({}ms)</td></tr>\n",
            report.summary.fastest.0, report.summary.fastest.1
        ));
        html.push_str(&format!(
            "<tr><td>最慢</td><td>{} ({}ms)</td></tr>\n",
            report.summary.slowest.0, report.summary.slowest.1
        ));
        html.push_str("</table>\n");

        // 端点结果
        html.push_str("<h2>端点结果</h2>\n<table>\n");
        html.push_str("<tr><th>接口</th><th>结果</th><th>HTTP</th><th>延迟</th></tr>\n");
        for ep in &report.endpoint_results {
            let result_class = if ep.passed { "pass" } else { "fail" };
            let result_text = if ep.passed {
                "✅ 通过"
            } else {
                "❌ 失败"
            };
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td><td>{}ms</td></tr>\n",
                ep.name, result_class, result_text, ep.status, ep.latency_ms
            ));
        }
        html.push_str("</table>\n");

        html.push_str("<div class=\"footer\">由 pm-api-test 自动生成</div>\n</body>\n</html>");

        fs::write(path, &html)?;
        tracing::info!("HTML 报告已写入: {}", path);
        Ok(())
    }

    /// 生成 JSON 报告。
    fn generate_json(&self, report: &TestReport, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        fs::write(path, &json)?;
        tracing::info!("JSON 报告已写入: {}", path);
        Ok(())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::types::{ReportSummary, TestType};
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn generate_markdown_report() {
        let mut report = TestReport::new(TestType::Contract);
        report.add_endpoint("Markets", true, 200, 132);
        report.add_endpoint("OrderBook", true, 200, 89);
        report.add_endpoint("Balance", false, 401, 45);
        report.finalize();

        let dir = tempdir().unwrap();
        let generator_test = ReportGenerator::new(dir.path().to_str().unwrap());

        let paths = generator_test.generate(&report).unwrap();
        assert!(std::path::Path::new(&paths.md_path).exists());
        assert!(std::path::Path::new(&paths.html_path).exists());
        assert!(std::path::Path::new(&paths.json_path).exists());
    }

    #[test]
    fn report_summary_calculates_correctly() {
        let mut report = TestReport::new(TestType::Mock);
        report.add_endpoint("Test1", true, 200, 50);
        report.add_endpoint("Test2", true, 200, 100);
        report.add_endpoint("Test3", false, 500, 200);
        report.finalize();

        assert_eq!(report.summary.total_endpoints, 3);
        assert_eq!(report.summary.passed, 2);
        assert_eq!(report.summary.failed, 1);
        assert!(report.summary.avg_latency_ms > 0.0);
        assert!(report.health_score < 100);
    }

    #[test]
    fn health_score_penalizes_failures() {
        let mut report = TestReport::new(TestType::All);
        for i in 0..5 {
            report.add_endpoint(&format!("EP-{}", i), false, 500, 100);
        }
        report.finalize();
        // 5 个失败各扣 10 分
        assert!(report.health_score <= 50);
    }
}
