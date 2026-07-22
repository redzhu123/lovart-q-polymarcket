//! pm-scanner：专用持续扫描二进制。
//!
//! 瘦入口：加载 `config.toml` -> 初始化 tracing -> 调用 [`pm_scanner::run_scan`]。
//! 统一 CLI（含 scan 等全部模式）见 `apps/cli`（`cargo run -- scan`）。
//! 本二进制经 `cargo run -p pm-scanner-app` 运行。

use anyhow::Result;

const CONFIG_PATH: &str = "config.toml";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cfg = pm_models::Config::load_or_default(CONFIG_PATH);

    // 按 config.logging.log_level 映射 tracing 过滤器；屏蔽 reqwest/hyper 英文连接日志。
    let level_filter = match cfg.effective_log_level() {
        pm_models::LogLevel::Error => "error",
        pm_models::LogLevel::Warn => "warn",
        pm_models::LogLevel::Info => "info",
        pm_models::LogLevel::Debug => "debug",
        pm_models::LogLevel::Trace => "trace",
    };
    let filter = format!(
        "{level_filter},hyper=warn,hyper_util=warn,reqwest=warn,tower=warn,rustls=warn"
    );
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .try_init();

    println!("pm-scanner -- Polymarket 量化平台 V1.02");
    println!("仅模拟 -- 无钱包 / 无下单 / 无签名");
    println!();

    pm_scanner::run_scan(&cfg).await
}
