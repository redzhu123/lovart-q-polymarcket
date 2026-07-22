//! pm-cli：统一研究 CLI（默认二进制）。
//!
//! 分发六个模式（`cargo run -- <mode>`）：
//!   scan            正常扫描 + Shadow + Paper + Execution Simulator
//!   replay          历史回放
//!   paper           历史机会走 PaperTradingEngine 回放
//!   backtest        完整回测
//!   execution-test  Execution Simulator 压力测试
//!   report          汇总报告
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不下单 / 无 Polygon / WebSocket / 数据库 / Redis。

use anyhow::Result;

const CONFIG_PATH: &str = "config.toml";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cfg = pm_models::Config::load_or_default(CONFIG_PATH);

    // 初始化 tracing：按 config.logging.log_level 映射过滤器（ERROR/WARN/INFO/DEBUG/TRACE）。
    // 强制 reqwest/hyper/hyper_util 降到 warn，避免其英文 DEBUG 连接日志污染中文控制台输出。
    // 仪表盘输出仍由 pm-scanner 内部按 log_level 门控的 println 负责。
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

    println!("pm-cli -- Polymarket 量化平台 V1.02");
    println!("仅模拟 -- 无钱包 / 无下单 / 无签名");
    println!();

    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("scan");

    match mode {
        "scan" => pm_scanner::run_scan(&cfg).await,
        "diagnose" => {
            println!("模式：诊断（单次扫描 + 完整诊断报告，不进入循环）");
            pm_scanner::run_diagnose(&cfg).await
        }
        "datasource" => {
            println!("模式：数据源诊断（Provider / 能力 / 健康 / 缓存 / 校验 / 快照）");
            pm_scanner::run_datasource_diagnose(&cfg).await
        }
        "replay" => {
            println!("模式：回放");
            pm_backtest::run_replay(&cfg).await
        }
        "paper" => {
            println!("模式：纸面");
            pm_paper::run_paper_history(&cfg)
        }
        "backtest" => {
            println!("模式：回测");
            pm_backtest::run_backtest(&cfg)
        }
        "execution-test" => {
            println!("模式：执行测试");
            let params = pm_execution::ExecParams {
                capital: 1_000_000.0,
                max_pending_orders: cfg.execution.max_pending_orders,
                order_notional: cfg.execution.order_notional,
                max_wait_scans: cfg.execution.max_wait_scans,
                max_fill_delay: cfg.execution.max_fill_delay,
            };
            pm_execution::run_execution_test(params, &cfg.paths.execution_csv)
        }
        "report" => {
            println!("模式：报告");
            run_report(&cfg)
        }
        other => {
            println!("未知模式: {}", other);
            print_usage();
            Ok(())
        }
    }
}

/// report 模式：读取各 CSV，打印平台级汇总。
fn run_report(cfg: &pm_models::Config) -> Result<()> {
    let opps = pm_storage::count_rows(&cfg.paths.opportunities_csv);
    let shadow = pm_shadow::load_history(&cfg.paths.shadow_csv);
    let paper_orders = pm_storage::count_rows(&cfg.paths.paper_orders_csv);
    let paper_positions = pm_storage::count_rows(&cfg.paths.paper_positions_csv);
    let paper_portfolio = pm_storage::count_rows(&cfg.paths.paper_portfolio_csv);
    let exec_orders = pm_storage::count_rows(&cfg.paths.execution_csv);
    let backtest_rows = pm_storage::count_rows(&cfg.paths.backtest_report_csv);

    println!("======================================");
    println!();
    println!("平台报告 -- 仅模拟");
    println!();
    println!("--------------------------------------");
    println!();
    println!("机会（生命周期）        : {}", opps);
    println!("影子交易（已平仓）      : {}", shadow.stats.total);
    println!("  盈利 / 亏损           : {} / {}", shadow.stats.winners, shadow.stats.losers);
    println!("  平均收益率            : {}", pm_utils::fmt_roi(shadow.stats.average_roi()));
    println!("  最佳 / 最差收益率     : {} / {}",
             pm_utils::fmt_roi(shadow.stats.best_roi()),
             pm_utils::fmt_roi(shadow.stats.worst_roi()));
    println!("纸面订单                : {}", paper_orders);
    println!("纸面持仓（已平仓）      : {}", paper_positions);
    println!("纸面组合快照            : {}", paper_portfolio);
    println!("执行订单                : {}", exec_orders);
    println!("回测报告行数            : {}", backtest_rows);
    println!();
    println!("======================================");
    println!();
    println!("仅模拟 -- 理论估算，非真实收益");
    Ok(())
}

fn print_usage() {
    println!();
    println!("用法:");
    println!();
    println!("  cargo run -- scan            正常扫描 + 纸面交易 + 执行模拟器");
    println!("  cargo run -- diagnose        诊断模式（单次扫描 + 完整诊断报告，不进入循环）");
    println!("  cargo run -- datasource      数据源诊断（Provider / 能力 / 健康 / 缓存 / 校验 / 快照）");
    println!("  cargo run -- replay          历史回放");
    println!("  cargo run -- paper           基于历史机会的纸面交易");
    println!("  cargo run -- backtest        完整回测");
    println!("  cargo run -- execution-test  执行模拟器压测");
    println!("  cargo run -- report          汇总报告");
}
