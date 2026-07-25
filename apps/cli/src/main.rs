//! pm-cli：统一研究 CLI（默认二进制）。
//!
//! 分发模式（`cargo run -- <mode>`）：
//!   scan / diagnose / datasource / replay / paper / backtest / execution-test / report
//!   reset（B5：清空 data/*.csv，避免历史与本次运行混淆；需带 `--yes` 才真删）
//!   market / orderbook / spread / liquidity（V1.03 市场微观结构）
//!   opportunities / top / explain（V1.04 机会引擎）
//!   risk / explain-risk / risk-replay（V1.05 风险引擎）
//!   orders / execution / queue / exec-replay（V1.06 执行引擎）
//!   gateway / account / balance（V1.08 Exchange Gateway）
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不下单 / 无 Polygon / WebSocket / 数据库 / Redis。

use anyhow::Result;
use pm_audit::{AuditReport, ExplainReport};
use pm_auth::{self, diagnose_auth_credential, diagnose_auth_health, diagnose_auth_session};
use pm_gateway::{
    self, GatewayConfig, create_gateway, diagnose_account, diagnose_balance, diagnose_gateway,
};
use pm_market_framework::MarketFramework;
use pm_market_framework::prelude::*;
use pm_oms::prelude::*;
use pm_orderbook::{
    DepthAnalyzer, LiquidityAnalyzer, MarketStatistics, OrderBookSnapshot, OrderBookValidator,
    OrderBookVisualizer, SpreadAnalyzer,
};
use pm_risk::{RiskConfig, RiskContext, RiskDashboard, RiskEngine, RiskReplay, TradeSuggestion};
use pm_scanner::DataSourceManager;
use pm_settlement::prelude::*;
use pm_trading::{
    self, MockTradingProvider, TradingConfig, diagnose_connection, diagnose_credential,
    diagnose_health, diagnose_provider, diagnose_session,
};
use pm_wallet::{self, diagnose_wallet_accounts, diagnose_wallet_balance, diagnose_wallet_health};

const CONFIG_PATH: &str = "config.toml";

/// 市场微观结构命令默认分析的订单簿数量上限。
const ORDERBOOK_LIMIT: usize = 10;

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
    let filter =
        format!("{level_filter},hyper=warn,hyper_util=warn,reqwest=warn,tower=warn,rustls=warn");
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .try_init();

    println!("pm-cli -- Polymarket 量化平台 V1.08");
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
        "reset" => {
            // B5：清空 data/*.csv。需带 --yes 才真删；否则只打印预览。
            let yes = args.iter().any(|a| a == "--yes" || a == "-y");
            run_reset(&cfg, yes)
        }
        // ---- V1.03 市场微观结构 CLI ----
        "market" => {
            println!("模式：市场列表");
            run_market(&cfg).await
        }
        "orderbook" => {
            println!("模式：订单簿");
            run_orderbook(&cfg).await
        }
        "spread" => {
            println!("模式：价差分析");
            run_spread(&cfg).await
        }
        "liquidity" => {
            println!("模式：流动性分析");
            run_liquidity(&cfg).await
        }
        // ---- V1.04 机会引擎 CLI ----
        "opportunities" | "opps" => {
            println!("模式：机会列表");
            run_opportunities(&cfg).await
        }
        "top" => {
            println!("模式：Top 10 机会");
            run_top(&cfg).await
        }
        "explain" => {
            let sub = args.get(2).map(|s| s.as_str()).unwrap_or("pipeline");
            match sub {
                "pipeline" | "" => {
                    println!("模式：数据链路解释");
                    run_explain_pipeline(&cfg)
                }
                "rejections" => {
                    println!("模式：拒绝原因分析");
                    run_explain_rejections(&cfg)
                }
                id => {
                    println!("模式：机会解释");
                    run_explain(&cfg, id).await
                }
            }
        }
        "audit" => {
            println!("模式：数据审计");
            run_audit(&cfg)
        }
        // ---- V1.09 End ----
        // ---- V1.05 风险引擎 CLI ----
        "risk" => {
            println!("模式：风险仪表盘");
            run_risk(&cfg).await
        }
        "explain-risk" => {
            println!("模式：风险解释");
            run_explain_risk(&cfg).await
        }
        "risk-replay" => {
            println!("模式：风险回放");
            run_risk_replay(&cfg).await
        }
        // ---- V1.06 执行引擎 CLI ----
        "orders" => {
            println!("模式：订单列表");
            run_orders(&cfg).await
        }
        "execution" => {
            println!("模式：执行状态");
            run_execution_status(&cfg)
        }
        "queue" => {
            println!("模式：队列查看");
            run_queue_status(&cfg)
        }
        "exec-replay" => {
            let id = args.get(2).cloned().unwrap_or_default();
            if id.is_empty() {
                println!("用法: cargo run -- exec-replay <order_id>");
                return Ok(());
            }
            println!("模式：订单回放");
            run_exec_replay(&cfg, &id)
        }
        // ---- V1.07 Trading Infrastructure CLI ----
        "provider" => {
            println!("模式：Provider 诊断");
            run_trading_provider(&cfg).await
        }
        "health" => {
            println!("模式：Health 诊断");
            run_trading_health(&cfg).await
        }
        "session" => {
            println!("模式：Session 诊断");
            run_trading_session(&cfg)
        }
        "connection" => {
            println!("模式：Connection 诊断");
            run_trading_connection(&cfg)
        }
        // ---- V1.08 Exchange Gateway CLI ----
        "gateway" => {
            println!("模式：Gateway 状态");
            run_gateway(&cfg).await
        }
        "account" => {
            println!("模式：账户诊断");
            run_account(&cfg).await
        }
        "balance" => {
            println!("模式：余额查询");
            run_balance(&cfg).await
        }
        // ---- P2-02 API Workflow CLI ----
        "workflow" => {
            println!("模式：API Workflow");
            let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
            run_workflow(sub).await
        }
        // ---- P2-04 OMS CLI ----
        "oms" => {
            println!("模式：OMS 概览");
            run_oms_overview(&cfg).await
        }
        "oms-orders" => {
            println!("模式：OMS 订单列表");
            run_oms_orders(&cfg).await
        }
        "oms-order" => {
            let id = args.get(2).cloned().unwrap_or_default();
            if id.is_empty() {
                println!("用法: cargo run -- oms-order <order_id>");
                return Ok(());
            }
            println!("模式：OMS 订单详情");
            run_oms_order(&cfg, &id).await
        }
        "oms-events" => {
            println!("模式：OMS 事件流");
            run_oms_events(&cfg).await
        }
        "oms-demo" => {
            println!("模式：OMS 演示（创建 5 个订单 + 推进到终态）");
            run_oms_demo(&cfg).await
        }
        // ---- P2-05 PMS CLI ----
        "portfolio" => {
            println!("模式：投资组合");
            run_portfolio(&cfg)
        }
        "positions" => {
            println!("模式：持仓列表");
            run_positions(&cfg)
        }
        "pnl" => {
            println!("模式：盈亏报告");
            run_pnl(&cfg)
        }
        "exposure" => {
            println!("模式：风险敞口");
            run_exposure(&cfg)
        }
        // ---- P2-06 Auth & Wallet Infrastructure CLI ----
        "auth" => {
            let sub = args.get(2).map(|s| s.as_str()).unwrap_or("health");
            match sub {
                "health" => {
                    println!("模式：认证健康检查");
                    run_auth_health(&cfg).await
                }
                "session" => {
                    println!("模式：会话诊断");
                    run_auth_session(&cfg).await
                }
                "credential" => {
                    println!("模式：凭据诊断");
                    run_auth_credential(&cfg).await
                }
                _ => {
                    println!("用法: cargo run -- auth [health|session|credential]");
                    Ok(())
                }
            }
        }
        "wallet" => {
            let sub = args.get(2).map(|s| s.as_str()).unwrap_or("health");
            match sub {
                "health" => {
                    println!("模式：钱包健康检查");
                    run_wallet_health(&cfg)
                }
                "balance" => {
                    println!("模式：余额查询");
                    run_wallet_balance(&cfg)
                }
                "account" => {
                    println!("模式：账户列表");
                    run_wallet_accounts(&cfg)
                }
                _ => {
                    println!("用法: cargo run -- wallet [health|balance|account]");
                    Ok(())
                }
            }
        }
        // ---- P2-06 Settlement Engine CLI ----
        "settlement" => {
            println!("模式：结算引擎");
            run_settlement(&cfg).await
        }
        "ledger" => {
            println!("模式：资金流水");
            run_ledger(&cfg)
        }
        "fees" => {
            println!("模式：手续费");
            run_fees(&cfg)
        }
        // ---- P3.0 多市场统一框架 CLI ----
        "markets" => {
            let sub = args.get(2).map(|s| s.as_str()).unwrap_or("list");
            match sub {
                "health" => {
                    println!("模式：多市场健康检查");
                    run_market_health().await
                }
                "list" | _ => {
                    println!("模式：多市场列表");
                    run_markets().await
                }
            }
        }
        // "orders" 已存在（V1.06），但 V1.08 增强为经 Gateway 查询
        other => {
            println!("未知模式: {}", other);
            print_usage();
            Ok(())
        }
    }
}

// ============================================================================
// V1.03 市场微观结构 CLI 命令
// ============================================================================

/// 获取市场 ID 列表：优先使用当前 Provider，不支持时回退到 Gamma。
async fn get_market_ids(cfg: &pm_models::Config, limit: usize) -> Result<Vec<String>> {
    let mut manager = DataSourceManager::from_config(cfg)?;

    if manager.capability().supports_markets {
        let outcome = manager.fetch_markets().await?;
        let ids: Vec<String> = outcome
            .markets
            .iter()
            .filter(|m| m.active())
            .take(limit)
            .map(|m| m.market_id.clone())
            .collect();
        return Ok(ids);
    }

    // 回退到 Gamma（CLOB 不支持市场列表）
    if cfg.datasource.provider != "gamma" {
        tracing::info!("当前 Provider 不支持市场列表，回退到 Gamma 获取市场 ID");
        let mut gamma_cfg = cfg.clone();
        gamma_cfg.datasource.provider = "gamma".into();
        let mut gamma_manager = DataSourceManager::from_config(&gamma_cfg)?;
        let outcome = gamma_manager.fetch_markets().await?;
        let ids: Vec<String> = outcome
            .markets
            .iter()
            .filter(|m| m.active())
            .take(limit)
            .map(|m| m.market_id.clone())
            .collect();
        return Ok(ids);
    }

    Ok(Vec::new())
}

/// `cargo run -- market`：拉取市场列表并展示基本信息。
async fn run_market(cfg: &pm_models::Config) -> Result<()> {
    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    if !manager.capability().supports_markets {
        println!("当前 Provider 不支持市场列表。");
        println!(
            "提示：使用 Gamma Provider (provider=\"gamma\") 或 Mock Provider (provider=\"mock\")。"
        );
        return Ok(());
    }

    println!("正在拉取市场数据...");
    println!();

    let outcome = manager.fetch_markets().await?;
    let markets = &outcome.markets;

    println!("市场数量: {}", markets.len());
    if outcome.cached {
        println!("（数据来自缓存）");
    }
    println!();

    // 打印前 20 个市场
    let display_count = markets.len().min(20);
    println!("前 {} 个市场：", display_count);
    println!();
    println!(
        "{:<40} {:>6} {:>6} {:>10} {:>10} {:>8}",
        "问题", "YES", "NO", "成交量", "流动性", "状态"
    );
    println!("{}", "-".repeat(90));

    for m in markets.iter().take(display_count) {
        let yes_str = m
            .yes_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "  -   ".into());
        let no_str = m
            .no_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "  -   ".into());
        let vol_str = if m.volume >= 1000.0 {
            format!("{:.1}k", m.volume / 1000.0)
        } else {
            format!("{:.0}", m.volume)
        };
        let liq_str = if m.liquidity >= 1000.0 {
            format!("{:.1}k", m.liquidity / 1000.0)
        } else {
            format!("{:.0}", m.liquidity)
        };
        // 截断问题到 38 个字符
        let q: String = m.question.chars().take(38).collect();

        println!(
            "{:<40} {:>6} {:>6} {:>10} {:>10} {:>8}",
            q,
            yes_str,
            no_str,
            vol_str,
            liq_str,
            m.status.as_zh()
        );
    }

    if markets.len() > display_count {
        println!("... 及其他 {} 个市场", markets.len() - display_count);
    }
    println!();

    Ok(())
}

/// `cargo run -- orderbook`：拉取订单簿并展示。
async fn run_orderbook(cfg: &pm_models::Config) -> Result<()> {
    let manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    if !manager.capability().supports_orderbook {
        println!("当前 Provider 不支持订单簿。");
        println!(
            "提示：使用 CLOB Provider (provider=\"clob\") 或 Mock Provider (provider=\"mock\")。"
        );
        return Ok(());
    }

    println!("正在拉取订单簿数据...");
    println!();

    let ids = get_market_ids(cfg, ORDERBOOK_LIMIT).await?;

    if ids.is_empty() {
        println!("无可用市场，无法获取订单簿。");
        return Ok(());
    }

    println!("对 {} 个市场查询订单簿...", ids.len());
    println!();

    let orderbooks = manager.provider().fetch_orderbooks(&ids).await?;

    // 校验
    let validation = OrderBookValidator::validate_all(&orderbooks);
    validation.print_summary();
    println!();

    // 展示每个订单簿
    for ob in &orderbooks {
        if ob.best_bid.is_none() && ob.best_ask.is_none() {
            println!("市场 {}：订单簿数据不可用", ob.market_id);
            println!();
            continue;
        }

        println!("{}", "─".repeat(60));
        println!("市场: {}", ob.market_id);
        println!();

        if let Some(bid) = ob.best_bid {
            println!(
                "  Best Bid : {:.4}  |  买盘深度: {:.2}",
                bid,
                ob.bid_depth.unwrap_or(0.0)
            );
        }
        if let Some(ask) = ob.best_ask {
            println!(
                "  Best Ask : {:.4}  |  卖盘深度: {:.2}",
                ask,
                ob.ask_depth.unwrap_or(0.0)
            );
        }
        if let Some(spread) = ob.spread {
            println!("  Spread   : {:.4}", spread);
        }
        println!(
            "  买盘档位: {}  |  卖盘档位: {}",
            ob.bid_levels.len(),
            ob.ask_levels.len()
        );
        println!(
            "  累计买量: {:.2}  |  累计卖量: {:.2}",
            ob.bid_volume, ob.ask_volume
        );
        println!();

        // ASCII 可视化
        if !ob.bid_levels.is_empty() || !ob.ask_levels.is_empty() {
            let ascii = OrderBookVisualizer::render(ob);
            println!("{}", ascii);
        }
    }

    // 保存快照
    let snapshots = OrderBookSnapshot::from_orderbooks(&orderbooks, chrono::Local::now());
    let snap_path = format!("{}/orderbook_snapshots.csv", cfg.paths.data_dir);
    OrderBookSnapshot::save_to_csv(&snapshots, &snap_path)?;

    Ok(())
}

/// `cargo run -- spread`：价差分析。
async fn run_spread(cfg: &pm_models::Config) -> Result<()> {
    let manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    if !manager.capability().supports_orderbook {
        println!("当前 Provider 不支持订单簿，无法进行价差分析。");
        println!();
        println!("提示：使用 Gamma Provider 无法获取价差（Gamma 无订单簿）。");
        println!("请将 config.toml 中 [datasource] provider 设为 \"clob\" 以获取真实订单簿。");
        println!("或设为 \"mock\" 使用模拟数据测试。");
        return Ok(());
    }

    let ids = get_market_ids(cfg, ORDERBOOK_LIMIT).await?;

    println!("对 {} 个市场进行价差分析...", ids.len());
    println!();

    let orderbooks = manager.provider().fetch_orderbooks(&ids).await?;

    let mut valid_count = 0;
    for ob in &orderbooks {
        let report = SpreadAnalyzer::analyze(ob);
        if report.spread.is_some() {
            valid_count += 1;
            SpreadAnalyzer::print_report(&report);
        }
    }

    // 汇总
    let summary = SpreadAnalyzer::summarize(&orderbooks);
    if valid_count > 0 {
        SpreadAnalyzer::print_summary(&summary);
    } else {
        println!("（无有效价差数据 -- 该 Provider 不支持订单簿或未返回价差信息）");
        println!();
    }

    Ok(())
}

/// `cargo run -- liquidity`：流动性分析。
async fn run_liquidity(cfg: &pm_models::Config) -> Result<()> {
    let manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    if !manager.capability().supports_orderbook {
        println!("当前 Provider 不支持订单簿，无法进行流动性分析。");
        println!();
        println!("提示：请将 config.toml 中 [datasource] provider 设为 \"clob\" 或 \"mock\"。");
        return Ok(());
    }

    let ids = get_market_ids(cfg, ORDERBOOK_LIMIT).await?;

    println!("对 {} 个市场进行流动性分析...", ids.len());
    println!();

    let orderbooks = manager.provider().fetch_orderbooks(&ids).await?;
    let reports = LiquidityAnalyzer::analyze_all(&orderbooks);

    let mut has_data = false;
    for report in &reports {
        if report.total_liquidity > 0.0 {
            has_data = true;
            LiquidityAnalyzer::print_report(report);
        }
    }

    if has_data {
        LiquidityAnalyzer::print_summary(&reports);
    }

    // 附带深度分析
    println!("{}", "=".repeat(60));
    println!();
    println!("深度分析（附带）：");
    println!();
    let depth_reports = DepthAnalyzer::analyze_all(&orderbooks);
    let depth_has_data = depth_reports
        .iter()
        .any(|r| r.bid_top10 > 0.0 || r.ask_top10 > 0.0);
    if depth_has_data {
        let depth_summary = DepthAnalyzer::summarize(&depth_reports);
        DepthAnalyzer::print_summary(&depth_summary);
    } else {
        println!("（无深度数据）");
        println!();
    }

    // 累计统计
    println!("{}", "=".repeat(60));
    println!();
    let mut stats = MarketStatistics::new();
    stats.accumulate(&orderbooks);
    stats.print_report();

    Ok(())
}

// ============================================================================
// 原有 CLI 命令
// ============================================================================

/// report 模式：读取各 CSV，打印平台级汇总。
fn run_report(cfg: &pm_models::Config) -> Result<()> {
    eprint!("[1/8] 读取机会（生命周期）... ");
    let opps = pm_storage::count_rows(&cfg.paths.opportunities_csv);
    eprintln!("{} 行", opps);

    eprint!("[2/8] 读取机会（检测即落盘）... ");
    let detected = pm_storage::count_rows(&cfg.paths.detected_opportunities_csv);
    eprintln!("{} 行", detected);

    eprint!("[3/8] 读取影子交易... ");
    let shadow = pm_shadow::load_history(&cfg.paths.shadow_csv);
    eprintln!("{} 笔（已平仓）", shadow.stats.total);

    eprint!("[4/8] 读取纸面订单... ");
    let paper_orders = pm_storage::count_rows(&cfg.paths.paper_orders_csv);
    eprintln!("{} 行", paper_orders);

    eprint!("[5/8] 读取纸面持仓... ");
    let paper_positions = pm_storage::count_rows(&cfg.paths.paper_positions_csv);
    eprintln!("{} 行", paper_positions);

    eprint!("[6/8] 读取纸面组合快照... ");
    let paper_portfolio = pm_storage::count_rows(&cfg.paths.paper_portfolio_csv);
    eprintln!("{} 行", paper_portfolio);

    eprint!("[7/8] 读取执行订单... ");
    let exec_orders = pm_storage::count_rows(&cfg.paths.execution_csv);
    eprintln!("{} 行", exec_orders);

    eprint!("[8/8] 读取回测报告... ");
    let backtest_rows = pm_storage::count_rows(&cfg.paths.backtest_report_csv);
    eprintln!("{} 行", backtest_rows);

    println!();
    println!("======================================");
    println!();
    println!("平台报告 -- 仅模拟");
    println!();
    println!("以下数字读取自 data/*.csv，**累计含历史**（跨多次运行未重置）。");
    println!("如需干净基线，请运行: cargo run -- reset --yes");
    println!();
    println!("--------------------------------------");
    println!();
    println!("机会（生命周期，已结束）  : {}", opps);
    println!("机会（检测即落盘）        : {}（B3: 与纸面订单对账用）", detected);
    println!("影子交易（已平仓）        : {}", shadow.stats.total);
    println!(
        "  盈利 / 亏损             : {} / {}",
        shadow.stats.winners, shadow.stats.losers
    );
    println!(
        "  平均收益率              : {}",
        pm_utils::fmt_roi(shadow.stats.average_roi())
    );
    println!(
        "  最佳 / 最差收益率       : {} / {}",
        pm_utils::fmt_roi(shadow.stats.best_roi()),
        pm_utils::fmt_roi(shadow.stats.worst_roi())
    );
    println!("纸面订单（开+平仓）       : {}", paper_orders);
    println!("纸面持仓（已平仓）        : {}", paper_positions);
    println!("纸面组合快照（每轮）      : {}", paper_portfolio);
    println!("执行订单                  : {}", exec_orders);
    println!("回测报告行数              : {}", backtest_rows);
    println!();
    println!("======================================");
    println!();
    println!("仅模拟 -- 理论估算，非真实收益");
    Ok(())
}

// ============================================================================
// B5：reset 命令 — 清空 data/*.csv，避免历史与本次运行混淆
// ============================================================================

/// 列出所有会受 reset 影响的 CSV / 日志文件。
fn reset_target_files(cfg: &pm_models::Config) -> Vec<String> {
    let mut files = vec![
        cfg.paths.opportunities_csv.clone(),
        cfg.paths.shadow_csv.clone(),
        cfg.paths.paper_orders_csv.clone(),
        cfg.paths.paper_positions_csv.clone(),
        cfg.paths.paper_portfolio_csv.clone(),
        cfg.paths.execution_csv.clone(),
        cfg.paths.backtest_report_csv.clone(),
        cfg.paths.detected_opportunities_csv.clone(),
    ];
    // V1.06/V1.08 额外 CSV
    files.push("data/execution_events.csv".to_string());
    files.push("data/execution_report.csv".to_string());
    files.push("data/risk_events.csv".to_string());
    files.push("data/risk_dashboard.csv".to_string());
    files.push("data/market_snapshots.csv".to_string());
    files.push("data/gateway_metrics.csv".to_string());
    files.push("data/gateway_health.csv".to_string());
    files.sort();
    files.dedup();
    files
}

/// reset 模式：清空所有 CSV。`yes=true` 时直接删；否则只打印预览。
///
/// 用法：
///   cargo run -- reset          # 仅预览
///   cargo run -- reset --yes    # 实际删除
fn run_reset(cfg: &pm_models::Config, yes: bool) -> Result<()> {
    let files = reset_target_files(cfg);

    println!("======================================");
    println!();
    println!("reset 模式 -- 清空历史 CSV");
    println!();
    println!("将影响以下文件（仅模拟数据，可安全删除）：");
    println!();
    let mut existing = 0usize;
    for f in &files {
        let status = if std::path::Path::new(f).exists() {
            existing += 1;
            "存在"
        } else {
            "不存在"
        };
        println!("  [{:<6}] {}", status, f);
    }
    println!();
    println!("共 {} 个文件，实际存在 {} 个", files.len(), existing);
    println!();

    if !yes {
        println!("未传 --yes / -y，仅预览。如确认删除请运行：");
        println!("    cargo run -- reset --yes");
        println!();
        return Ok(());
    }

    let mut deleted = 0usize;
    let mut failed = Vec::new();
    for f in &files {
        let p = std::path::Path::new(f);
        if !p.exists() {
            continue;
        }
        match std::fs::remove_file(p) {
            Ok(_) => deleted += 1,
            Err(e) => failed.push(format!("{}: {}", f, e)),
        }
    }

    println!("已删除 {} 个文件", deleted);
    if !failed.is_empty() {
        eprintln!("以下文件删除失败：");
        for fe in &failed {
            eprintln!("  {}", fe);
        }
        anyhow::bail!("reset 部分失败");
    }
    println!("reset 完成。下次扫描将重新生成 CSV。");
    Ok(())
}

// ============================================================================
// V1.04 机会引擎 CLI 命令
// ============================================================================

/// `cargo run -- opportunities`：列出所有机会。
async fn run_opportunities(cfg: &pm_models::Config) -> Result<()> {
    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    println!("正在拉取市场数据并分析机会...");
    println!();

    let outcome = manager.fetch_markets().await?;
    let markets = &outcome.markets;

    let mut engine = pm_opportunity::OpportunityEngine::from_pm_config(cfg);

    // 尝试获取订单簿
    let mut orderbooks_map: std::collections::HashMap<String, pm_models::OrderBook> =
        std::collections::HashMap::new();
    if manager.capability().supports_orderbook {
        let ids: Vec<String> = markets.iter().map(|m| m.market_id.clone()).collect();
        if let Ok(obs) = manager.provider().fetch_orderbooks(&ids).await {
            for ob in obs {
                if ob.best_bid.is_some() || ob.best_ask.is_some() {
                    orderbooks_map.insert(ob.market_id.clone(), ob);
                }
            }
        }
    }

    let output = engine.analyze(
        markets,
        &orderbooks_map,
        &manager.provider().capability(),
        chrono::Utc::now(),
    );

    println!("机会总数: {}", output.opportunities.len());
    println!(
        "新增: {}  更新: {}  过滤: {}  过期: {}",
        output.new_count,
        output.updated_count,
        output.filtered_count,
        output.expired.len()
    );
    println!();
    println!("{}", "=".repeat(100));
    println!();

    if output.opportunities.is_empty() {
        println!("（未发现符合条件的套利机会）");
        println!();
        println!("提示：");
        println!("  - 当前阈值: SUM < {}", cfg.scanner.opportunity_threshold);
        println!("  - Gamma 数据归一化（YES+NO=1.0），常态下无套利机会");
        println!("  - 使用 CLOB (provider=\"clob\") 获取真实订单簿价格");
        println!("  - 使用 Mock (provider=\"mock\") 查看模拟机会");
        return Ok(());
    }

    for opp in &output.opportunities {
        println!("{}", pm_opportunity::ExplainEngine::explain(opp));
        println!();
    }

    Ok(())
}

/// `cargo run -- top`：Top 10 机会。
async fn run_top(cfg: &pm_models::Config) -> Result<()> {
    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    println!("正在拉取市场数据并分析 Top 10 机会...");
    println!();

    let outcome = manager.fetch_markets().await?;
    let markets = &outcome.markets;

    let mut engine = pm_opportunity::OpportunityEngine::from_pm_config(cfg);

    let mut orderbooks_map: std::collections::HashMap<String, pm_models::OrderBook> =
        std::collections::HashMap::new();
    if manager.capability().supports_orderbook {
        let ids: Vec<String> = markets.iter().map(|m| m.market_id.clone()).collect();
        if let Ok(obs) = manager.provider().fetch_orderbooks(&ids).await {
            for ob in obs {
                if ob.best_bid.is_some() || ob.best_ask.is_some() {
                    orderbooks_map.insert(ob.market_id.clone(), ob);
                }
            }
        }
    }

    let _output = engine.analyze(
        markets,
        &orderbooks_map,
        &manager.provider().capability(),
        chrono::Utc::now(),
    );

    let top = engine.top_n(10);

    if top.is_empty() {
        println!("（未发现符合条件的套利机会）");
        return Ok(());
    }

    println!("Top {} 机会：", top.len());
    println!();
    println!(
        "{:<6} {:<10} {:<45} {:<8} {:<8} {:<8} {:<8}",
        "排名", "类型", "问题", "评分", "置信度", "风险", "ROI"
    );
    println!("{}", "-".repeat(100));

    for (i, opp) in top.iter().enumerate() {
        let q: String = opp.question.chars().take(43).collect();
        println!(
            "{:<6} {:<10} {:<45} {:<8.0} {:<8.0} {:<8.0} {:<8.2}",
            i + 1,
            opp.opportunity_type.as_zh(),
            q,
            opp.score,
            opp.confidence * 100.0,
            opp.risk_score,
            opp.expected_roi * 100.0,
        );
    }
    println!();

    Ok(())
}

/// `cargo run -- explain <id>`：解释某个机会的评分。
async fn run_explain(cfg: &pm_models::Config, target_id: &str) -> Result<()> {
    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    println!("正在查找机会: {} ...", target_id);
    println!();

    let outcome = manager.fetch_markets().await?;
    let markets = &outcome.markets;

    let mut engine = pm_opportunity::OpportunityEngine::from_pm_config(cfg);

    let mut orderbooks_map: std::collections::HashMap<String, pm_models::OrderBook> =
        std::collections::HashMap::new();
    if manager.capability().supports_orderbook {
        let ids: Vec<String> = markets.iter().map(|m| m.market_id.clone()).collect();
        if let Ok(obs) = manager.provider().fetch_orderbooks(&ids).await {
            for ob in obs {
                if ob.best_bid.is_some() || ob.best_ask.is_some() {
                    orderbooks_map.insert(ob.market_id.clone(), ob);
                }
            }
        }
    }

    let output = engine.analyze(
        markets,
        &orderbooks_map,
        &manager.provider().capability(),
        chrono::Utc::now(),
    );

    // 按 ID 或 market_id 匹配
    let found = output
        .opportunities
        .iter()
        .find(|opp| opp.id.starts_with(target_id) || opp.market_id.starts_with(target_id));

    match found {
        Some(opp) => {
            let explanation = pm_opportunity::ExplainEngine::explain(opp);
            println!("{}", explanation);
        }
        None => {
            println!("未找到匹配的机会: {}", target_id);
            println!();
            println!("可用的机会 ID：");
            for opp in &output.opportunities {
                println!(
                    "  {} ({})",
                    opp.id,
                    opp.question.chars().take(50).collect::<String>()
                );
            }
            if output.opportunities.is_empty() {
                println!("  （无机会）");
            }
        }
    }

    Ok(())
}

// ============================================================================
// V1.09 数据审计 CLI 命令
// ============================================================================

/// `cargo run -- explain pipeline`：完整数据链路分析报告。
fn run_explain_pipeline(cfg: &pm_models::Config) -> Result<()> {
    println!();
    let mut report = ExplainReport::from_csv_paths(
        &cfg.paths.opportunities_csv,
        &cfg.paths.detected_opportunities_csv,
        &cfg.paths.shadow_csv,
        &cfg.paths.paper_orders_csv,
        &cfg.paths.paper_positions_csv,
        &cfg.paths.paper_portfolio_csv,
        &cfg.paths.execution_csv,
    );

    // 添加市场扫描数统计
    let snapshots_csv = format!("{}/market_snapshots.csv", cfg.paths.data_dir);
    report.stats.markets_scanned = pm_storage::count_rows(&snapshots_csv);

    // 添加分析说明
    let provider = &cfg.datasource.provider;
    let threshold = cfg.scanner.opportunity_threshold;
    report.add_note(format!(
        "数据源: {} | 机会阈值: {} | 扫描间隔: {}s",
        provider, threshold, cfg.scanner.scan_interval_secs
    ));

    if report.stats.opportunities_count == 0 {
        report.add_note(
            "机会数 = 0：可能原因：(1) Gamma API 返回归一化价格 (YES+NO=1.0)，常态下无套利机会。\
             (2) 当前 provider 不支持真实订单簿价格。建议将 provider 切换为 \"clob\" 获取真实买卖价。"
                .into(),
        );
    }
    if report.stats.portfolio_snapshots > 1000 {
        report.add_note(format!(
            "组合快照数 ({}) 异常偏高。V1.09 已修复：仅组合变化时写入，避免每轮重复。\
             重置数据目录可清理旧快照。",
            report.stats.portfolio_snapshots
        ));
    }
    if report.stats.paper_orders_count > 0 && report.stats.opportunities_count == 0 {
        report.add_note(
            "纸面订单 > 0 但机会 = 0：订单可能来自历史运行（Mock Provider 模拟数据）。\
             重置 data/*.csv 可清除。"
                .into(),
        );
    }
    if report.stats.execution_orders_count > report.stats.paper_orders_count * 2 + 10 {
        report.add_note(format!(
            "执行订单 ({}) 远超纸面订单 ({}) × 2。执行订单包含 BUY + SELL + 拒绝/过期/取消，\
             数量偏高可能因快速连续扫描积压所致。V1.09 已限制组合快照写入，其他 CSV 保留完整审计轨迹。",
            report.stats.execution_orders_count,
            report.stats.paper_orders_count
        ));
    }

    println!("{}", report.render_zh());

    // 保存报告
    let report_dir = "reports/audit";
    let _ = std::fs::create_dir_all(report_dir);
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = format!("{}/pipeline_explain_{}.txt", report_dir, ts);
    if let Err(e) = std::fs::write(&path, report.render_zh()) {
        tracing::warn!("报告保存失败: {} — {}", path, e);
    } else {
        println!("报告已保存至: {}", path);
    }

    Ok(())
}

/// `cargo run -- explain rejections`：拒绝原因详细分析。
fn run_explain_rejections(cfg: &pm_models::Config) -> Result<()> {
    println!();
    println!("=========================");
    println!("拒绝原因分析");
    println!("=========================");
    println!();
    println!("当前数据源: {}", cfg.datasource.provider);
    println!("机会阈值: {}", cfg.scanner.opportunity_threshold);
    println!();

    let opps_count = pm_storage::count_rows(&cfg.paths.opportunities_csv);
    println!("机会数(CSV): {}", opps_count);

    if opps_count == 0 {
        println!();
        println!("── 分析 ──");
        println!();
        println!("  无机会记录。可能原因：");
        println!();
        if cfg.datasource.provider == "gamma" {
            println!("  1. Gamma API 归一化价格 (YES+NO 恒为 1.0)");
            println!(
                "     所有市场的 sum 都 >= 阈值 ({}),",
                cfg.scanner.opportunity_threshold
            );
            println!("     被归类为'价差过小'(SpreadTooSmall)。");
            println!();
            println!("  2. 建议：");
            println!("     - 使用 CLOB Provider 获取真实订单簿价格");
            println!("       修改 config.toml: [datasource] provider = \"clob\"");
            println!("     - 或提高阈值: [scanner] opportunity_threshold = 1.01");
        } else if cfg.datasource.provider == "mock" {
            println!("  1. Mock Provider 可能未生成足够低 SUM 的数据。");
            println!();
            println!("  2. 建议切换到 Gamma 或 CLOB 获取真实数据。");
        } else {
            println!(
                "  1. 当前 Provider \"{}\" 可能无满足阈值条件的市场。",
                cfg.datasource.provider
            );
            println!();
            println!("  2. 检查 config.toml 中的 opportunity_threshold 设置。");
        }
    }

    println!();
    println!("=========================");
    Ok(())
}

/// `cargo run -- audit`：自动数据一致性审计。
fn run_audit(cfg: &pm_models::Config) -> Result<()> {
    println!();
    let report = AuditReport::run(
        &cfg.paths.opportunities_csv,
        &cfg.paths.detected_opportunities_csv,
        &cfg.paths.shadow_csv,
        &cfg.paths.paper_orders_csv,
        &cfg.paths.paper_positions_csv,
        &cfg.paths.paper_portfolio_csv,
        &cfg.paths.execution_csv,
    );
    println!("{}", report.render_zh());

    // 保存报告
    let report_dir = "reports/audit";
    let _ = std::fs::create_dir_all(report_dir);
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = format!("{}/audit_{}.txt", report_dir, ts);
    if let Err(e) = std::fs::write(&path, report.render_zh()) {
        tracing::warn!("审计报告保存失败: {} — {}", path, e);
    } else {
        println!("审计报告已保存至: {}", path);
    }

    Ok(())
}

// ============================================================================
// V1.05 风险引擎 CLI 命令
// ============================================================================

/// `cargo run -- risk`：显示风险仪表盘。
async fn run_risk(cfg: &pm_models::Config) -> Result<()> {
    let risk_config = RiskConfig::from_pm_config(cfg);
    let engine = RiskEngine::new(risk_config);

    // 构建 RiskContext（从 Paper + Execution 当前状态）
    let now = chrono::Local::now();
    let ctx = RiskContext::minimal(
        cfg.portfolio.initial_capital,
        cfg.portfolio.initial_capital,
        now,
    );

    let exposure = pm_risk::ExposureReport::new(cfg.portfolio.initial_capital);

    let dashboard = RiskDashboard::render(&engine, &ctx, &exposure);
    println!("{}", dashboard);
    println!();
    println!("提示：运行 `cargo run -- explain-risk` 查看风险规则说明。");
    println!();

    Ok(())
}

/// `cargo run -- explain-risk`：解释风险规则。
async fn run_explain_risk(cfg: &pm_models::Config) -> Result<()> {
    let risk_config = RiskConfig::from_pm_config(cfg);

    println!("【风险规则说明】");
    println!();
    println!("Risk Engine V1.05 — 统一风险引擎");
    println!();
    println!("所有交易必须经过 Risk Engine 审核，禁止绕过。");
    println!();
    println!("决策类型：");
    println!(
        "  ✅ 接受（Accept）  — 风险评分 ≥ {}",
        risk_config.accept_threshold
    );
    println!(
        "  ⚠️ 需审核（Review） — 风险评分 {:.0}~{:.0} 或存在警告",
        risk_config.review_threshold, risk_config.accept_threshold
    );
    println!(
        "  ❌ 拒绝（Reject）  — 风险评分 < {:.0} 或触发硬限制",
        risk_config.review_threshold
    );
    println!();
    println!("── 风险规则 ──");
    println!();
    println!("  最大持仓数量：    {} 个", risk_config.max_positions);
    println!(
        "  单笔最大资金：    {:.0} USDC",
        risk_config.max_position_size
    );
    println!("  最大待处理订单：  {} 个", risk_config.max_open_orders);
    println!(
        "  最大单笔占用：    {:.0} USDC",
        risk_config.max_single_capital
    );
    println!(
        "  最大资金利用率：  {:.0}%",
        risk_config.max_capital_usage * 100.0
    );
    println!("  每日最大亏损：    {:.0} USDC", risk_config.max_daily_loss);
    println!(
        "  连续亏损上限：    {} 次",
        risk_config.max_consecutive_losses
    );
    println!(
        "  最大回撤：        {:.0}%",
        risk_config.max_drawdown * 100.0
    );
    println!(
        "  最大市场暴露：    {:.0}%",
        risk_config.max_market_exposure * 100.0
    );
    println!(
        "  最大类别暴露：    {:.0}%",
        risk_config.max_category_exposure * 100.0
    );
    println!(
        "  最大方向暴露：    {:.0}%",
        risk_config.max_side_exposure * 100.0
    );
    println!("  最低流动性：      {:.0} USDC", risk_config.min_liquidity);
    println!("  最低深度：        {:.0} USDC", risk_config.min_depth);
    println!(
        "  最大滑点：        {:.1}%",
        risk_config.max_slippage * 100.0
    );
    println!(
        "  最高波动率：      {:.0}%",
        risk_config.max_volatility * 100.0
    );
    println!();
    println!("── 仓位策略 ──");
    println!();
    println!("  当前策略：{}", risk_config.position_sizer.as_zh());
    println!();

    // 示例：模拟几个场景
    println!("── 示例场景 ──");
    println!();
    println!("场景1：正常交易（流动性充足、无亏损、低持仓）");
    println!("  预期：✅ 接受");
    println!();
    println!("场景2：连续亏损达到上限");
    println!("  原因：连续亏损达到限制");
    println!("  预期：❌ 拒绝");
    println!();
    println!("场景3：市场流动性不足");
    println!("  原因：市场流动性不足");
    println!("  预期：⚠️ 需审核 或 ❌ 拒绝");
    println!();
    println!("场景4：资金占用超过限制");
    println!("  原因：资金占用超过限制");
    println!("  预期：❌ 拒绝");
    println!();

    Ok(())
}

/// `cargo run -- risk-replay`：回放历史风险。
async fn run_risk_replay(cfg: &pm_models::Config) -> Result<()> {
    let risk_config = RiskConfig::from_pm_config(cfg);
    let mut replay = RiskReplay::new(risk_config);

    let now = chrono::Local::now();

    // 从历史机会 CSV 构建回放数据（如果有的话）
    println!("正在读取历史数据...");
    println!();

    // 简化版：使用几个模拟场景展示回放功能
    let scenarios = vec![
        (
            "正常场景",
            TradeSuggestion::new(
                "mkt-normal",
                "正常市场机会",
                pm_core::Side::Buy,
                0.45,
                100.0,
                "DefaultStrategy",
            ),
            {
                let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
                ctx.market_liquidity = 50000.0;
                ctx.suggested_price = 0.45;
                ctx.suggested_notional = 100.0;
                ctx
            },
        ),
        (
            "亏损场景",
            TradeSuggestion::new(
                "mkt-loss",
                "亏损场景机会",
                pm_core::Side::Buy,
                0.45,
                100.0,
                "DefaultStrategy",
            ),
            {
                let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
                ctx.daily_realized_pnl = -1200.0;
                ctx.market_liquidity = 50000.0;
                ctx.suggested_price = 0.45;
                ctx.suggested_notional = 100.0;
                ctx
            },
        ),
        (
            "满仓场景",
            TradeSuggestion::new(
                "mkt-full",
                "满仓场景机会",
                pm_core::Side::Buy,
                0.45,
                100.0,
                "DefaultStrategy",
            ),
            {
                let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
                ctx.open_position_count = 10;
                ctx.market_liquidity = 50000.0;
                ctx.suggested_price = 0.45;
                ctx.suggested_notional = 100.0;
                ctx
            },
        ),
        (
            "连续亏损场景",
            TradeSuggestion::new(
                "mkt-streak",
                "连续亏损场景",
                pm_core::Side::Buy,
                0.45,
                100.0,
                "DefaultStrategy",
            ),
            {
                let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
                ctx.consecutive_losses = 5;
                ctx.market_liquidity = 50000.0;
                ctx.suggested_price = 0.45;
                ctx.suggested_notional = 100.0;
                ctx
            },
        ),
        (
            "低流动性场景",
            TradeSuggestion::new(
                "mkt-illiq",
                "低流动性场景",
                pm_core::Side::Buy,
                0.45,
                100.0,
                "DefaultStrategy",
            ),
            {
                let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
                ctx.market_liquidity = 50.0;
                ctx.suggested_price = 0.45;
                ctx.suggested_notional = 100.0;
                ctx
            },
        ),
    ];

    let history: Vec<_> = scenarios
        .into_iter()
        .map(|(name, sug, ctx)| {
            println!("回放: {} ...", name);
            (now, sug, ctx)
        })
        .collect();

    let _records = replay.replay(&history);

    println!();
    println!("{}", replay.report_zh());
    println!();

    // 保存回放 CSV
    let replay_path = format!("{}/risk_replay.csv", cfg.paths.data_dir);
    replay.save_csv(&replay_path)?;
    println!("回放记录已保存至: {}", replay_path);
    println!();

    Ok(())
}

// ============================================================================
// V1.06 执行引擎 CLI 命令
// ============================================================================

/// `cargo run -- orders`：显示所有订单列表（中文，经 Gateway）。
async fn run_orders(cfg: &pm_models::Config) -> Result<()> {
    let gw_cfg = GatewayConfig::from_raw(&cfg.gateway);
    let gateway = create_gateway(&gw_cfg);
    println!("【订单列表】");
    println!();
    println!(
        "  Gateway      : {} ({})",
        gateway.name(),
        gateway.gateway_type()
    );
    println!(
        "  模式         : {}",
        if gateway.live_enabled() {
            "⚠️ 真实交易"
        } else {
            "🔒 模拟交易"
        }
    );
    println!();

    let info = gateway.info();
    println!("  {}", info.summary_zh());
    println!();

    // 经 Gateway 查询活跃订单
    let orders = gateway.list_orders().await;
    if orders.is_empty() {
        println!("（暂无活跃订单）");
        println!();
        println!("提示：运行 `cargo run -- scan` 或 `cargo run -- execution-test` 生成订单数据。");
    } else {
        println!("  活跃订单: {} 个", orders.len());
        println!();
        for (i, o) in orders.iter().enumerate() {
            println!(
                "  {}. {} | 状态: {} | 成交: {:.2} | {}",
                i + 1,
                o.gateway_order_id,
                o.status.as_zh(),
                o.filled,
                o.message,
            );
        }
    }

    // 历史订单 CSV
    let count = pm_storage::count_rows(&cfg.paths.execution_csv);
    println!();
    println!("  历史订单数（CSV）: {}", count);
    if count > 0 {
        println!("  查看历史订单：{}", cfg.paths.execution_csv);
    }

    Ok(())
}

/// `cargo run -- execution`：显示 Execution 状态（中文）。
fn run_execution_status(cfg: &pm_models::Config) -> Result<()> {
    use pm_execution::ExecutionConfigV106;

    let exec_cfg = ExecutionConfigV106::from_pm_config(&cfg.execution);
    let qc = exec_cfg.to_queue_config();
    let sc = exec_cfg.to_scheduler_config();

    println!("【Execution 状态】");
    println!();
    println!("══════════════════════════════════════");
    println!();
    println!("── 配置 ──");
    println!();
    println!("  Gateway        : {}", exec_cfg.gateway);
    println!("  初始资金       : {:.0} USDC", exec_cfg.capital);
    println!("  单笔成本       : {:.0} USDC", exec_cfg.order_notional);
    println!("  待处理上限     : {}", exec_cfg.max_pending_orders);
    println!("  订单超时       : {} ms", exec_cfg.timeout_ms);
    println!();
    println!("── 队列 ──");
    println!();
    println!("  最大容量       : {}", qc.max_size);
    println!("  最大重试       : {} 次", qc.max_retries);
    println!("  重试延迟       : {} ms", qc.retry_delay_ms);
    println!();
    println!("── 调度器 ──");
    println!();
    println!("  每秒上限       : {} 单", sc.max_orders_per_second);
    println!("  突发容量       : {} 单", sc.burst_size);
    println!();
    println!("── CSV ──");
    println!();
    println!("  订单记录       : {}", cfg.paths.execution_csv);
    println!("  事件记录       : {}", cfg.execution.events_csv);
    println!("  报告           : {}", cfg.execution.report_csv);
    println!();
    println!("══════════════════════════════════════");
    println!();
    println!("仅模拟 -- Execution Engine V1.06");
    println!();
    println!("提示：运行 `cargo run -- scan` 启动完整流水线。");
    println!("      运行 `cargo run -- queue` 查看队列。");
    println!("      运行 `cargo run -- orders` 查看订单。");

    Ok(())
}

/// `cargo run -- queue`：查看队列状态（中文）。
fn run_queue_status(cfg: &pm_models::Config) -> Result<()> {
    use pm_execution::{ExecutionConfigV106, ExecutionQueue};

    let exec_cfg = ExecutionConfigV106::from_pm_config(&cfg.execution);
    let qc = exec_cfg.to_queue_config();
    let q = ExecutionQueue::new(qc);

    q.print_status();
    println!();
    println!("提示：运行 `cargo run -- scan` 启动扫描并观察队列实时状态。");

    Ok(())
}

/// `cargo run -- exec-replay <order_id>`：回放指定订单的生命周期。
fn run_exec_replay(cfg: &pm_models::Config, order_id: &str) -> Result<()> {
    use pm_execution::OrderReplay;

    println!("订单回放: {}", order_id);
    println!();

    // 尝试从事件 CSV 加载
    let events_path = &cfg.execution.events_csv;
    match OrderReplay::from_csv(events_path) {
        Ok(replay) => {
            replay.print_timeline(order_id);
        }
        Err(_) => {
            println!("无法读取事件文件: {}", events_path);
            println!();
            println!("提示：运行 `cargo run -- scan` 生成执行事件。");
        }
    }

    Ok(())
}

// ============================================================================
// V1.07 Trading Infrastructure CLI 命令
// ============================================================================

/// `cargo run -- provider`：查看 Provider 诊断信息。
async fn run_trading_provider(_cfg: &pm_models::Config) -> Result<()> {
    let trading_cfg = TradingConfig::load_or_default("provider.toml");
    println!("{}", trading_cfg.safe_summary());
    println!();

    let provider = MockTradingProvider::new();
    let output = diagnose_provider(&provider).await;
    println!("{}", output);
    Ok(())
}

/// `cargo run -- health`：查看 Health 诊断信息。
async fn run_trading_health(_cfg: &pm_models::Config) -> Result<()> {
    let trading_cfg = TradingConfig::load_or_default("provider.toml");
    println!("{}", trading_cfg.safe_summary());
    println!();

    let provider = MockTradingProvider::new();
    let output = diagnose_health(&provider).await;
    println!("{}", output);
    Ok(())
}

/// `cargo run -- session`：查看 Session 诊断信息。
fn run_trading_session(_cfg: &pm_models::Config) -> Result<()> {
    let provider = MockTradingProvider::new();
    let output = diagnose_session(&provider.session_manager);
    println!("{}", output);
    Ok(())
}

/// `cargo run -- connection`：查看 Connection 诊断信息。
fn run_trading_connection(_cfg: &pm_models::Config) -> Result<()> {
    let provider = MockTradingProvider::new();
    let output = diagnose_connection(&provider.connection_manager);
    println!("{}", output);

    println!();
    let cred_output = diagnose_credential(&provider.credential_manager);
    println!("{}", cred_output);
    Ok(())
}

// ============================================================================
// V1.08 Exchange Gateway CLI 命令
// ============================================================================

/// `cargo run -- gateway`：查看 Gateway 状态（中文）。
async fn run_gateway(cfg: &pm_models::Config) -> Result<()> {
    let gw_cfg = GatewayConfig::from_raw(&cfg.gateway);
    println!("{}", gw_cfg.safety_summary_zh());
    println!();
    println!("{}", gw_cfg.summary_zh());
    println!();

    let gateway = create_gateway(&gw_cfg);
    let diag = diagnose_gateway(gateway.as_ref()).await;
    println!("{}", diag);
    println!();

    // 断路器状态
    println!("提示：使用 `cargo run -- account` 查看账户详情。");
    println!("      使用 `cargo run -- balance` 查看余额。");
    println!("      使用 `cargo run -- orders` 查看活跃订单。");

    Ok(())
}

/// `cargo run -- account`：查看账户诊断（中文）。
async fn run_account(cfg: &pm_models::Config) -> Result<()> {
    let gw_cfg = GatewayConfig::from_raw(&cfg.gateway);
    let gateway = create_gateway(&gw_cfg);

    let diag = diagnose_account(gateway.as_ref()).await;
    println!("{}", diag);

    Ok(())
}

/// `cargo run -- balance`：查看余额（中文）。
async fn run_balance(cfg: &pm_models::Config) -> Result<()> {
    let gw_cfg = GatewayConfig::from_raw(&cfg.gateway);
    let gateway = create_gateway(&gw_cfg);

    let diag = diagnose_balance(gateway.as_ref()).await;
    println!("{}", diag);

    Ok(())
}

// ============================================================================
// P2-04 OMS CLI 命令
// ============================================================================

/// 构建带 CSV 持久化的 OMS 实例（共享 orders.csv / events.csv）。
fn build_cli_oms(cfg: &pm_models::Config) -> anyhow::Result<Oms> {
    use std::sync::Arc;
    let gw_cfg = GatewayConfig::from_raw(&cfg.gateway);
    let gateway: Arc<dyn pm_gateway::ExchangeGateway> = Arc::from(create_gateway(&gw_cfg));
    let orders_csv = std::path::PathBuf::from(format!("{}/oms_orders.csv", cfg.paths.data_dir));
    let events_csv = std::path::PathBuf::from(format!("{}/oms_events.csv", cfg.paths.data_dir));
    let oms_cfg = OmsConfig {
        repository_type: pm_oms::prelude::RepositoryType::Csv,
        orders_csv: Some(orders_csv),
        events_csv: Some(events_csv),
        sqlite_path: None,
        auto_recover: false,
        subscribe_metrics: true,
    };
    Oms::new(oms_cfg, gateway)
}

/// `cargo run -- oms`：OMS 健康概览（中文）。
async fn run_oms_overview(cfg: &pm_models::Config) -> Result<()> {
    let oms = build_cli_oms(cfg)?;
    println!();
    println!("{}", oms.health().await);
    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  状态机（11 态 + 1 聚合）");
    println!("═══════════════════════════════════════════════════════════");
    for line in pm_oms::prelude::StateMachine::diagram_zh().lines() {
        println!("  {}", line);
    }
    println!();
    println!("提示：");
    println!("  cargo run -- oms-orders      查看所有 OMS 订单");
    println!("  cargo run -- oms-order <id>  查看单个订单详情");
    println!("  cargo run -- oms-events      查看订单事件流");
    println!("  cargo run -- oms-demo        创建 5 个示例订单（演示用）");
    println!();
    Ok(())
}

/// `cargo run -- oms-orders`：OMS 订单列表（中文）。
async fn run_oms_orders(cfg: &pm_models::Config) -> Result<()> {
    let oms = build_cli_oms(cfg)?;
    let orders = oms.list_orders()?;
    println!();
    println!("【OMS 订单列表】");
    println!();
    println!(
        "  Gateway : {} ({})",
        oms.gateway().name(),
        oms.gateway().gateway_type()
    );
    println!(
        "  模式    : {}",
        if oms.gateway().live_enabled() {
            "⚠️ 真实交易"
        } else {
            "🔒 模拟交易"
        }
    );
    println!("  仓库    : {:?}", oms.repository().name());
    println!();

    if orders.is_empty() {
        println!("（暂无 OMS 订单）");
        println!();
        println!("提示：运行 `cargo run -- oms-demo` 创建示例订单。");
        println!();
        return Ok(());
    }

    // 表头
    println!(
        "  {:<18} {:<14} {:<10} {:<8} {:<10} {:<10} {:<10}",
        "OMS ID", "客户端 ID", "状态", "方向", "价格", "数量", "成交率"
    );
    println!("  {}", "─".repeat(98));

    for o in &orders {
        let filled_pct = o.fill_rate() * 100.0;
        println!(
            "  {:<18} {:<14} {:<10} {:<8} {:<10.4} {:<10.2} {:<10.1}",
            o.order_id,
            o.client_order_id,
            o.status.as_zh(),
            format!("{} {}", o.direction.as_zh(), o.side.as_str()),
            o.price,
            o.quantity,
            filled_pct,
        );
    }
    println!();

    // 状态汇总
    let counts = oms.repository().count_by_status()?;
    println!("  ── 状态分布 ──");
    for (s, c) in counts {
        println!("    {} : {}", s.as_zh(), c);
    }
    println!();

    // Metrics
    let m = oms.metrics_snapshot();
    println!("  ── Metrics ──");
    println!("{}", m.summary_zh());
    println!();

    Ok(())
}

/// `cargo run -- oms-order <id>`：单个 OMS 订单详情。
async fn run_oms_order(cfg: &pm_models::Config, target: &str) -> Result<()> {
    let oms = build_cli_oms(cfg)?;
    let order = oms.get_order(target)?;
    let Some(o) = order else {
        // 尝试 client_order_id
        if let Some(o2) = oms.get_order_by_client_id(target)? {
            o2.print_timeline();
            return Ok(());
        }
        println!();
        println!("未找到订单: {}", target);
        println!();
        println!("提示：");
        println!("  - 运行 `cargo run -- oms-orders` 查看所有订单 ID");
        println!("  - 支持 OMS ID 或客户端订单 ID 模糊匹配（仅 client_order_id 完全匹配）");
        println!();
        return Ok(());
    };
    println!();
    o.print_timeline();
    Ok(())
}

/// `cargo run -- oms-events`：OMS 事件流（从 CSV 读取最近 50 条）。
async fn run_oms_events(cfg: &pm_models::Config) -> Result<()> {
    let oms = build_cli_oms(cfg)?;
    let events = oms.repository().list_events()?;
    println!();
    println!("【OMS 事件流】");
    println!();
    println!("  总事件数: {}", events.len());
    println!();

    if events.is_empty() {
        println!("（暂无事件）");
        println!();
        println!("提示：运行 `cargo run -- oms-demo` 生成示例事件。");
        println!();
        return Ok(());
    }

    // 取最近 50 条
    let start = events.len().saturating_sub(50);
    println!(
        "  {:<22} {:<20} {:<22} {}",
        "时间戳", "事件类型", "订单 ID", "extra"
    );
    println!("  {}", "─".repeat(98));
    for e in &events[start..] {
        let ts = e.timestamp().format("%Y-%m-%d %H:%M:%S").to_string();
        let oid = e.order_id();
        let oid_short = if oid.len() > 20 { &oid[..20] } else { oid };
        let extra = serde_json::to_string(e).unwrap_or_default();
        let extra_short = if extra.len() > 40 {
            format!("{}...", &extra[..40])
        } else {
            extra
        };
        println!(
            "  {:<22} {:<20} {:<22} {}",
            ts,
            e.event_name_zh(),
            oid_short,
            extra_short,
        );
    }
    println!();
    Ok(())
}

/// `cargo run -- oms-demo`：创建 5 个示例订单用于演示。
async fn run_oms_demo(cfg: &pm_models::Config) -> Result<()> {
    use chrono::Local;
    let oms = build_cli_oms(cfg)?;
    println!();
    println!("正在创建 5 个示例 OMS 订单...");
    println!();

    let scenarios: Vec<(&str, &str, f64, f64)> = vec![
        ("CLI-DEMO-001", "mkt-btc-2024", 0.55, 100.0),
        ("CLI-DEMO-002", "mkt-eth-2024", 0.45, 200.0),
        ("CLI-DEMO-003", "mkt-btc-2024", 0.50, 150.0),
        ("CLI-DEMO-004", "mkt-sol-2024", 0.65, 50.0),
        ("CLI-DEMO-005", "mkt-eth-2024", 0.40, 80.0),
    ];

    let now = Local::now();
    for (cid, mid, price, qty) in scenarios {
        let input = CreateOrderInput::limit(
            cid,
            mid,
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            price,
            qty,
            "DefaultStrategy",
            "RISK-001",
            "OPP-DEMO",
        );
        match oms.create_order(&input, now) {
            Ok(o) => println!(
                "  ✓ 创建订单 {} ({} @ {:.4} × {:.2})",
                o.order_id, mid, price, qty
            ),
            Err(e) => println!("  ✗ 创建失败 {}：{}", cid, e),
        }
    }
    println!();
    println!("提示：");
    println!("  cargo run -- oms-orders    查看刚创建的订单");
    println!("  cargo run -- oms-events    查看事件流");
    println!();
    Ok(())
}

// ============================================================================
// P2-05 PMS CLI 命令
// ============================================================================

/// 构建 PMS 实例（Memory 仓库 + 模拟数据）。
fn build_cli_pms(cfg: &pm_models::Config) -> anyhow::Result<pm_pms::Pms> {
    let config = pm_pms::PmsConfig {
        repository_type: pm_pms::prelude::RepositoryType::Memory,
        portfolio_csv: None,
        positions_csv: None,
        pnl_csv: None,
        initial_capital: cfg.portfolio.initial_capital,
        subscribe_to_oms: false, // CLI 独立运行，不订阅 OMS
    };
    let repo = pm_pms::prelude::create_repository(
        config.repository_type,
        config.portfolio_csv.clone(),
        config.positions_csv.clone(),
        config.pnl_csv.clone(),
    )?;
    pm_pms::Pms::new(config, repo)
}

/// `cargo run -- portfolio`：查看投资组合。
fn run_portfolio(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut pms = build_cli_pms(cfg)?;
    // 添加一些示例数据
    let now = chrono::Local::now();
    let _ = pms.handle_order_filled(
        "OMS-DEMO-001",
        "mkt-btc-2024",
        pm_pms::prelude::Direction::Yes,
        pm_core::Side::Buy,
        0.55,
        100.0,
    );
    let _ = pms.handle_order_filled(
        "OMS-DEMO-002",
        "mkt-eth-2024",
        pm_pms::prelude::Direction::No,
        pm_core::Side::Buy,
        0.45,
        200.0,
    );
    // mark-to-market
    pms.position_mgr
        .mark_position("mkt-btc-2024", pm_pms::prelude::Direction::Yes, 0.60, now);
    pms.revalue_all(now);
    pms.print_dashboard();
    Ok(())
}

/// `cargo run -- positions`：查看全部持仓。
fn run_positions(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut pms = build_cli_pms(cfg)?;
    let now = chrono::Local::now();
    let _ = pms.handle_order_filled(
        "OMS-001",
        "mkt-btc-2024",
        pm_pms::prelude::Direction::Yes,
        pm_core::Side::Buy,
        0.55,
        100.0,
    );
    let _ = pms.handle_order_filled(
        "OMS-002",
        "mkt-eth-2024",
        pm_pms::prelude::Direction::No,
        pm_core::Side::Buy,
        0.45,
        200.0,
    );
    let _ = pms.handle_order_filled(
        "OMS-003",
        "mkt-sol-2024",
        pm_pms::prelude::Direction::Yes,
        pm_core::Side::Buy,
        0.65,
        50.0,
    );
    pms.position_mgr
        .mark_position("mkt-btc-2024", pm_pms::prelude::Direction::Yes, 0.60, now);
    pms.revalue_all(now);
    pms.print_positions();
    Ok(())
}

/// `cargo run -- pnl`：查看盈亏报告。
fn run_pnl(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut pms = build_cli_pms(cfg)?;
    let now = chrono::Local::now();
    // 创建几个模拟持仓并平仓以展示盈亏
    let _ = pms.handle_order_filled(
        "OMS-001",
        "mkt-btc",
        pm_pms::prelude::Direction::Yes,
        pm_core::Side::Buy,
        0.50,
        100.0,
    );
    let _ = pms.handle_order_filled(
        "OMS-002",
        "mkt-eth",
        pm_pms::prelude::Direction::No,
        pm_core::Side::Buy,
        0.40,
        100.0,
    );
    // 模拟平仓
    let _ = pms
        .position_mgr
        .close_position("mkt-btc", pm_pms::prelude::Direction::Yes, 0.55, now);
    let _ = pms
        .position_mgr
        .close_position("mkt-eth", pm_pms::prelude::Direction::No, 0.35, now);
    pms.revalue_all(now);
    pms.print_pnl();
    Ok(())
}

/// `cargo run -- exposure`：查看风险敞口。
fn run_exposure(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut pms = build_cli_pms(cfg)?;
    let now = chrono::Local::now();
    let _ = pms.handle_order_filled(
        "OMS-001",
        "mkt-btc",
        pm_pms::prelude::Direction::Yes,
        pm_core::Side::Buy,
        0.55,
        100.0,
    );
    let _ = pms.handle_order_filled(
        "OMS-002",
        "mkt-eth",
        pm_pms::prelude::Direction::No,
        pm_core::Side::Buy,
        0.45,
        200.0,
    );
    pms.position_mgr
        .mark_position("mkt-btc", pm_pms::prelude::Direction::Yes, 0.60, now);
    pms.revalue_all(now);
    pms.print_exposure();
    Ok(())
}

// ============================================================================
// P2-02 API Workflow CLI 命令
// ============================================================================

/// `cargo run -- workflow [dryrun|replay|live]`：API Workflow 引擎。
///
/// - 无子参：显示当前 Workflow 配置 + 状态机 + 最近报告。
/// - `dryrun`：执行 DryRun Workflow（默认，无网络、无真实下单）。
/// - `replay`：执行 Replay Workflow（从 fixtures/ 回放）。
/// - `live`：执行 Live ReadOnly Workflow（真实只读，禁止下单/撤单）。
async fn run_workflow(sub: &str) -> Result<()> {
    use pm_api_workflow::config::{WorkflowConfig, WorkflowMode};
    use pm_api_workflow::report::ReportGenerator;
    use pm_api_workflow::state_machine::StateMachine;

    let mut cfg = WorkflowConfig::load_or_default("workflow.toml");

    println!("═══════════════════════════════════════════════════════════");
    println!("{}", cfg.safety_summary_zh());
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("【状态机】");
    for line in StateMachine::diagram_zh().lines() {
        println!("  {}", line);
    }
    println!();

    match sub {
        "" => {
            println!("【当前 Workflow】{}", cfg.mode.as_zh());
            println!();
            println!("可用命令:");
            println!("  cargo run -- workflow          显示当前 Workflow（本视图）");
            println!(
                "  cargo run -- workflow dryrun   执行 DryRun Workflow（默认，无网络/无下单）"
            );
            println!("  cargo run -- workflow replay   执行 Replay Workflow（从 fixtures 回放）");
            println!("  cargo run -- workflow live     执行 Live ReadOnly Workflow（真实只读）");
            println!();
            let generator = ReportGenerator::new(&cfg.report_dir);
            if generator.report_exists() {
                let paths = generator.latest_paths();
                println!("最近报告: {}", paths.md_path);
            } else {
                println!("（暂无报告，运行 dryrun/replay/live 生成）");
            }
        }
        "dryrun" => {
            cfg.mode = WorkflowMode::DryRun;
            println!("执行 DryRun Workflow...");
            println!();
            let report = pm_api_workflow::run_dryrun(&cfg).await?;
            println!();
            println!("{}", report.summary_zh());
            println!("{}", report.validation.summary_zh());
            println!();
            println!("提示：报告已输出至 {}", cfg.report_dir);
        }
        "replay" => {
            cfg.mode = WorkflowMode::Replay;
            println!("执行 Replay Workflow（从 fixtures 回放）...");
            println!();
            let report = pm_api_workflow::run_replay(&cfg).await?;
            println!();
            println!("{}", report.summary_zh());
            println!("{}", report.validation.summary_zh());
            println!();
            println!("提示：报告已输出至 {}", cfg.report_dir);
        }
        "live" => {
            cfg.mode = WorkflowMode::LiveReadOnly;
            println!("执行 Live ReadOnly Workflow（真实只读，禁止下单/撤单）...");
            println!();
            let report = pm_api_workflow::run_live_readonly(&cfg).await?;
            println!();
            println!("{}", report.summary_zh());
            println!("{}", report.validation.summary_zh());
            println!();
            println!("提示：报告已输出至 {}", cfg.report_dir);
        }
        other => {
            println!("未知 Workflow 子命令: {}", other);
            println!("可用: dryrun | replay | live");
        }
    }

    Ok(())
}

// ============================================================================
// P2-06 Auth & Wallet Infrastructure CLI 命令
// ============================================================================

/// `cargo run -- auth health`：认证健康检查（P2-06）。
async fn run_auth_health(_cfg: &pm_models::Config) -> Result<()> {
    let auth = pm_auth::create_default_auth()?;
    let output = diagnose_auth_health(&auth).await;
    println!("{}", output);
    Ok(())
}

/// `cargo run -- auth session`：会话诊断（P2-06）。
async fn run_auth_session(_cfg: &pm_models::Config) -> Result<()> {
    let auth = pm_auth::create_default_auth()?;
    let output = diagnose_auth_session(&auth).await;
    println!("{}", output);
    Ok(())
}

/// `cargo run -- auth credential`：凭据诊断（P2-06）。
async fn run_auth_credential(_cfg: &pm_models::Config) -> Result<()> {
    let auth = pm_auth::create_default_auth()?;
    let cred_mgr = auth.credential_manager();
    let output = diagnose_auth_credential(cred_mgr);
    println!("{}", output);
    Ok(())
}

/// `cargo run -- wallet health`：钱包健康检查（P2-06）。
fn run_wallet_health(_cfg: &pm_models::Config) -> Result<()> {
    let (wallet, account_mgr, balance_mgr, allowance_mgr) = pm_wallet::create_default_wallet()?;
    let output = diagnose_wallet_health(&wallet, &account_mgr, &balance_mgr, &allowance_mgr);
    println!("{}", output);
    Ok(())
}

/// `cargo run -- wallet balance`：余额查询（P2-06）。
fn run_wallet_balance(_cfg: &pm_models::Config) -> Result<()> {
    let (_wallet, _account_mgr, balance_mgr, _allowance_mgr) = pm_wallet::create_default_wallet()?;
    let output = diagnose_wallet_balance(&balance_mgr);
    println!("{}", output);
    Ok(())
}

/// `cargo run -- wallet account`：账户列表（P2-06）。
fn run_wallet_accounts(_cfg: &pm_models::Config) -> Result<()> {
    let (wallet, account_mgr, _balance_mgr, _allowance_mgr) = pm_wallet::create_default_wallet()?;
    let output = diagnose_wallet_accounts(&wallet, &account_mgr);
    println!("{}", output);
    Ok(())
}

// ============================================================================
// P2-06 Settlement Engine CLI 命令
// ============================================================================

/// 构建 Settlement Engine（Memory 仓库 + 模拟数据）。
fn build_cli_settlement(cfg: &pm_models::Config) -> anyhow::Result<SettlementEngine> {
    use pm_settlement::prelude::*;
    let config = SettlementConfig {
        initial_capital: cfg.portfolio.initial_capital,
        default_account_id: "ACCT-MAIN-001".to_string(),
        fee_rule: FeeRule::zero_fee(),
        enable_event_bus: true,
    };
    let repo = pm_settlement::prelude::create_repository(
        pm_settlement::prelude::RepositoryType::Memory,
        None,
        None,
        None,
    )?;
    SettlementEngine::new(config, repo)
}

/// `cargo run -- settlement`：查看最近结算。
async fn run_settlement(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut engine = build_cli_settlement(cfg)?;

    // 注入模拟数据
    let now = chrono::Local::now();
    let demo_fills = vec![
        (
            "T-DEMO-001",
            "OMS-DEMO-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            0.55,
            100.0,
        ),
        (
            "T-DEMO-002",
            "OMS-DEMO-002",
            "mkt-eth",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            0.45,
            200.0,
        ),
        (
            "T-DEMO-003",
            "OMS-DEMO-003",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Sell,
            0.60,
            50.0,
        ),
        (
            "T-DEMO-004",
            "OMS-DEMO-004",
            "mkt-sol",
            pm_execution::order::Direction::No,
            pm_core::Side::Buy,
            0.30,
            150.0,
        ),
        (
            "T-DEMO-005",
            "OMS-DEMO-005",
            "mkt-eth",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            0.48,
            100.0,
        ),
    ];

    for (tid, oid, mid, dir, side, price, qty) in &demo_fills {
        let fill = TradeFillEvent {
            trade_id: tid.to_string(),
            order_id: oid.to_string(),
            client_order_id: format!("CLI-{}", oid),
            exchange_order_id: None,
            market_id: mid.to_string(),
            account_id: "ACCT-MAIN-001".to_string(),
            direction: *dir,
            side: *side,
            fill_price: *price,
            fill_quantity: *qty,
            filled_at: now,
            is_taker: true,
            gateway_name: "MockGateway".to_string(),
        };
        let result = engine.process_fill(&fill);
        println!(
            "  {} {} → {} | 余额: {:.2} → {:.2} | {}",
            if result.status.is_success() {
                "✓"
            } else {
                "✗"
            },
            result.trade_id,
            result.status.as_zh(),
            result.balance_before,
            result.balance_after,
            result.position_summary.as_deref().unwrap_or("-"),
        );
    }
    println!();

    engine.print_dashboard();
    Ok(())
}

/// `cargo run -- ledger`：查看资金流水。
fn run_ledger(cfg: &pm_models::Config) -> anyhow::Result<()> {
    let mut engine = build_cli_settlement(cfg)?;
    let now = chrono::Local::now();

    // 注入模拟数据
    let fills = vec![
        (
            "T-001",
            "OMS-001",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            0.55,
            100.0,
        ),
        (
            "T-002",
            "OMS-002",
            "mkt-eth",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Buy,
            0.45,
            200.0,
        ),
        (
            "T-003",
            "OMS-003",
            "mkt-btc",
            pm_execution::order::Direction::Yes,
            pm_core::Side::Sell,
            0.60,
            100.0,
        ),
    ];

    for (tid, oid, mid, dir, side, price, qty) in &fills {
        let fill = TradeFillEvent {
            trade_id: tid.to_string(),
            order_id: oid.to_string(),
            client_order_id: format!("CLI-{}", oid),
            exchange_order_id: None,
            market_id: mid.to_string(),
            account_id: "ACCT-MAIN-001".to_string(),
            direction: *dir,
            side: *side,
            fill_price: *price,
            fill_quantity: *qty,
            filled_at: now,
            is_taker: true,
            gateway_name: "MockGateway".to_string(),
        };
        engine.process_fill(&fill);
    }

    engine.ledger.print_zh(20);
    Ok(())
}

/// `cargo run -- fees`：查看手续费规则。
fn run_fees(_cfg: &pm_models::Config) -> anyhow::Result<()> {
    use pm_settlement::prelude::*;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  手续费系统");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    let standard = FeeRule::default();
    println!("── 标准费率 ──");
    println!("{}", standard.display_zh());
    println!();

    let zero = FeeRule::zero_fee();
    println!("── 零费率（模拟环境）──");
    println!("{}", zero.display_zh());
    println!();

    // 模拟几次成交的手续费
    let engine = FeeEngine::default();
    println!("── 手续费示例 ──");
    println!();

    let examples = vec![
        ("小额定单 (Taker)", 0.50, 100.0, true),
        ("小额定单 (Maker)", 0.50, 100.0, false),
        ("中额定单 (Taker)", 0.55, 1000.0, true),
        ("大额定单 (Taker)", 0.60, 10000.0, true),
    ];

    for (label, price, qty, is_taker) in &examples {
        let notional = price * qty;
        let rate = if *is_taker {
            engine.active_rule.taker_rate
        } else {
            engine.active_rule.maker_rate
        };
        let fee = notional * rate;
        let role = if *is_taker { "Taker" } else { "Maker" };
        println!(
            "  {}: {:.4} × {:.0} = {:.2} USDC | 费率 {:.2}% | 手续费 {:.4} USDC | {}",
            label,
            price,
            qty,
            notional,
            rate * 100.0,
            fee,
            role
        );
    }
    println!();

    println!("═══════════════════════════════════════════════════════════");
    println!();
    Ok(())
}

// ============================================================================
// P3.0 多市场统一框架 CLI 命令
// ============================================================================

/// 创建一个带有示例注册插件的 MarketFramework 实例（演示用）。
fn create_demo_framework() -> MarketFramework {
    let fw = MarketFramework::new();
    tracing::info!("创建多市场框架演示实例");

    // 未来在这里注册真实的市场插件（PolymarketPlugin、BinancePlugin 等）
    // 当前为框架演示，展示注册表和发现功能

    fw
}

/// `cargo run -- markets`：列出所有已安装市场（P3.0）。
async fn run_markets() -> Result<()> {
    let fw = create_demo_framework();

    println!("══════ 多市场统一框架（P3.0）══════");
    println!();
    println!("{}", fw.registry().render_table_zh());
    println!();
    println!("{}", Discovery::discover_and_report(fw.registry()));

    // 能力矩阵
    println!();
    println!("── 能力说明 ──");
    println!();
    println!("每个市场声明自己的能力，系统根据能力自动启用功能。");
    println!("当前支持的能力类别：");
    println!("  📊 数据能力：ReadMarket / ReadOrderBook / ReadTrades / HistoricalData");
    println!(
        "  💹 交易能力：PaperTrading / LiveTrading / CancelOrder / ReplaceOrder / BatchOrders"
    );
    println!("  💰 账户能力：Wallet / Balance / Settlement");
    println!("  📡 传输能力：Rest / WebSocket / Streaming / FIX");
    println!("  🏷️  市场类型：Spot / Margin / Perpetual / Futures / Options / Prediction");
    println!("  🔧 扩展能力：MultiAsset / MultiChain / CrossMargin / IsolatedMargin");
    println!("  ⭐ 高级能力：Leverage / Staking / Launchpad");
    println!();
    println!("── 预定义能力模板 ──");
    println!();
    println!("  🎲 预测市场（Polymarket风格）：{}", {
        let caps = CapabilitySet::prediction_market_full();
        let names: Vec<String> = caps
            .list_all()
            .iter()
            .map(|c| c.as_zh().to_string())
            .collect();
        names.join(" / ")
    });
    println!();
    println!("  📈 现货交易所（Binance风格）：{}", {
        let caps = CapabilitySet::spot_exchange_full();
        let names: Vec<String> = caps
            .list_all()
            .iter()
            .map(|c| c.as_zh().to_string())
            .collect();
        names.join(" / ")
    });
    println!();
    println!("── 未来市场 ──");
    println!();
    println!(
        "  计划支持：Polymarket / Kalshi / Binance / OKX / Bybit / Hyperliquid / Uniswap / Raydium"
    );
    println!("  新增市场仅需：新增 provider + adapter + gateway + 实现 MarketPlugin");
    println!("  不得修改：Strategy / Risk / OMS / Settlement / PMS / Infrastructure");
    println!();

    Ok(())
}

/// `cargo run -- markets health`：多市场健康检查（P3.0 第七节）。
async fn run_market_health() -> Result<()> {
    let fw = create_demo_framework();

    println!("══════ 多市场健康检查（P3.0）══════");
    println!();

    // 健康检查报告
    let health_report = MarketHealthReport::healthy("多市场框架");

    println!("{}", health_report.report_zh());
    println!();

    // 各维度说明
    println!("── 健康检查维度 ──");
    println!();
    println!("  ✅ REST API       — HTTP/HTTPS 连接状态");
    println!("  ✅ WebSocket      — 实时推送连接状态");
    println!("  ✅ 网关           — 交易所网关状态");
    println!("  ✅ 认证           — API Key / Token 有效性");
    println!("  ✅ 延迟           — 请求响应时间");
    println!("  ✅ 流数据         — gRPC stream / SSE 状态");
    println!();
    println!("  未来每个市场插件实现健康检查后，`markets health`");
    println!("  将汇总所有已注册市场的健康状态。");
    println!();

    // 诊断报告
    let diag = fw.generate_report();
    println!("── 诊断概览 ──");
    println!();
    println!("  已安装市场: {} 个", fw.registry().count());
    println!("  注册表状态: 正常");
    println!("  已知限制: {} 项", diag.known_limitations.len());
    println!("  优化建议: {} 项", diag.optimization_suggestions.len());
    println!();

    Ok(())
}

fn print_usage() {
    println!();
    println!("用法:");
    println!();
    println!("  cargo run -- scan            正常扫描 + 纸面交易 + 执行模拟器");
    println!("  cargo run -- diagnose        诊断模式（单次扫描 + 完整诊断报告，不进入循环）");
    println!(
        "  cargo run -- datasource      数据源诊断（Provider / 能力 / 健康 / 缓存 / 校验 / 快照）"
    );
    println!("  cargo run -- replay          历史回放");
    println!("  cargo run -- paper           基于历史机会的纸面交易");
    println!("  cargo run -- backtest        完整回测");
    println!("  cargo run -- execution-test  执行模拟器压测");
    println!("  cargo run -- report          汇总报告");
    println!();
    println!("  多市场框架（P3.0）：");
    println!("  cargo run -- markets          列出所有已安装市场");
    println!("  cargo run -- markets health   多市场健康检查");
    println!();
    println!("  市场微观结构（V1.03）：");
    println!("  cargo run -- market          市场列表");
    println!("  cargo run -- orderbook       订单簿");
    println!("  cargo run -- spread          价差分析");
    println!("  cargo run -- liquidity       流动性分析");
    println!();
    println!("  数据审计（V1.09）：");
    println!("  cargo run -- explain           完整数据链路分析报告");
    println!("  cargo run -- explain pipeline  同上");
    println!("  cargo run -- explain rejections  拒绝原因分析");
    println!("  cargo run -- explain <id>      解释某个机会的评分");
    println!("  cargo run -- audit             自动数据一致性审计");
    println!();
    println!("  机会引擎（V1.04）：");
    println!("  cargo run -- opportunities   列出全部机会");
    println!("  cargo run -- top             Top 10 机会");
    println!();
    println!("  风险引擎（V1.05）：");
    println!("  cargo run -- risk           风险仪表盘");
    println!("  cargo run -- explain-risk   风险规则说明");
    println!("  cargo run -- risk-replay    风险回放");
    println!();
    println!("  执行引擎（V1.06）：");
    println!("  cargo run -- orders         订单列表");
    println!("  cargo run -- execution      执行状态");
    println!("  cargo run -- queue          队列查看");
    println!("  cargo run -- exec-replay <id>  订单回放");
    println!();
    println!("  Trading 基础设施（V1.07）：");
    println!("  cargo run -- provider       Provider 诊断");
    println!("  cargo run -- health         Health 诊断");
    println!("  cargo run -- session        Session 诊断");
    println!("  cargo run -- connection     Connection 诊断");
    println!();
    println!("  Exchange Gateway（V1.08）：");
    println!("  cargo run -- gateway        Gateway 状态与诊断");
    println!("  cargo run -- account        账户详情（余额+持仓）");
    println!("  cargo run -- balance        余额查询");
    println!("  cargo run -- orders         订单列表（经 Gateway）");
    println!();
    println!("  API Workflow（P2-02）：");
    println!("  cargo run -- workflow       显示当前 Workflow（配置+状态机+最近报告）");
    println!("  cargo run -- workflow dryrun   执行 DryRun Workflow（默认，无网络/无下单）");
    println!("  cargo run -- workflow replay   执行 Replay Workflow（从 fixtures 回放）");
    println!("  cargo run -- workflow live     执行 Live ReadOnly Workflow（真实只读）");
    println!();
    println!("  OMS（P2-04）：");
    println!("  cargo run -- oms              OMS 健康概览 + 状态机图");
    println!("  cargo run -- oms-orders       OMS 订单列表（CSV 持久化）");
    println!("  cargo run -- oms-order <id>   OMS 订单详情（含状态历史）");
    println!("  cargo run -- oms-events       OMS 事件流");
    println!("  cargo run -- oms-demo         创建 5 个示例订单");
    println!();
    println!("  PMS（P2-05）：");
    println!("  cargo run -- portfolio       投资组合仪表盘");
    println!("  cargo run -- positions       全部持仓列表");
    println!("  cargo run -- pnl             盈亏报告");
    println!("  cargo run -- exposure        风险敞口报告");
    println!();
    println!("  认证与钱包（P2-06）：");
    println!("  cargo run -- auth health     认证健康诊断（凭据/会话/Token/认证）");
    println!("  cargo run -- auth session    会话诊断");
    println!("  cargo run -- auth credential 凭据诊断（脱敏）");
    println!("  cargo run -- wallet health   钱包健康诊断（钱包/余额/授权/Nonce）");
    println!("  cargo run -- wallet balance  余额查询");
    println!("  cargo run -- wallet account  账户列表（脱敏）");
    println!();
    println!("  结算引擎（P2-06）：");
    println!("  cargo run -- settlement      查看最近结算（含模拟数据）");
    println!("  cargo run -- ledger          查看资金流水");
    println!("  cargo run -- fees            查看手续费规则与示例");
}
