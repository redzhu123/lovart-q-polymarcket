//! 扫描循环 driver：拉取市场 -> 跟踪 -> 调用 Strategy 各 hook -> 写 CSV -> 更新 Metrics -> 渲染仪表盘。
//!
//! 被 `apps/scanner` 与 `apps/cli::scan` 共用（无 app->app 依赖）。
//! Simulation Only -- 单轮失败不退出，打印错误后继续下一轮；Ctrl+C 可中断。
//!
//! V1.0.1：增加完整可观测性（Observability）--
//! - 启动连通性检查 [`startup_check`]：Config / CSV / API / JSON 任一失败则不进入扫描循环。
//! - 每轮打印 Scanner Debug 区块（HTTP -> Markets -> Filter -> Price -> Strategy -> Opportunity）。
//! - Markets Received == 0 时打印 WARNING 并跳过 Strategy / Paper / Execution。
//! - [`ScannerStats`] 跨轮累计统计。
//!
//! V1.01：在 V1.0.1 基础上进一步强化可观测性，**不改变任何交易/策略/Shadow/Execution 逻辑**--
//! - 启动检查迁移至 [`crate::health`]，扩展 Storage / Clock / Memory（[`startup_check`] 委托）。
//! - 每轮各阶段用 [`crate::pipeline::Stopwatch`] 计时，产出 `ModuleStats`，打印执行时间线 + 流水线时间线。
//! - 输出按 `config.logging.log_level` 门控（ERROR/WARN/INFO/DEBUG/TRACE，默认 DEBUG）。
//! - 每轮打印系统汇总（System Summary）；TRACE 打印全量市场转储。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Local;
use tracing;

use pm_execution::{ExecEvent, ExecParams, ExecutionEngine};
use pm_metrics::Metrics;
use pm_models::{Config, FinishedOpportunity, LogLevel, TrackUpdate};
use pm_opportunity::{Opportunity, OpportunityEngine, OpportunityStatistics};
use pm_paper::{OrderRecord, PaperTradingEngine, PortfolioRecord, PositionRecord};
use pm_portfolio::RiskPolicy;
use pm_recorder::LifecycleRecord;
use pm_shadow::{ShadowEngine, ShadowTradeRecord};
use pm_strategy::{DefaultStrategy, ScanContext, ScanEvents, Strategy};
use pm_tracker::OpportunityTracker;

use crate::datasource::{
    DataSourceManager, DataStatistics, FetchOutcome, MarketSnapshot, Validator,
};
use crate::display;
use crate::health;
use crate::market;
use crate::pipeline::{ModuleStats, print_module_stats_table, print_pipeline_timeline};
use crate::stats::{FetchStats, ScannerStats};

/// scan 模式入口：连续扫描 + Shadow + Paper + Execution Simulator。
///
/// 加载 `config.toml`，初始化各 engine 与 CSV，进入扫描循环。
/// Simulation Only -- 不连接钱包 / 不真实交易。
pub async fn run_scan(cfg: &Config) -> Result<()> {
    println!("模式：扫描");
    println!();
    println!("启动中...");

    // V1.02：数据源统一经 DataSourceManager（按 config.datasource.provider 选择），
    // Scanner 不再自建 reqwest::Client / 不直接访问 HTTP。
    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    // 启动连通性检查：Config / CSV / API / JSON。任一失败则不继续扫描。
    startup_check(&manager, cfg).await?;

    // 读取历史影子交易，注入引擎并展示历史规模。
    let history = pm_shadow::load_history(&cfg.paths.shadow_csv);
    println!();
    println!("历史影子交易");
    println!();
    println!("{}", history.stats.total);
    let mut shadow = ShadowEngine::new();
    shadow.load_history(history.stats, history.next_id_base);

    // Paper Trading 引擎：组合初始资金来自配置，order_id 从历史 CSV 行数续编号。
    let policy = RiskPolicy {
        max_positions: cfg.portfolio.max_positions,
        max_position_size: cfg.portfolio.max_position_size,
        max_open_orders: cfg.execution.max_pending_orders,
        max_daily_loss: cfg.risk.max_daily_loss,
    };
    println!();
    println!("纸面交易");
    println!();
    println!("初始资金");
    println!();
    println!(
        "{} USDC",
        pm_utils::fmt_money(cfg.portfolio.initial_capital)
    );
    println!();
    println!("最大持仓数");
    println!();
    println!("{}", cfg.portfolio.max_positions);
    println!();
    println!("单笔持仓上限");
    println!();
    println!(
        "{} USDC",
        pm_utils::fmt_money(cfg.portfolio.max_position_size)
    );
    println!();
    println!("仅模拟 -- 无钱包 / 无下单 / 无签名");
    let mut paper = PaperTradingEngine::new(cfg.portfolio.initial_capital, policy);
    paper.load_order_base(pm_paper::records::load_order_base(
        &cfg.paths.paper_orders_csv,
    ));

    // Execution Simulator 引擎：与 Paper 同初始资金，order_id 从历史 CSV 行数续编号。
    let exec_params = ExecParams {
        capital: cfg.execution.capital,
        max_pending_orders: cfg.execution.max_pending_orders,
        order_notional: cfg.execution.order_notional,
        max_wait_scans: cfg.execution.max_wait_scans,
        max_fill_delay: cfg.execution.max_fill_delay,
    };
    println!();
    println!("执行模拟器");
    println!();
    println!("初始资金");
    println!();
    println!("{} USDC", pm_utils::fmt_money(cfg.execution.capital));
    println!();
    println!("最大待处理订单数");
    println!();
    println!("{}", cfg.execution.max_pending_orders);
    println!();
    println!("仅模拟 -- 无钱包 / 无下单 / 无签名");
    let mut exec = ExecutionEngine::new(exec_params);
    exec.load_order_base(pm_execution::load_order_base(&cfg.paths.execution_csv));

    let mut tracker = OpportunityTracker::new();
    let mut strategy = DefaultStrategy::new();
    let mut metrics = Metrics::new();
    let mut scanner_stats = ScannerStats::new();

    // V1.04：机会引擎（Market → Opportunity）
    let mut opp_engine = OpportunityEngine::from_pm_config(cfg);
    let mut opp_stats = OpportunityStatistics::new();

    // B3: 初始化检测机会 CSV（检测时即落盘，与纸面订单对账用）
    if let Err(e) = pm_opportunity::ensure_opportunity_csv(&cfg.paths.detected_opportunities_csv) {
        eprintln!("[警告] 检测机会 CSV 初始化失败: {}", e);
    }

    println!();
    println!("机会引擎 V1.04");
    println!();
    println!("已就绪 -- 评分 / 分类 / 过滤 / 排序");

    let scan_interval = Duration::from_secs(cfg.scanner.scan_interval_secs.max(1)); // B4: 1s 下限防无界空转
    let level = cfg.effective_log_level();
    println!();
    println!("{}", display::SEP);
    println!();
    println!("日志级别: {}（ERROR/WARN/INFO/DEBUG/TRACE）", level.as_zh());
    println!();

    loop {
        // 清屏：每轮重新绘制
        print!("{}", display::CLEAR_SCREEN);

        // 拉取并分析市场、更新跟踪器与影子/Paper/Execution 交易、输出本轮结果；期间可被 Ctrl+C 中断
        let scan_result = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("扫描器已停止");
                return Ok(());
            }
            r = scan_once(
                &mut manager,
                cfg,
                &mut tracker,
                &mut shadow,
                &mut paper,
                &mut exec,
                &mut strategy,
                &mut metrics,
                &mut scanner_stats,
                &mut opp_engine,
                &mut opp_stats, // V1.04: 累计统计（保留供后续扩展）
            ) => r,
        };

        // 单轮失败不退出，打印错误后继续下一轮
        if let Err(e) = scan_result {
            println!();
            println!("扫描失败: {:#}", e);
            tracing::warn!(error = %e, "scan round failed");
        }

        // B4: 间隔为 0 时不下发"无界空转"，而是按 1s 下限等待
        // （scan_interval 已在构造时 .max(1)）
        if cfg.scanner.scan_interval_secs == 0 {
            eprintln!(
                "[警告] scan_interval_secs = 0 已被下限保护为 1s（避免无界空转导致 CSV 疯长），\
                 请在 config.toml 中显式设置合理值。"
            );
        }
        println!();
        println!("{}", display::SEP);
        println!();
        println!("⏳ 等待 {} 秒后开始下一轮扫描……", scan_interval.as_secs());
        println!();

        // 等待后再次扫描；期间可被 Ctrl+C 中断
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("扫描器已停止");
                return Ok(());
            }
            _ = tokio::time::sleep(scan_interval) => {}
        }
    }
}

/// 启动健康检查：Config / CSV / Storage / Clock / Memory / API / JSON（V1.01 第十节）。
///
/// 委托 [`crate::health::run_health_check`]（V1.02：经 Provider 探测，不直接 HTTP）。
/// 任一 Fail 即打印报告并返回 Err，**不进入扫描循环**。
async fn startup_check(manager: &DataSourceManager, cfg: &Config) -> Result<()> {
    let report = health::run_health_check(manager.provider(), cfg).await;
    health::print_health_report(&report);
    if !report.all_pass() {
        anyhow::bail!("启动检查失败 -- 见上方报告");
    }
    Ok(())
}

/// 执行一轮扫描：拉取市场 -> 找机会 -> 更新 Tracker -> 调用 Strategy 开/平/mark ->
/// 打印明细与统计 -> 写 CSV -> 更新 Metrics。
#[allow(clippy::too_many_arguments)]
async fn scan_once(
    manager: &mut DataSourceManager,
    cfg: &Config,
    tracker: &mut OpportunityTracker,
    shadow: &mut ShadowEngine,
    paper: &mut PaperTradingEngine,
    exec: &mut ExecutionEngine,
    strategy: &mut dyn Strategy,
    metrics: &mut Metrics,
    scanner_stats: &mut ScannerStats,
    opp_engine: &mut OpportunityEngine,
    _opp_stats: &mut OpportunityStatistics,
) -> Result<()> {
    // 本轮统一时间戳：明细展示与状态写入共用，保证一轮内时间一致
    let now_dt = Local::now();
    let now = now_dt.format("%Y-%m-%d %H:%M:%S").to_string();

    let level = cfg.effective_log_level();
    let debug = level >= LogLevel::Debug;
    let threshold = cfg.scanner.opportunity_threshold;

    if level >= LogLevel::Info {
        display::print_scan_header(&now);
    }

    // ---------------- 数据拉取（经 DataSourceManager，带完整 HTTP 可观测性）----------------
    let FetchOutcome {
        markets,
        stats: fetch_stats,
        cached,
    } = manager.fetch_markets().await?;

    // ---------------- 分析（计时）----------------
    // find_opportunities 始终用于实际机会（快速路径，无明细开销）；
    // debug=true 时额外跑 analyze_markets 收集细分统计与拒绝明细。
    let norm_start = Instant::now();
    let snaps = market::find_opportunities(&markets, threshold);
    let analysis = if debug {
        Some(market::analyze_markets(&markets, threshold))
    } else {
        None
    };
    let norm_ms = norm_start.elapsed().as_millis();

    // ---------------- 累计统计更新（先更新再打印，避免本轮未计入）----------------
    scanner_stats.record_round();
    scanner_stats.add_fetch(&fetch_stats);
    if let Some(a) = &analysis {
        scanner_stats.add_round(a);
    } else {
        // debug=false：仅更新基础计数（累计统计不打印，但保持正确）
        scanner_stats.market_count += markets.len() as u64;
        scanner_stats.parsed_count += markets.len() as u64;
        scanner_stats.opportunity_count += snaps.len() as u64;
    }

    // ---------------- Scanner Debug 区块（DEBUG+）----------------
    if debug {
        if let Some(a) = &analysis {
            display::print_scanner_debug(&fetch_stats, a);
            display::print_scanner_stats_cumulative(scanner_stats);
            // V1.01 第七节：第一个市场完整字段 + 空字段标注
            crate::diagnostics::print_json_diagnostics(&markets);
        }
    }

    // ---------------- V1.02 数据层：校验 + 快照 + 统计 ----------------
    // 数据校验（第七节）：统计非法市场，tracing 打印明细。
    let validator_report = Validator::validate_many(&markets);
    // 市场快照（第九节）：B8 — 每 10 轮保存一次（原每轮无条件写导致无界增长）。
    const MARKET_SNAPSHOT_INTERVAL: u64 = 10;
    let snap_path = format!("{}/market_snapshots.csv", cfg.paths.data_dir);
    if scanner_stats.round_count % MARKET_SNAPSHOT_INTERVAL == 0 {
        let snapshot = MarketSnapshot::from_markets(&markets, manager.name(), now_dt);
        if let Err(e) = snapshot.save_to_csv(&snap_path) {
            tracing::warn!(error = %e, "市场快照保存失败");
        }
    }
    // 市场数据统计（第十节）：缓存命中则全部计为 Cached，否则 0。
    let cached_count = if cached { markets.len() } else { 0 };
    let data_stats = DataStatistics::build(
        &markets,
        manager.name(),
        validator_report.invalid,
        cached_count,
        now_dt,
    );
    if level >= LogLevel::Info {
        data_stats.print_block();
    }

    // ---------------- Markets Received == 0：警告并跳过后续（WARN+）----------------
    // 不继续执行 Strategy / Paper Trading / Execution，因为后续统计没有意义。
    if markets.is_empty() {
        if level >= LogLevel::Warn {
            println!("{}", display::SEP);
            println!();
            println!("🟡 警告：未接收到市场");
            println!();
            println!("扫描器本轮无法继续。");
            println!();
            println!("跳过策略 / 纸面交易 / 执行 -- 无有效数据。");
            println!();
        }
        return Ok(());
    }

    // ---------------- V1.04 机会引擎（Market → Opportunity）----------------
    // 尝试获取订单簿（Provider 支持时），然后经引擎评分/分类/过滤。
    let opp_engine_start = Instant::now();
    let mut orderbooks_map: std::collections::HashMap<String, pm_models::OrderBook> =
        std::collections::HashMap::new();
    if manager.capability().supports_orderbook {
        let active_ids: Vec<String> = markets.iter().map(|m| m.market_id.clone()).collect();
        if !active_ids.is_empty() {
            match manager.provider().fetch_orderbooks(&active_ids).await {
                Ok(obs) => {
                    for ob in obs {
                        if ob.best_bid.is_some() || ob.best_ask.is_some() {
                            orderbooks_map.insert(ob.market_id.clone(), ob);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, "订单簿获取失败，引擎将无订单簿运行");
                }
            }
        }
    }
    let engine_output = opp_engine.analyze(
        &markets,
        &orderbooks_map,
        &manager.provider().capability(),
        chrono::Utc::now(),
    );
    let opp_engine_ms = opp_engine_start.elapsed().as_millis();

    // 从引擎输出构建 OppSnapshot 列表（兼容现有 Tracker）
    let enriched_snaps: Vec<pm_models::OppSnapshot> = engine_output
        .opportunities
        .iter()
        .map(|opp| pm_models::OppSnapshot {
            question: opp.question.clone(),
            yes_price: opp.yes_price,
            no_price: opp.no_price,
            sum: opp.sum,
            volume: opp.volume,
            liquidity: opp.liquidity,
        })
        .collect();

    // 打印引擎输出（INFO+）
    if level >= LogLevel::Info {
        display::print_opportunity_engine_block(
            &engine_output,
            opp_engine_ms,
            orderbooks_map.len(),
        );
    }

    // ---------------- 跟踪器 + 策略（计时）----------------
    let strat_start = Instant::now();
    // 本轮发现的机会 Key（用于 reap 判定"消失"）
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut new_events: Vec<TrackUpdate> = Vec::new();
    let mut updated_events: Vec<TrackUpdate> = Vec::new();
    let mut events = ScanEvents::default();

    // V1.04：同时迭代 OppSnapshot（给 Tracker）和 Opportunity（给 Strategy）
    // B3: 收集本轮新检测机会
    let mut detected_opps: Vec<Opportunity> = Vec::new();
    for (snap, opp) in enriched_snaps
        .iter()
        .zip(engine_output.opportunities.iter())
    {
        seen_keys.insert(snap.question.clone());
        let ev = tracker.observe(snap, now_dt);
        let is_new = ev.is_new;
        // B3: 新检测机会立即持久化（与纸面订单落盘时点对齐）
        if is_new {
            detected_opps.push(opp.clone());
        }
        {
            let mut ctx = ScanContext {
                now: now_dt,
                shadow: &mut *shadow,
                paper: &mut *paper,
                execution: &mut *exec,
                events: &mut events,
            };
            strategy.on_opportunity(opp, is_new, &mut ctx);
        }
        if is_new {
            new_events.push(ev);
        } else {
            updated_events.push(ev);
        }
    }

    // 如果引擎无输出，回退到原 market::find_opportunities 路径
    if enriched_snaps.is_empty() {
        let fallback_snaps = market::find_opportunities(&markets, threshold);
        for snap in &fallback_snaps {
            if seen_keys.contains(&snap.question) {
                continue; // 引擎已处理过
            }
            seen_keys.insert(snap.question.clone());
            let ev = tracker.observe(snap, now_dt);
            let is_new = ev.is_new;
            // 回退路径：用 OppSnapshot 构建简化 Opportunity
            let fallback_opp = opp_from_snap(snap);
            {
                let mut ctx = ScanContext {
                    now: now_dt,
                    shadow: &mut *shadow,
                    paper: &mut *paper,
                    execution: &mut *exec,
                    events: &mut events,
                };
                strategy.on_opportunity(&fallback_opp, is_new, &mut ctx);
            }
            if is_new {
                // B3: 回退路径的检测机会也持久化
                detected_opps.push(fallback_opp.clone());
                new_events.push(ev);
            } else {
                updated_events.push(ev);
            }
        }
    }

    // 清理本轮未再出现的机会 -> 生命周期结束 -> 平仓对应交易（B6: 超时强制到期）
    let max_age = cfg.scanner.max_opportunity_age_secs;
    let finished = tracker.reap(&seen_keys, now_dt, max_age);
    for f in &finished {
        let mut ctx = ScanContext {
            now: now_dt,
            shadow: &mut *shadow,
            paper: &mut *paper,
            execution: &mut *exec,
            events: &mut events,
        };
        strategy.on_close(f, &mut ctx);
    }

    // 本轮所有开/平/mark 完成，推进 exec tick + 重估 paper
    {
        let mut ctx = ScanContext {
            now: now_dt,
            shadow: &mut *shadow,
            paper: &mut *paper,
            execution: &mut *exec,
            events: &mut events,
        };
        strategy.on_scan(&mut ctx);
    }
    let strat_ms = strat_start.elapsed().as_millis();

    // ---------------- 渲染明细（INFO+）----------------
    if level >= LogLevel::Info {
        display::print_new_opportunities(&new_events);
        display::print_shadow_opened(&events.shadow_opened);
        display::print_paper_opens(&events.paper_opens);
        display::print_paper_rejections(&events.paper_rejections);
        display::print_updated_opportunities(&updated_events);
    }

    // ---------------- CSV 写入（计时；写入始终执行，提示 INFO+）----------------
    let storage_start = Instant::now();

    // B3: 写入本轮新检测机会到 detected_opportunities.csv
    if !detected_opps.is_empty() {
        pm_opportunity::append_opportunities(&cfg.paths.detected_opportunities_csv, &detected_opps);
    }

    if !finished.is_empty() {
        if level >= LogLevel::Info {
            display::print_finished(&finished);
        }
        // 写入 opportunities.csv
        let records: Vec<LifecycleRecord> = finished.iter().map(LifecycleRecord::from).collect();
        let _written = pm_recorder::append_records(&records, &cfg.paths.opportunities_csv);
        if level >= LogLevel::Info {
            println!("已保存至 CSV");
            println!();
        }
    }

    if !events.shadow_closed.is_empty() {
        if level >= LogLevel::Info {
            display::print_shadow_closed(&events.shadow_closed);
        }
        let records: Vec<ShadowTradeRecord> = events
            .shadow_closed
            .iter()
            .map(ShadowTradeRecord::from)
            .collect();
        let _written = pm_shadow::append_records(&records, &cfg.paths.shadow_csv);
        if level >= LogLevel::Info {
            println!("影子交易已保存至 CSV");
            println!();
        }
    }

    if !events.paper_closes.is_empty() {
        if level >= LogLevel::Info {
            display::print_paper_closes(&events.paper_closes);
        }
        let order_recs: Vec<OrderRecord> = events
            .paper_closes
            .iter()
            .map(|c| OrderRecord::from(&c.order))
            .collect();
        let pos_recs: Vec<PositionRecord> = events
            .paper_closes
            .iter()
            .map(|c| PositionRecord::from_closed(&c.position))
            .collect();
        let _o = pm_paper::append_orders(&order_recs, &cfg.paths.paper_orders_csv);
        let _p = pm_paper::append_positions(&pos_recs, &cfg.paths.paper_positions_csv);
        if level >= LogLevel::Info {
            println!("纸面交易已保存至 CSV");
            println!();
        }
    }

    // 写入本轮新建的 BUY 订单
    if !events.paper_opens.is_empty() {
        let order_recs: Vec<OrderRecord> =
            events.paper_opens.iter().map(OrderRecord::from).collect();
        let _o = pm_paper::append_orders(&order_recs, &cfg.paths.paper_orders_csv);
    }

    // Paper 仪表盘（INFO+）
    if level >= LogLevel::Info {
        display::print_paper_dashboard(paper);
    }

    // 写入组合快照到 paper_portfolio.csv（V1.09：仅组合变化时写入，避免每轮重复）
    if paper.portfolio().has_changed() {
        let pf_rec = PortfolioRecord::from_portfolio(paper.portfolio(), now_dt);
        let _pf = pm_paper::append_portfolio(&[pf_rec], &cfg.paths.paper_portfolio_csv);
        paper.portfolio_mut().mark_saved();
    }

    // Execution 仪表盘（INFO+）
    if level >= LogLevel::Info {
        display::print_exec_events(&events.exec_events);
        display::print_exec_dashboard(&exec.portfolio_summary());
        display::print_exec_stats(exec.stats());
    }

    // 写入本轮终态订单到 execution_orders.csv
    let drained = exec.drain_terminal();
    if !drained.is_empty() {
        let recs: Vec<pm_execution::ExecutionOrderRecord> = drained
            .iter()
            .map(pm_execution::ExecutionOrderRecord::from)
            .collect();
        let _ = pm_execution::append_orders(&recs, &cfg.paths.execution_csv);
        if level >= LogLevel::Info {
            println!("执行订单已保存至 CSV");
            println!();
        }
    }
    let storage_ms = storage_start.elapsed().as_millis();

    // ---------------- 统计区块（INFO+）----------------
    if level >= LogLevel::Info {
        let csv_records = pm_storage::count_rows(&cfg.paths.opportunities_csv);
        display::print_opportunity_stats(
            tracker.active_count(),
            finished.len(),
            new_events.len(),
            updated_events.len(),
            csv_records,
        );
        display::print_shadow_stats(shadow.stats());
    }

    // ---------------- 更新 Metrics（计时）----------------
    let metrics_start = Instant::now();
    update_metrics(
        metrics,
        &events,
        new_events.len(),
        updated_events.len(),
        finished.len(),
    );
    let metrics_ms = metrics_start.elapsed().as_millis();

    // ---------------- 模块时间线 + 流水线 + 系统汇总（INFO+）----------------
    if level >= LogLevel::Info {
        let modules = build_round_module_stats(
            &fetch_stats,
            norm_ms,
            strat_ms,
            storage_ms,
            metrics_ms,
            &events,
            &snaps,
            markets.len(),
        );
        let round_total_ms = ModuleStats::total_duration_ms(&modules);
        print_module_stats_table(&modules);
        print_pipeline_timeline(&modules);
        display::print_round_system_summary(
            &fetch_stats,
            &analysis,
            &events,
            markets.len(),
            round_total_ms,
        );
    }

    // ---------------- TRACE：全量市场转储（前 50，防爆炸）----------------
    if level == LogLevel::Trace {
        display::print_market_trace_dump(&markets);
    }

    Ok(())
}

/// 构造本轮扫描各模块的 ModuleStats（V1.01 第二节）。
///
/// Shadow/Paper/Execution 封装在 Strategy hook 内不可拆分，其行报 input/output 事件计数、
/// 耗时并入"策略调度"行。计数才是诊断核心（opportunity=0 -> 三者 input/output=0）。
#[allow(clippy::too_many_arguments)]
fn build_round_module_stats(
    fetch: &FetchStats,
    norm_ms: u128,
    strat_ms: u128,
    storage_ms: u128,
    metrics_ms: u128,
    events: &ScanEvents,
    snaps: &[pm_models::OppSnapshot],
    markets_len: usize,
) -> Vec<ModuleStats> {
    let exec_new_orders = events
        .exec_events
        .iter()
        .filter(|e| matches!(e, ExecEvent::NewOrder { .. }))
        .count() as u64;
    let events_total = events.exec_events.len() as u64;
    let snaps_n = snaps.len() as u64;

    vec![
        ModuleStats {
            name: "HTTP 请求".into(),
            duration_ms: fetch.total_ms,
            input_count: 0,
            output_count: markets_len as u64,
            success: fetch.failed_count == 0,
            error_count: fetch.failed_count,
            warning_count: 0,
        },
        ModuleStats {
            name: "JSON 解析".into(),
            duration_ms: fetch.deserialize_ms,
            input_count: fetch.total_bytes,
            output_count: markets_len as u64,
            success: fetch.failed_count == 0,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "市场归一化".into(),
            duration_ms: norm_ms,
            input_count: markets_len as u64,
            output_count: snaps_n,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "跟踪器".into(),
            duration_ms: 0,
            input_count: snaps_n,
            output_count: snaps_n,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "策略调度".into(),
            duration_ms: strat_ms,
            input_count: snaps_n,
            output_count: (events.shadow_opened.len() + events.paper_opens.len()) as u64
                + exec_new_orders,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "影子交易".into(),
            duration_ms: 0,
            input_count: snaps_n,
            output_count: events.shadow_opened.len() as u64,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "纸面交易".into(),
            duration_ms: 0,
            input_count: snaps_n,
            output_count: events.paper_opens.len() as u64,
            success: true,
            error_count: 0,
            warning_count: events.paper_rejections.len() as u64,
        },
        ModuleStats {
            name: "执行模拟".into(),
            duration_ms: 0,
            input_count: snaps_n,
            output_count: exec_new_orders,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "指标".into(),
            duration_ms: metrics_ms,
            input_count: events_total,
            output_count: 1,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
        ModuleStats {
            name: "存储".into(),
            duration_ms: storage_ms,
            input_count: 0,
            output_count: 0,
            success: true,
            error_count: 0,
            warning_count: 0,
        },
    ]
}

/// V1.04 辅助：从 OppSnapshot 构建简化 Opportunity（回退路径用）。
fn opp_from_snap(snap: &pm_models::OppSnapshot) -> Opportunity {
    use pm_opportunity::OpportunityType;
    Opportunity::new(
        snap.question.clone(),
        snap.question.clone(),
        "scanner".into(),
        chrono::Utc::now(),
        OpportunityType::Unknown,
        50.0,
        0.5,
        50,
        25.0,
        20.0,
        0.0,
        10.0,
        5.0,
        10.0,
        0.01,
        1.0,
        snap.yes_price,
        snap.no_price,
        snap.sum,
        None,
        snap.volume,
        snap.liquidity,
        None,
        None,
    )
}

/// 从本轮事件累加 Metrics 计数（exec 事件按类型分类）。
fn update_metrics(
    metrics: &mut Metrics,
    events: &ScanEvents,
    new: usize,
    updated: usize,
    finished: usize,
) {
    metrics.record_round();
    metrics.add_opportunities(new as u64, updated as u64, finished as u64);
    metrics.add_shadow(
        events.shadow_opened.len() as u64,
        events.shadow_closed.len() as u64,
    );
    metrics.add_paper(
        events.paper_opens.len() as u64,
        events.paper_closes.len() as u64,
        events.paper_rejections.len() as u64,
    );

    let mut submitted = 0u64;
    let mut filled = 0u64;
    let mut cancelled = 0u64;
    let mut expired = 0u64;
    let mut rejected = 0u64;
    for e in &events.exec_events {
        match e {
            ExecEvent::NewOrder { .. } => submitted += 1,
            ExecEvent::Filled { .. } => filled += 1,
            ExecEvent::Cancelled { .. } => cancelled += 1,
            ExecEvent::Expired { .. } => expired += 1,
            ExecEvent::Rejected { .. } => rejected += 1,
            _ => {}
        }
    }
    metrics.add_exec(submitted, filled, cancelled, expired, rejected);
    metrics.add_portfolio_snapshot();
}

// 静默引用 FinishedOpportunity，保留类型在文档脉络中（driver 内部经 strategy 使用）。
#[allow(dead_code)]
fn _finished_used(_: FinishedOpportunity) {}
