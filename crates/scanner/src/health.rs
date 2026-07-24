//! pm-scanner::health：启动健康检查（V1.01 第十节）。
//!
//! **只增强可观测性，不改变任何交易/策略逻辑。**
//!
//! 检查项：Config / CSV / Storage / Clock / Memory / API / JSON。任一 Fail 即不应继续启动
//!（由 driver 在扫描循环前调用并 `bail!`）。Memory 在非 Windows 平台降级为 Warn 跳过。
//!
//! V1.02：API + JSON 检查不再直接访问 HTTP，改为复用 [`crate::datasource::MarketDataProvider::health_check`]。
//! 一次探测同时产出 API（HTTP 状态）与 JSON（解析结果）两项检查。

use chrono::Local;

use pm_models::Config;

use crate::datasource::{HealthProbe, MarketDataProvider};
use crate::display::{DASH, SEP};

/// 单项检查结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    fn emoji(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "🟢",
            CheckStatus::Warn => "🟡",
            CheckStatus::Fail => "🔴",
        }
    }

    fn zh(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "正常",
            CheckStatus::Warn => "警告",
            CheckStatus::Fail => "失败",
        }
    }
}

/// 单项检查结果。
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

/// 健康检查报告。
#[derive(Debug, Clone, Default)]
pub struct HealthReport {
    pub checks: Vec<CheckResult>,
}

impl HealthReport {
    /// 全部通过（无 Fail）。Warn 不阻断启动。
    pub fn all_pass(&self) -> bool {
        self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }

    fn push(&mut self, name: &str, status: CheckStatus, detail: String) {
        self.checks.push(CheckResult {
            name: name.into(),
            status,
            detail,
        });
    }
}

/// 运行完整健康检查（Config / CSV / Storage / Clock / Memory / API / JSON）。
///
/// 始终返回 `HealthReport`；各项失败记录为 `CheckStatus::Fail`，由调用方决定是否阻断。
/// API 与 JSON 共用一次 `provider.health_check()` 探测：先判 HTTP 状态，再判 JSON 解析。
pub async fn run_health_check(provider: &dyn MarketDataProvider, cfg: &Config) -> HealthReport {
    let mut report = HealthReport::default();

    let (s, d) = check_config(cfg).await;
    report.push("配置", s, d);

    let (s, d) = check_csv(cfg).await;
    report.push("CSV", s, d);

    let (s, d) = check_storage(cfg).await;
    report.push("存储", s, d);

    let (s, d) = check_clock().await;
    report.push("时钟", s, d);

    let (s, d) = check_memory().await;
    report.push("内存", s, d);

    // API + JSON 共用一次 provider 探测（V1.02：不再直接 HTTP）
    let probe = match provider.health_check().await {
        Ok(p) => p,
        Err(e) => HealthProbe {
            ok: false,
            status: 0,
            market_count: 0,
            latency_ms: 0,
            detail: format!("探测失败: {:#}", e),
        },
    };
    let (api_status, api_detail) = check_api(&probe);
    report.push("API", api_status, api_detail);
    let (json_status, json_detail) = check_json(&probe);
    report.push("JSON", json_status, json_detail);

    report
}

async fn check_config(cfg: &Config) -> (CheckStatus, String) {
    if cfg.scanner.scan_interval_secs == 0 {
        return (CheckStatus::Fail, "scanner.scan_interval_secs == 0".into());
    }
    if cfg.scanner.opportunity_threshold <= 0.0 || cfg.scanner.opportunity_threshold > 2.0 {
        return (
            CheckStatus::Fail,
            "scanner.opportunity_threshold 超出范围 (0, 2]".into(),
        );
    }
    if cfg.portfolio.initial_capital <= 0.0 {
        return (CheckStatus::Fail, "portfolio.initial_capital <= 0".into());
    }
    if cfg.execution.capital <= 0.0 {
        return (CheckStatus::Fail, "execution.capital <= 0".into());
    }
    (
        CheckStatus::Ok,
        format!(
            "间隔={}s 阈值={} 初始资金={} USDC",
            cfg.scanner.scan_interval_secs,
            cfg.scanner.opportunity_threshold,
            pm_utils::fmt_money(cfg.portfolio.initial_capital)
        ),
    )
}

async fn check_csv(cfg: &Config) -> (CheckStatus, String) {
    let r = pm_recorder::ensure_csv(&cfg.paths.opportunities_csv)
        .and_then(|_| pm_shadow::ensure_csv(&cfg.paths.shadow_csv))
        .and_then(|_| {
            pm_paper::ensure_csv(
                &cfg.paths.paper_orders_csv,
                &cfg.paths.paper_positions_csv,
                &cfg.paths.paper_portfolio_csv,
            )
        })
        .and_then(|_| pm_execution::ensure_csv(&cfg.paths.execution_csv));
    match r {
        Ok(()) => (CheckStatus::Ok, "全部 CSV 就绪".into()),
        Err(e) => (CheckStatus::Fail, format!("{:#}", e)),
    }
}

async fn check_storage(cfg: &Config) -> (CheckStatus, String) {
    let dir = &cfg.paths.data_dir;
    if !std::path::Path::new(dir).exists() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return (CheckStatus::Fail, format!("创建数据目录失败: {:#}", e));
        }
    }
    // 写一个临时文件验证可写
    let probe = std::path::Path::new(dir).join(".pm_health_probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            (CheckStatus::Ok, format!("数据目录可写: {}", dir))
        }
        Err(e) => (CheckStatus::Fail, format!("数据目录不可写: {:#}", e)),
    }
}

async fn check_clock() -> (CheckStatus, String) {
    let now = Local::now();
    let year = now.format("%Y").to_string().parse::<i32>().unwrap_or(0);
    if year < 2020 || year > 2100 {
        return (
            CheckStatus::Fail,
            format!(
                "系统时钟异常: {}（期望 2020..=2100）",
                now.format("%Y-%m-%d %H:%M:%S")
            ),
        );
    }
    (
        CheckStatus::Ok,
        format!("系统时间: {}", now.format("%Y-%m-%d %H:%M:%S")),
    )
}

async fn check_memory() -> (CheckStatus, String) {
    match available_memory_mb() {
        Some(mb) => {
            if mb < 64 {
                (
                    CheckStatus::Fail,
                    format!("可用物理内存过低: {} MB（< 64 MB）", mb),
                )
            } else {
                (CheckStatus::Ok, format!("可用物理内存: {} MB", mb))
            }
        }
        None => (CheckStatus::Warn, "当前平台不支持内存检查，已跳过".into()),
    }
}

/// API 检查：依据探测的 HTTP 状态码（status==0 表示请求未到达服务端）。
fn check_api(probe: &HealthProbe) -> (CheckStatus, String) {
    if probe.status >= 200 && probe.status < 300 {
        (
            CheckStatus::Ok,
            format!("HTTP {}（{} 毫秒）", probe.status, probe.latency_ms),
        )
    } else if probe.status == 0 {
        (CheckStatus::Fail, probe.detail.clone())
    } else {
        (CheckStatus::Fail, format!("HTTP {}", probe.status))
    }
}

/// JSON 检查：HTTP 成功前提下，依据探测是否解析成功（probe.ok）。
fn check_json(probe: &HealthProbe) -> (CheckStatus, String) {
    if probe.status >= 200 && probe.status < 300 {
        if probe.ok {
            (
                CheckStatus::Ok,
                format!("解析 {} 个市场", probe.market_count),
            )
        } else {
            (CheckStatus::Fail, "JSON 解析失败".into())
        }
    } else {
        (CheckStatus::Fail, "API 不可用，无法校验 JSON".into())
    }
}

/// 打印健康检查报告（V1.01 第十节）。
pub fn print_health_report(report: &HealthReport) {
    println!("{}", SEP);
    println!();
    println!("🚀 启动检查");
    println!();
    println!("{}", DASH);
    println!();
    for c in &report.checks {
        println!("{} {} -- {}", c.status.emoji(), c.name, c.status.zh());
        println!("  {}", c.detail);
        println!();
    }
    println!("{}", DASH);
    println!();
    if report.all_pass() {
        println!("🟢 启动检查通过");
    } else {
        let fails = report
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        println!("🔴 启动检查失败（{} 项失败）", fails);
        println!();
        println!("扫描器无法继续。请修复上述失败项后重启。");
    }
    println!();
}

// ============================================================================
// 内存检查：Windows GlobalMemoryStatusEx 原生 FFI（不引入新 crate）
// ============================================================================

#[cfg(windows)]
#[repr(C)]
#[allow(non_camel_case_types)]
struct MEMORYSTATUSEX {
    dw_length: u32,
    dw_memory_load: u32,
    ull_total_phys: u64,
    ull_avail_phys: u64,
    ull_total_page_file: u64,
    ull_avail_page_file: u64,
    ull_total_virtual: u64,
    ull_avail_virtual: u64,
    ull_avail_extended_virtual: u64,
}

#[cfg(windows)]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(lp_buffer: *mut MEMORYSTATUSEX) -> i32;
}

/// 返回可用物理内存（MB）。Windows 用 `GlobalMemoryStatusEx`；其他平台返回 `None`。
fn available_memory_mb() -> Option<u64> {
    #[cfg(windows)]
    {
        let mut s = MEMORYSTATUSEX {
            dw_length: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };
        // SAFETY: 传入正确长度初始化的结构体指针；GlobalMemoryStatusEx 仅写入该结构体。
        let ok = unsafe { GlobalMemoryStatusEx(&mut s) };
        if ok == 0 {
            return None;
        }
        Some(s.ull_avail_phys / (1024 * 1024))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pass_true_when_no_fail() {
        let r = HealthReport {
            checks: vec![
                CheckResult {
                    name: "a".into(),
                    status: CheckStatus::Ok,
                    detail: "".into(),
                },
                CheckResult {
                    name: "b".into(),
                    status: CheckStatus::Warn,
                    detail: "".into(),
                },
            ],
        };
        assert!(r.all_pass());
    }

    #[test]
    fn all_pass_false_when_fail() {
        let r = HealthReport {
            checks: vec![CheckResult {
                name: "a".into(),
                status: CheckStatus::Fail,
                detail: "".into(),
            }],
        };
        assert!(!r.all_pass());
    }

    #[tokio::test]
    async fn check_clock_passes_now() {
        let (status, _) = check_clock().await;
        assert_eq!(status, CheckStatus::Ok);
    }

    #[test]
    fn available_memory_does_not_panic() {
        let _ = available_memory_mb();
    }
}
