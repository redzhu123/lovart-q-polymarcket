//! 日志工具（V1.08）。
//!
//! 统一 tracing 初始化 + 中文日志辅助函数。
//! 禁止使用 println! — 全部使用 tracing。

use tracing_subscriber::EnvFilter;

/// 初始化 tracing 订阅器（中文时间格式 + 环境过滤）。
///
/// # 使用
///
/// ```ignore
/// pm_api_test::utils::logging::init_logging();
/// ```
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

/// 初始化带级别的日志。
pub fn init_logging_with_level(level: &str) {
    let filter = EnvFilter::try_from_env("PM_API_LOG").unwrap_or_else(|_| EnvFilter::new(level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

/// 日志分隔线（中文）。
pub fn log_separator() {
    tracing::info!("══════════════════════════════════════════════════════════");
}

/// 日志小节标题。
pub fn log_section(title: &str) {
    tracing::info!("");
    tracing::info!("┌──────────────────────────────────────────────────────────┐");
    tracing::info!("│  {}", title);
    tracing::info!("└──────────────────────────────────────────────────────────┘");
}

/// 测试开始。
pub fn log_test_start(test_name: &str) {
    tracing::info!("");
    tracing::info!("╔══════════════════════════════════════════════════════════╗");
    tracing::info!("║  测试开始: {}", test_name);
    tracing::info!("╚══════════════════════════════════════════════════════════╝");
}

/// 测试结束。
pub fn log_test_end(test_name: &str, passed: bool) {
    let status = if passed { "✅ 通过" } else { "❌ 失败" };
    tracing::info!("");
    tracing::info!("──────────────────────────────────────────────────────────");
    tracing::info!("  测试结束: {} —— {}", test_name, status);
    tracing::info!("──────────────────────────────────────────────────────────");
}

/// 记录 API 调用。
pub fn log_api_call(method: &str, url: &str, status: u16, latency_ms: u64) {
    let status_icon = if status >= 200 && status < 300 {
        "✅"
    } else {
        "❌"
    };
    tracing::info!(
        "[API] {} {} → HTTP {} {} | 耗时: {}ms",
        method,
        url,
        status,
        status_icon,
        latency_ms
    );
}

/// 记录校验步骤。
pub fn log_check(name: &str, passed: bool, detail: &str) {
    let icon = if passed { "✅" } else { "❌" };
    if detail.is_empty() {
        tracing::info!(
            "    {} {} {}",
            icon,
            name,
            if passed { "通过" } else { "失败" }
        );
    } else {
        tracing::info!(
            "    {} {} {} — {}",
            icon,
            name,
            if passed { "通过" } else { "失败" },
            detail
        );
    }
}

/// 记录错误详情。
pub fn log_error(context: &str, error: &str) {
    tracing::error!("  ❌ {}: {}", context, error);
}

/// 记录警告。
pub fn log_warning(context: &str, msg: &str) {
    tracing::warn!("  ⚠️ {}: {}", context, msg);
}

/// 记录成功。
pub fn log_success(context: &str) {
    tracing::info!("  ✅ {}", context);
}

/// 记录信息。
pub fn log_info(msg: &str) {
    tracing::info!("  ℹ️ {}", msg);
}
