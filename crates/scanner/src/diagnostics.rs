//! pm-scanner::diagnostics：诊断模式（V1.01 第十二节）+ 诊断报告渲染。
//!
//! **只增强可观测性，不改变任何交易/策略逻辑。**
//!
//! [`run_diagnose`]：执行**一次**扫描，输出完整诊断报告，**不进入循环、不清屏**。
//! 报告覆盖第 1-11 节全部要素：启动检查 / HTTP / JSON / 市场统计 / 过滤统计 / 策略分析 /
//! 市场快照 / 模块时间线 / 流水线时间线 / 系统汇总 / 第十四节 12 问显式作答。
//!
//! 诊断模式忽略 `log_level`，全量输出；不写 CSV（非持久化诊断）。

use std::collections::HashSet;
use std::time::Instant;

use anyhow::Result;
use chrono::Local;

use pm_execution::{ExecEvent, ExecParams, ExecutionEngine};
use pm_models::{Config, UnifiedMarket};
use pm_paper::PaperTradingEngine;
use pm_portfolio::RiskPolicy;
use pm_shadow::ShadowEngine;
use pm_strategy::{DefaultStrategy, ScanContext, ScanEvents, Strategy};
use pm_tracker::OpportunityTracker;

use crate::datasource::DataSourceManager;
use crate::display::{DASH, SEP};
use crate::health;
use crate::market;
use crate::pipeline::{print_module_stats_table, print_pipeline_timeline, ModuleStats};
use crate::stats::{MarketSample, RejectionReason, RoundAnalysis};

/// 诊断模式入口：单次扫描 + 完整诊断报告（V1.01 第十二节）。
///
/// 流程：启动检查 -> 拉取市场 -> 分析 -> 引擎通道（开/平/tick）-> 模块时间线 ->
/// 流水线时间线 -> 系统汇总 -> 12 问诊断。不进入循环、不清屏、不写 CSV。
pub async fn run_diagnose(cfg: &Config) -> Result<()> {
    println!("{}", SEP);
    println!();
    println!("🔍 诊断报告");
    println!();
    println!("仅模拟 -- 单次扫描，输出完整诊断，不进入循环");
    println!();
    println!("{}", SEP);
    println!();

    let mut manager = DataSourceManager::from_config(cfg)?;
    manager.print_capability_block();

    // ---- 启动检查（经 Provider 探测，不直接 HTTP）----
    let report = health::run_health_check(manager.provider(), cfg).await;
    health::print_health_report(&report);
    // 诊断模式：即使启动检查有失败也继续，把失败本身作为诊断结论展示。

    let threshold = cfg.scanner.opportunity_threshold;

    // ---- 拉取市场（经 DataSourceManager）----
    let fetch = manager.fetch_markets().await;
    let (markets, fetch_stats) = match fetch {
        Ok(r) => (r.markets, r.stats),
        Err(e) => {
            // 拉取失败也是诊断结论
            println!("{}", DASH);
            println!();
            println!("🔴 数据拉取失败: {:#}", e);
            println!();
            println!("无法继续数据流诊断（数据源不可达）。请先修复网络 / 代理。");
            println!();
            print_diagnostic_answers_offline();
            return Ok(());
        }
    };

    // ---- 分析 ----
    let norm_start = Instant::now();
    let analysis = market::analyze_markets(&markets, threshold);
    let norm_ms = norm_start.elapsed().as_millis();
    let snaps = analysis.opportunities.clone();

    // ---- 各模块时间线 ----
    let mut modules: Vec<ModuleStats> = Vec::new();

    // HTTP / Deserialize 来自 fetch_stats
    modules.push(ModuleStats {
        name: "HTTP 请求".into(),
        duration_ms: fetch_stats.total_ms,
        input_count: 0,
        output_count: markets.len() as u64,
        success: fetch_stats.failed_count == 0,
        error_count: fetch_stats.failed_count,
        warning_count: 0,
    });
    modules.push(ModuleStats {
        name: "JSON 解析".into(),
        duration_ms: fetch_stats.deserialize_ms,
        input_count: fetch_stats.total_bytes,
        output_count: markets.len() as u64,
        success: fetch_stats.failed_count == 0,
        error_count: 0,
        warning_count: 0,
    });
    modules.push(ModuleStats {
        name: "市场归一化".into(),
        duration_ms: norm_ms,
        input_count: markets.len() as u64,
        output_count: analysis.opportunity_count() as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });

    // ---- 引擎通道：跟踪器 + 策略（开/平/tick），手动子计时 ----
    let engine_stats = run_engine_pass(cfg, &snaps);
    modules.extend(engine_stats.module_stats.clone());

    // ---- 报告输出 ----
    print_http_diagnostics(&fetch_stats);
    print_json_diagnostics(&markets);
    print_market_stats(&analysis);
    print_filter_funnel(&analysis);
    print_strategy_explain(&analysis);
    print_market_snapshots(&analysis.samples);

    // 生命周期 / 影子 / 纸面 / 执行 模块计数（多数为 0，即诊断点）
    print_engine_lifecycle(&engine_stats);

    print_module_stats_table(&modules);
    print_pipeline_timeline(&modules);

    let total_ms = ModuleStats::total_duration_ms(&modules);
    print_system_summary(&fetch_stats, &analysis, &engine_stats, total_ms);
    print_diagnostic_answers(&fetch_stats, &analysis, &engine_stats, threshold);

    println!("{}", SEP);
    println!();
    println!("诊断完成 -- 仅模拟");
    println!();
    Ok(())
}

/// 引擎通道执行结果（诊断用）。
struct EnginePassResult {
    /// 跟踪器 / 策略 / 影子 / 纸面 / 执行 / 指标 / 存储 的模块统计。
    module_stats: Vec<ModuleStats>,
    /// 影子开仓数。
    shadow_opened: usize,
    /// 纸面开仓数。
    paper_opens: usize,
    /// 纸面拒绝数。
    paper_rejections: usize,
    /// 执行新订单数。
    exec_new_orders: usize,
    /// 执行成交数。
    exec_filled: usize,
}

/// 跑一遍引擎通道：跟踪器 observe -> 策略 on_opportunity(开) -> reap 全部 -> on_close(平) -> on_scan(tick)。
///
/// 调用策略与各 engine 的公开 API，**不修改任何交易逻辑**。手动子计时产出各模块 ModuleStats。
fn run_engine_pass(cfg: &Config, snaps: &[pm_models::OppSnapshot]) -> EnginePassResult {
    let now = Local::now();

    let policy = RiskPolicy {
        max_positions: cfg.portfolio.max_positions,
        max_position_size: cfg.portfolio.max_position_size,
        max_open_orders: cfg.execution.max_pending_orders,
        max_daily_loss: cfg.risk.max_daily_loss,
    };
    let mut shadow = ShadowEngine::new();
    let mut paper = PaperTradingEngine::new(cfg.portfolio.initial_capital, policy);
    let exec_params = ExecParams {
        capital: cfg.execution.capital,
        max_pending_orders: cfg.execution.max_pending_orders,
        order_notional: cfg.execution.order_notional,
        max_wait_scans: cfg.execution.max_wait_scans,
        max_fill_delay: cfg.execution.max_fill_delay,
    };
    let mut exec = ExecutionEngine::new(exec_params);
    let mut tracker = OpportunityTracker::new();
    let mut strategy = DefaultStrategy::new();
    let mut events = ScanEvents::default();

    let mut tracker_ms = 0u128;
    let mut strategy_ms = 0u128;

    // 开仓阶段
    for snap in snaps {
        let t0 = Instant::now();
        let _ = tracker.observe(snap, now);
        tracker_ms += t0.elapsed().as_millis();

        let t1 = Instant::now();
        let mut ctx = ScanContext {
            now,
            shadow: &mut shadow,
            paper: &mut paper,
            execution: &mut exec,
            events: &mut events,
        };
        let opp = opp_from_snap_for_diag(snap);
        strategy.on_opportunity(&opp, true, &mut ctx);
        strategy_ms += t1.elapsed().as_millis();
    }

    // 平仓阶段：reap 全部（seen 为空 -> 全部生命周期结束）
    let seen: HashSet<String> = HashSet::new();
    let t2 = Instant::now();
    let finished = tracker.reap(&seen, now);
    tracker_ms += t2.elapsed().as_millis();
    for f in &finished {
        let t3 = Instant::now();
        let mut ctx = ScanContext {
            now,
            shadow: &mut shadow,
            paper: &mut paper,
            execution: &mut exec,
            events: &mut events,
        };
        strategy.on_close(f, &mut ctx);
        strategy_ms += t3.elapsed().as_millis();
    }

    // on_scan：纸面重估 + 执行 tick
    let t4 = Instant::now();
    let mut ctx = ScanContext {
        now,
        shadow: &mut shadow,
        paper: &mut paper,
        execution: &mut exec,
        events: &mut events,
    };
    strategy.on_scan(&mut ctx);
    let reconcile_ms = t4.elapsed().as_millis();

    // 统计事件
    let shadow_opened = events.shadow_opened.len();
    let shadow_closed = events.shadow_closed.len();
    let paper_opens = events.paper_opens.len();
    let paper_rejections = events.paper_rejections.len();
    let paper_closes = events.paper_closes.len();
    let exec_new_orders = events
        .exec_events
        .iter()
        .filter(|e| matches!(e, ExecEvent::NewOrder { .. }))
        .count();
    let exec_filled = events
        .exec_events
        .iter()
        .filter(|e| matches!(e, ExecEvent::Filled { .. }))
        .count();
    let exec_events_total = events.exec_events.len();

    let mut module_stats: Vec<ModuleStats> = Vec::new();
    module_stats.push(ModuleStats {
        name: "跟踪器".into(),
        duration_ms: tracker_ms,
        input_count: snaps.len() as u64,
        output_count: snaps.len() as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    // 策略调度耗时 = on_opportunity + on_close；on_scan 单列为"纸面/执行推进"
    module_stats.push(ModuleStats {
        name: "策略调度".into(),
        duration_ms: strategy_ms,
        input_count: snaps.len() as u64,
        output_count: (shadow_opened + paper_opens + exec_new_orders) as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    // 影子 / 纸面 / 执行：封装在策略 hook 内，耗时并入"策略调度"+"纸面/执行推进"，行内报 0ms + 计数
    module_stats.push(ModuleStats {
        name: "影子交易".into(),
        duration_ms: 0,
        input_count: snaps.len() as u64,
        output_count: shadow_opened as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    module_stats.push(ModuleStats {
        name: "纸面交易".into(),
        duration_ms: 0,
        input_count: snaps.len() as u64,
        output_count: paper_opens as u64,
        success: true,
        error_count: 0,
        warning_count: paper_rejections as u64,
    });
    module_stats.push(ModuleStats {
        name: "执行模拟".into(),
        duration_ms: 0,
        input_count: snaps.len() as u64,
        output_count: exec_new_orders as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    module_stats.push(ModuleStats {
        name: "纸面/执行推进".into(),
        duration_ms: reconcile_ms,
        input_count: 0,
        output_count: (exec_events_total - exec_new_orders) as u64,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    module_stats.push(ModuleStats {
        name: "指标".into(),
        duration_ms: 0,
        input_count: (shadow_opened + shadow_closed + paper_opens + paper_closes + exec_events_total)
            as u64,
        output_count: 1,
        success: true,
        error_count: 0,
        warning_count: 0,
    });
    module_stats.push(ModuleStats {
        name: "存储".into(),
        duration_ms: 0,
        input_count: 0,
        output_count: 0,
        success: true,
        error_count: 0,
        warning_count: 0,
    });

    EnginePassResult {
        module_stats,
        shadow_opened,
        paper_opens,
        paper_rejections,
        exec_new_orders,
        exec_filled,
    }
}

// ============================================================================
// 报告渲染函数
// ============================================================================

fn kv(label: &str, value: &str) {
    println!("{}", label);
    println!();
    println!("{}", value);
    println!();
}

/// 第六节：API / HTTP 诊断。
fn print_http_diagnostics(fetch: &crate::stats::FetchStats) {
    println!("{}", SEP);
    println!();
    println!("🌐 HTTP 请求");
    println!();
    println!("{}", DASH);
    println!();
    kv("请求地址", fetch.first_url.as_deref().unwrap_or("（无）"));
    kv("HTTP 请求数", &fetch.request_count.to_string());
    kv("HTTP 成功数", &fetch.success_count.to_string());
    kv("HTTP 失败数", &fetch.failed_count.to_string());
    kv("HTTP 末次状态", &fetch.last_status.to_string());
    kv("HTTP 总字节数", &fetch.total_bytes.to_string());
    kv("HTTP 总耗时", &format!("{} 毫秒", fetch.total_ms));
    kv("JSON 反序列化耗时", &format!("{} 毫秒", fetch.deserialize_ms));
    kv("Rate-Limit", fetch.rate_limit.as_deref().unwrap_or("无"));
    if let Some(e) = &fetch.last_error {
        kv("HTTP 末次错误", e);
    }
    if !fetch.pages.is_empty() {
        println!("逐页明细");
        println!();
        for p in &fetch.pages {
            println!(
                "  # offset={} | 状态={} | {} 字节 | {} 毫秒 | {}",
                p.url.rsplit("offset=").next().unwrap_or("?"),
                p.status,
                p.bytes,
                p.elapsed_ms,
                if p.ok { "正常" } else { "失败" }
            );
            if let Some(e) = &p.error {
                println!("    错误: {}", e);
            }
        }
        println!();
    }
}

/// 第七节：数据字段诊断 -- 第一个 UnifiedMarket 完整字段 + 空字段标注。
pub(crate) fn print_json_diagnostics(markets: &[UnifiedMarket]) {
    println!("{}", SEP);
    println!();
    println!("📝 数据字段（第一个市场完整字段）");
    println!();
    println!("{}", DASH);
    println!();
    if markets.is_empty() {
        println!("（未接收到市场，无法校验字段映射）");
        println!();
        return;
    }
    let m = &markets[0];
    let mut empties: Vec<&str> = Vec::new();

    if m.market_id.trim().is_empty() {
        empties.push("market_id（市场标识）");
    }
    kv("market_id", &m.market_id);

    let q_empty = m.question.trim().is_empty();
    if q_empty {
        empties.push("question（问题）");
    }
    kv("question", if q_empty { "（空）" } else { &m.question });

    kv("status", m.status.as_zh());

    let yes_str = m
        .yes_price
        .map(|v| v.to_string())
        .unwrap_or_else(|| "（缺失）".into());
    if m.yes_price.is_none() {
        empties.push("yes_price（YES 价）");
    }
    kv("yes_price", &yes_str);
    let no_str = m
        .no_price
        .map(|v| v.to_string())
        .unwrap_or_else(|| "（缺失）".into());
    if m.no_price.is_none() {
        empties.push("no_price（NO 价）");
    }
    kv("no_price", &no_str);

    kv("outcome_count", &m.outcome_count.to_string());

    if m.volume == 0.0 {
        empties.push("volume（成交额==0）");
    }
    kv("volume", &m.volume.to_string());
    if m.liquidity == 0.0 {
        empties.push("liquidity（流动性==0）");
    }
    kv("liquidity", &m.liquidity.to_string());

    let cat = m.category.clone().unwrap_or_default();
    if cat.is_empty() {
        empties.push("category（分类）");
    }
    kv("category", if cat.is_empty() { "（缺失）" } else { &cat });

    kv("provider", &m.provider);

    println!("{}", DASH);
    println!();
    if empties.is_empty() {
        println!("🟢 全部字段非空");
    } else {
        println!("🟡 空字段: {}", empties.join("、"));
    }
    println!();
}

/// 市场统计（数据流各阶段计数）。
fn print_market_stats(a: &RoundAnalysis) {
    println!("{}", SEP);
    println!();
    println!("📊 市场统计");
    println!();
    println!("{}", DASH);
    println!();
    kv("接收市场数", &a.received.to_string());
    kv("解析市场数", &a.parsed.to_string());
    kv("活跃市场数", &a.active.to_string());
    kv("已关闭市场数", &a.closed.to_string());
    kv("有价市场数", &a.with_prices.to_string());
    kv("缺价市场数", &a.missing_prices.to_string());
    kv("通过校验市场数", &a.passed_validation.to_string());
    kv("通过策略市场数", &a.passed_strategy.to_string());
    kv("潜在机会数", &a.opportunity_count().to_string());
}

/// 第三节：过滤统计漏斗。
fn print_filter_funnel(a: &RoundAnalysis) {
    println!("{}", SEP);
    println!();
    println!("📊 过滤统计（漏斗）");
    println!();
    println!("{}", DASH);
    println!();
    println!("接收市场: {}", a.received);
    println!("  ↓ 已关闭: {}", a.filtered_closed);
    println!("  ↓ 不活跃: {}", a.filtered_inactive);
    println!("  ↓ 缺价: {}", a.filtered_missing_price);
    println!("  ↓ 数据无效: {}", a.filtered_invalid_data);
    println!(
        "  ↓ 策略过滤（YES+NO >= 阈值）: {}",
        a.filtered_strategy
    );
    println!("  ↓ 套利机会: {}", a.opportunity_count());
    println!();
}

/// 第四节：策略分析 -- 拒绝明细（每组前 20）。
fn print_strategy_explain(a: &RoundAnalysis) {
    println!("{}", SEP);
    println!();
    println!("🔍 策略分析（拒绝明细）");
    println!();
    println!("{}", DASH);
    println!();
    if a.rejections.is_empty() {
        println!("（无拒绝 -- 所有接收市场均成为机会，或未接收到市场）");
        println!();
        return;
    }
    println!("拒绝总数: {}", a.rejections.len());
    println!();
    let groups = [
        ("缺价", RejectionReason::MissingPrice),
        ("数据无效", RejectionReason::InvalidData),
        ("不活跃", RejectionReason::Inactive),
        ("已关闭", RejectionReason::Closed),
        ("YES+NO >= 阈值", RejectionReason::SumAboveThreshold),
    ];
    const CAP: usize = 20;
    for (name, reason) in groups {
        let members: Vec<&crate::stats::MarketRejection> =
            a.rejections.iter().filter(|r| r.reason == reason).collect();
        if members.is_empty() {
            continue;
        }
        println!("{}", DASH);
        println!();
        kv(name, &members.len().to_string());
        for r in members.iter().take(CAP) {
            println!("- {} （{}）", r.question, r.reason.as_str());
        }
        if members.len() > CAP {
            println!("... 及其他 {} 个", members.len() - CAP);
        }
        println!();
    }
}

/// 第五节：市场快照（随机 3 个）。
fn print_market_snapshots(samples: &[MarketSample]) {
    println!("{}", SEP);
    println!();
    println!("📊 市场快照（随机 3 个）");
    println!();
    if samples.is_empty() {
        println!("（未接收到市场）");
        println!();
        return;
    }
    for s in samples {
        println!("{}", DASH);
        println!();
        kv("问题", &s.question);
        let (yes, no, sum) = match s.price {
            Some((y, n)) => (y.to_string(), n.to_string(), format!("{:.2}", y + n)),
            None => ("（缺失）".into(), "（缺失）".into(), "（缺失）".into()),
        };
        kv("YES", &yes);
        kv("NO", &no);
        kv("YES+NO", &sum);
        kv("成交额", &s.volume.to_string());
        kv("流动性", &s.liquidity.to_string());
        kv("已关闭", &s.closed.to_string());
        kv("结果数", &s.outcome_count.to_string());
    }
}

/// 生命周期 / 影子 / 纸面 / 执行 模块计数（多数为 0，即诊断点）。
fn print_engine_lifecycle(e: &EnginePassResult) {
    println!("{}", SEP);
    println!();
    println!("📈 生命周期跟踪 / 影子 / 纸面 / 执行（引擎通道计数）");
    println!();
    println!("{}", DASH);
    println!();
    kv("影子交易开仓", &e.shadow_opened.to_string());
    kv("纸面交易开仓", &e.paper_opens.to_string());
    kv("纸面交易拒绝", &e.paper_rejections.to_string());
    kv("执行模拟新订单", &e.exec_new_orders.to_string());
    kv("执行模拟成交", &e.exec_filled.to_string());
    println!();
    println!("（以上多数为 0 属正常：常态下无套利机会 -> 策略输入 0 -> 各引擎输出 0）");
    println!();
}

/// 第十一节：系统汇总。
fn print_system_summary(
    fetch: &crate::stats::FetchStats,
    a: &RoundAnalysis,
    e: &EnginePassResult,
    total_ms: u128,
) {
    let http_total = fetch.request_count.max(1);
    let http_success_rate = (fetch.success_count as f64) / (http_total as f64) * 100.0;
    let json_ok = fetch.failed_count == 0;
    println!("{}", SEP);
    println!();
    println!("📊 系统汇总");
    println!();
    println!("{}", DASH);
    println!();
    kv("HTTP 成功率", &format!("{:.0}%", http_success_rate));
    kv("JSON 成功率", if json_ok { "100%" } else { "0%" });
    kv("市场数", &a.received.to_string());
    kv("策略通过", &a.passed_strategy.to_string());
    kv("策略拒绝", &a.filtered_strategy.to_string());
    kv("影子交易", &e.shadow_opened.to_string());
    kv("纸面订单", &e.paper_opens.to_string());
    kv("执行订单", &e.exec_new_orders.to_string());
    kv("CSV 写入", "未写入（诊断模式不持久化）");
    kv("总耗时", &format!("{} 毫秒", total_ms));
}

/// 第十四节：12 问显式作答。
fn print_diagnostic_answers(
    fetch: &crate::stats::FetchStats,
    a: &RoundAnalysis,
    e: &EnginePassResult,
    threshold: f64,
) {
    let api_ok = fetch.failed_count == 0 && fetch.success_count > 0;
    let json_ok = fetch.failed_count == 0;
    let mark = |ok: bool| if ok { "✅ 是" } else { "❌ 否" };

    println!("{}", SEP);
    println!();
    println!("🔍 诊断报告 -- 12 问作答");
    println!();
    println!("{}", DASH);
    println!();

    // 1 API 是否成功？
    println!("1. API 是否成功？ {}", mark(api_ok));
    println!(
        "   依据: 请求数={} 成功={} 失败={} 末次状态={}",
        fetch.request_count, fetch.success_count, fetch.failed_count, fetch.last_status
    );
    println!();

    // 2 HTTP 是否正常？
    println!("2. HTTP 是否正常？ {}", mark(api_ok));
    println!(
        "   依据: 失败数={}（0 为正常）；末次状态={}（422 为分页末尾标记，视作正常）",
        fetch.failed_count, fetch.last_status
    );
    println!();

    // 3 JSON 是否正确？
    println!("3. JSON 是否正确？ {}", mark(json_ok));
    println!(
        "   依据: 反序列化耗时 {} 毫秒，解析得到 {} 个市场",
        fetch.deserialize_ms,
        a.received
    );
    println!();

    // 4 Market 有多少？
    println!("4. Market 有多少？ {}", a.received);
    println!(
        "   细分: 活跃={} 已关闭={} 有价={} 缺价={}",
        a.active, a.closed, a.with_prices, a.missing_prices
    );
    println!();

    // 5 为什么被过滤？
    println!("5. 为什么被过滤？");
    println!(
        "   已关闭={} 不活跃={} 缺价={} 数据无效={} 策略过滤={}",
        a.filtered_closed,
        a.filtered_inactive,
        a.filtered_missing_price,
        a.filtered_invalid_data,
        a.filtered_strategy
    );
    println!();

    // 6 策略为什么拒绝？
    println!("6. 策略为什么拒绝？");
    if a.filtered_strategy > 0 {
        println!(
            "   {} 个市场 YES+NO >= 阈值 {}（归一化市场 SUM≈1.0，常态全部被拒）",
            a.filtered_strategy, threshold
        );
    } else {
        println!("   无策略拒绝（filtered_strategy=0）");
    }
    println!();

    // 7 为什么没有 Opportunity？
    println!("7. 为什么没有套利机会？");
    if a.opportunity_count() == 0 {
        println!(
            "   通过校验 {} 个，但 SUM 均 >= 阈值 -> 套利机会 0。",
            a.passed_validation
        );
        println!("   根因: Gamma outcomePrices 为归一化中间价（YES+NO≡1.0），结构上无 SUM<阈值。");
        println!("   真实套利需接入 CLOB API 取最优买卖价（V1.01 不接入，保持模拟）。");
    } else {
        println!("   实际发现 {} 个套利机会。", a.opportunity_count());
    }
    println!();

    // 8 为什么没有 Paper Order？
    println!("8. 为什么没有纸面订单？ {}", e.paper_opens);
    if e.paper_opens == 0 {
        println!("   根因: 套利机会=0 -> 策略输入 0 -> 纸面交易未触发开仓。");
    }
    println!();

    // 9 为什么没有 Execution？
    println!("9. 为什么没有执行订单？ {}", e.exec_new_orders);
    if e.exec_new_orders == 0 {
        println!("   根因: 套利机会=0 -> 策略输入 0 -> 执行模拟器未提交订单。");
    }
    println!();

    // 10 为什么没有 Shadow Trade？
    println!("10. 为什么没有影子交易？ {}", e.shadow_opened);
    if e.shadow_opened == 0 {
        println!("   根因: 套利机会=0 -> 策略输入 0 -> 影子交易未开仓。");
    }
    println!();

    // 11 综合
    println!("11. 综合结论:");
    if !api_ok {
        println!("   🔴 API 层故障 -- 优先修复网络 / 代理 / Gamma 可达性。");
    } else if !json_ok {
        println!("   🔴 JSON 解析故障 -- 检查 Serde 字段映射（见 JSON 诊断块）。");
    } else if a.received == 0 {
        println!("   🟡 API 正常但未返回市场 -- 检查 closed=false 过滤与分页。");
    } else if a.opportunity_count() == 0 {
        println!(
            "   🟢 数据链路正常：API✓ JSON✓ 市场={}✓。无套利机会是**预期行为**（归一化价格），非故障。",
            a.received
        );
    } else {
        println!("   🟢 数据链路正常且发现套利机会，引擎通道应已激活。");
    }
    println!();

    // 12 下一步
    println!("12. 下一步建议:");
    if !api_ok {
        println!("   - 检查 HTTPS_PROXY 代理（127.0.0.1:7890）与 Gamma API 可达性");
    } else if a.opportunity_count() == 0 {
        println!("   - 接入 CLOB API 取真实买卖价以发现真实套利（超出 V1.01 范围）");
        println!("   - 或调高 opportunity_threshold 观察机会（当前 {}）", threshold);
    } else {
        println!("   - 观察 scan 模式持续跟踪机会生命周期与模拟盈亏");
    }
    println!();
}

/// 离线（API 不可达）时的简化 12 问作答。
fn print_diagnostic_answers_offline() {
    println!("{}", SEP);
    println!();
    println!("🔍 诊断报告 -- 12 问作答（API 离线）");
    println!();
    println!("{}", DASH);
    println!();
    println!("1. API 是否成功？ ❌ 否");
    println!("2. HTTP 是否正常？ ❌ 否");
    println!("3. JSON 是否正确？ ❌ 无法校验（API 不可达）");
    println!("4-10. 依赖 API 数据，均无法判定");
    println!("11. 综合结论: 🔴 API 层故障 -- 优先修复网络 / 代理 / Gamma 可达性。");
    println!("12. 下一步: 检查 HTTPS_PROXY 代理（127.0.0.1:7890）与 DNS。");
    println!();
}

/// V1.04 辅助：诊断模式中将 OppSnapshot 转为 Opportunity（供 Strategy 调用）。
fn opp_from_snap_for_diag(snap: &pm_models::OppSnapshot) -> pm_opportunity::Opportunity {
    use pm_opportunity::{Opportunity, OpportunityType};
    Opportunity::new(
        snap.question.clone(),
        snap.question.clone(),
        "diagnostics".into(),
        chrono::Utc::now(),
        OpportunityType::Unknown,
        50.0, 0.5, 50,
        25.0, 20.0, 0.0, 10.0, 5.0, 10.0,
        0.01, 1.0,
        snap.yes_price, snap.no_price, snap.sum,
        None, snap.volume, snap.liquidity,
        None, None,
    )
}
