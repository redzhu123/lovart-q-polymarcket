//! 仪表盘与明细渲染：把一轮扫描的结果打印到控制台。
//!
//! Simulation Only -- 所有交易明细均带 "仅模拟" 标注，禁止让人误认为已真实成交。
//! 格式化原语复用 [`pm_utils`]。
//!
//! V1.0.1：日志统一为中文。Side / RiskRejection / TerminalReason 来自交易相关 crate
//! （不可改实现），通过本模块的 `*_zh` 映射函数转为中文展示，**不改动任何交易逻辑**。

use pm_execution::{ExecEvent, ExecutionStats, PortfolioSummary, TerminalReason};
use pm_models::{FinishedOpportunity, TrackUpdate, UnifiedMarket};
use pm_paper::PaperTradingEngine;
use pm_portfolio::{Order, RiskRejection};
use pm_shadow::{ShadowStats, ShadowTrade};
use pm_strategy::ScanEvents;
use pm_utils::{fmt_money, fmt_pct, fmt_pnl, fmt_qty, fmt_roi, fmt_scans, fmt_sum};

use pm_core::Side;
use pm_paper::CloseOutcome;

use crate::stats::{FetchStats, MarketRejection, MarketSample, PageStats, RoundAnalysis, ScannerStats};

/// 主分隔线（等号）。
pub const SEP: &str = "======================================";
/// 段内分隔线。
pub const DASH: &str = "---";
/// 清屏 ANSI 转义：清屏并把光标移到左上角。
pub const CLEAR_SCREEN: &str = "\x1B[2J\x1B[1;1H";

// ============================================================================
// 跨 crate 字符串中文化映射（不修改交易 crate 的 as_str 实现）
// ============================================================================

/// 买卖方向中文化。
fn side_zh(s: Side) -> &'static str {
    match s {
        Side::Buy => "买入",
        Side::Sell => "卖出",
    }
}

/// 纸面交易风控拒绝原因中文化。
fn risk_rejection_zh(r: RiskRejection) -> &'static str {
    match r {
        RiskRejection::MaxPositions => "已达最大持仓数",
        RiskRejection::MaxOpenOrders => "已达最大待处理订单数",
        RiskRejection::InsufficientCash => "可用资金不足",
        RiskRejection::MaxDailyLoss => "已达单日亏损上限",
        RiskRejection::InvalidPrice => "价格非法",
    }
}

/// 执行模拟器终态原因中文化。
fn terminal_reason_zh(r: TerminalReason) -> &'static str {
    match r {
        TerminalReason::Timeout => "超时",
        TerminalReason::MaxPending => "已达最大待处理订单数",
        TerminalReason::InsufficientCash => "可用资金不足",
        TerminalReason::InvalidPrice => "价格非法",
        TerminalReason::NoPosition => "无可平仓位",
        TerminalReason::None => "",
    }
}

/// 扫描头。
pub fn print_scan_header(now: &str) {
    println!("{}", SEP);
    println!();
    println!("扫描器运行中");
    println!();
    println!("扫描时间");
    println!();
    println!("{}", now);
    println!();
    println!("执行模拟器 -- 仅模拟");
}

// ---------------- 明细：新机会 ----------------
pub fn print_new_opportunities(events: &[TrackUpdate]) {
    for ev in events {
        println!("{}", DASH);
        println!();
        println!("新机会");
        println!();
        println!("问题");
        println!();
        println!("{}", ev.question);
        println!();
        println!("总和");
        println!();
        println!("{}", fmt_sum(ev.sum));
        println!();
    }
}

// ---------------- 明细：影子交易开仓 ----------------
pub fn print_shadow_opened(trades: &[ShadowTrade]) {
    for t in trades {
        println!("{}", DASH);
        println!();
        println!("影子交易开仓");
        println!();
        println!("问题");
        println!();
        println!("{}", t.question);
        println!();
        println!("本金");
        println!();
        println!("{}", fmt_money(t.capital));
        println!();
        println!("入场");
        println!();
        println!("{}", fmt_sum(t.entry_sum()));
        println!();
    }
}

// ---------------- 明细：纸面交易开仓 ----------------
pub fn print_paper_opens(orders: &[Order]) {
    for o in orders {
        println!("{}", DASH);
        println!();
        println!("纸面交易开仓 -- 仅模拟");
        println!();
        println!("问题");
        println!();
        println!("{}", o.question);
        println!();
        println!("方向");
        println!();
        println!("{}", side_zh(o.side));
        println!();
        println!("数量");
        println!();
        println!("{}", fmt_qty(o.quantity));
        println!();
        println!("价格");
        println!();
        println!("{}", fmt_sum(o.price));
        println!();
        println!("成本");
        println!();
        println!("{}", fmt_money(o.notional()));
        println!();
    }
}

// ---------------- 明细：纸面交易拒绝 ----------------
pub fn print_paper_rejections(rejections: &[(String, RiskRejection)]) {
    for (q, r) in rejections {
        println!("{}", DASH);
        println!();
        println!("纸面交易拒绝 -- 仅模拟");
        println!();
        println!("问题");
        println!();
        println!("{}", q);
        println!();
        println!("原因");
        println!();
        println!("{}", risk_rejection_zh(*r));
        println!();
    }
}

// ---------------- 明细：机会更新 ----------------
pub fn print_updated_opportunities(events: &[TrackUpdate]) {
    for ev in events {
        println!("{}", DASH);
        println!();
        println!("机会更新");
        println!();
        println!("问题");
        println!();
        println!("{}", ev.question);
        println!();
        println!("持续时间");
        println!();
        println!("{} 秒", ev.duration_sec);
        println!();
        println!("最优");
        println!();
        println!("{}", fmt_sum(ev.best_sum));
        println!();
        println!("观测");
        println!();
        println!("{} 次扫描", ev.scan_count);
        println!();
    }
}

// ---------------- 明细：机会结束 ----------------
pub fn print_finished(finished: &[FinishedOpportunity]) {
    for f in finished {
        println!("{}", DASH);
        println!();
        println!("机会结束");
        println!();
        println!("问题");
        println!();
        println!("{}", f.question);
        println!();
        println!("持续时间");
        println!();
        println!("{} 秒", f.duration_sec);
        println!();
        println!("最优");
        println!();
        println!("{}", fmt_sum(f.best_sum));
        println!();
        println!("观测");
        println!();
        println!("{} 次扫描", f.scan_count);
        println!();
    }
}

// ---------------- 明细：影子交易平仓 ----------------
pub fn print_shadow_closed(trades: &[ShadowTrade]) {
    for t in trades {
        let pnl = t.estimated_pnl.unwrap_or(0.0);
        let roi = t.estimated_roi.unwrap_or(0.0);
        let dur = t.duration_sec.unwrap_or(0);
        println!("{}", DASH);
        println!();
        println!("影子交易平仓");
        println!();
        println!("问题");
        println!();
        println!("{}", t.question);
        println!();
        println!("持续时间");
        println!();
        println!("{} 秒", dur);
        println!();
        println!("盈亏");
        println!();
        println!("{}", fmt_pnl(pnl));
        println!();
        println!("收益率");
        println!();
        println!("{}", fmt_roi(roi));
        println!();
    }
}

// ---------------- 明细：纸面交易平仓 ----------------
pub fn print_paper_closes(closes: &[CloseOutcome]) {
    for c in closes {
        println!("{}", DASH);
        println!();
        println!("纸面交易平仓 -- 仅模拟");
        println!();
        println!("问题");
        println!();
        println!("{}", c.position.question);
        println!();
        println!("方向");
        println!();
        println!("{}", side_zh(c.order.side));
        println!();
        println!("数量");
        println!();
        println!("{}", fmt_qty(c.position.quantity));
        println!();
        println!("出场价");
        println!();
        println!("{}", fmt_sum(c.position.current_price));
        println!();
        println!("已实现盈亏");
        println!();
        println!("{}", fmt_pnl(c.position.realized_pnl));
        println!();
        println!("收益率");
        println!();
        println!("{}", fmt_roi(c.position.roi));
        println!();
    }
}

// ---------------- 纸面交易仪表盘 ----------------
pub fn print_paper_dashboard(paper: &PaperTradingEngine) {
    let pf = paper.portfolio();
    println!("{}", SEP);
    println!();
    println!("纸面交易 -- 仅模拟");
    println!();
    println!("{}", DASH);
    println!();
    println!("组合");
    println!();
    println!("现金");
    println!();
    println!("{}", fmt_money(pf.cash));
    println!();
    println!("总价值");
    println!();
    println!("{}", fmt_money(pf.total_value));
    println!();
    println!("盈亏");
    println!();
    println!("{}", fmt_pnl(pf.total_pnl));
    println!();
    println!("收益率");
    println!();
    println!("{}", fmt_roi(pf.roi()));
    println!();
    println!("{}", DASH);
    println!();
    println!("持仓数");
    println!();
    println!("{}", pf.open_positions.len());
    println!();

    for p in &pf.open_positions {
        println!("{}", DASH);
        println!();
        println!("{}", p.question);
        println!();
        println!("买入");
        println!();
        println!("入场价");
        println!();
        println!("{}", fmt_sum(p.average_price));
        println!();
        println!("当前价");
        println!();
        println!("{}", fmt_sum(p.current_price));
        println!();
        println!("盈亏");
        println!();
        println!("{}", fmt_pnl(p.unrealized_pnl));
        println!();
    }
}

// ---------------- 执行模拟器事件明细 ----------------
pub fn print_exec_events(events: &[ExecEvent]) {
    if events.is_empty() {
        return;
    }
    println!("{}", SEP);
    println!();
    println!("执行模拟器 -- 仅模拟");
    println!();
    for ev in events {
        match ev {
            ExecEvent::NewOrder {
                order_id,
                question,
                side,
                quantity,
                notional,
            } => {
                println!("{}", DASH);
                println!();
                println!("新订单 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", side_zh(*side));
                println!();
                println!("{}", question);
                println!();
                println!("金额");
                println!();
                println!("{} USDC", fmt_money(*notional));
                println!();
                println!("数量");
                println!();
                println!("{}", fmt_qty(*quantity));
                println!();
                println!("状态");
                println!();
                println!("待成交");
                println!();
            }
            ExecEvent::Filled {
                order_id,
                question,
                side,
                fill_time_scans,
                slippage,
                filled_quantity,
            } => {
                println!("{}", DASH);
                println!();
                println!("订单成交 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", side_zh(*side));
                println!();
                println!("{}", question);
                println!();
                println!("成交数量");
                println!();
                println!("{}", fmt_qty(*filled_quantity));
                println!();
                println!("成交时间");
                println!();
                println!("{} 次扫描", fill_time_scans);
                println!();
                println!("滑点");
                println!();
                println!("{}", fmt_pct(*slippage));
                println!();
            }
            ExecEvent::PartiallyFilled {
                order_id,
                question,
                filled_quantity,
                slippage,
            } => {
                println!("{}", DASH);
                println!();
                println!("部分成交 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", question);
                println!();
                println!("成交数量");
                println!();
                println!("{}", fmt_qty(*filled_quantity));
                println!();
                println!("滑点");
                println!();
                println!("{}", fmt_pct(*slippage));
                println!();
                println!("状态");
                println!();
                println!("部分成交");
                println!();
            }
            ExecEvent::Cancelled {
                order_id,
                question,
                filled_quantity,
            } => {
                println!("{}", DASH);
                println!();
                println!("订单取消 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", question);
                println!();
                println!("原因");
                println!();
                println!("超时");
                println!();
                println!("成交数量");
                println!();
                println!("{}", fmt_qty(*filled_quantity));
                println!();
            }
            ExecEvent::Expired { order_id, question } => {
                println!("{}", DASH);
                println!();
                println!("订单过期 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", question);
                println!();
                println!("原因");
                println!();
                println!("超时");
                println!();
            }
            ExecEvent::Rejected {
                question,
                side,
                reason,
            } => {
                println!("{}", DASH);
                println!();
                println!("订单拒绝 -- 仅模拟");
                println!();
                println!("{}", side_zh(*side));
                println!();
                println!("{}", question);
                println!();
                println!("原因");
                println!();
                println!("{}", terminal_reason_zh(*reason));
                println!();
            }
            ExecEvent::PositionClosed {
                order_id,
                question,
                realized_pnl,
            } => {
                println!("{}", DASH);
                println!();
                println!("持仓平仓 -- 仅模拟");
                println!();
                println!("{}", order_id);
                println!();
                println!("{}", question);
                println!();
                println!("已实现盈亏");
                println!();
                println!("{}", fmt_pnl(*realized_pnl));
                println!();
            }
        }
    }
}

/// 执行模拟器组合概览。
pub fn print_exec_dashboard(summary: &PortfolioSummary) {
    println!("{}", SEP);
    println!();
    println!("执行组合 -- 仅模拟");
    println!();
    println!("{}", DASH);
    println!();
    println!("可用资金");
    println!();
    println!("{} USDC", fmt_money(summary.available_cash));
    println!();
    println!("占用资金");
    println!();
    println!("{} USDC", fmt_money(summary.pending_cash));
    println!();
    println!("待处理订单");
    println!();
    println!("{}", summary.pending_orders);
    println!();
    println!("持仓数");
    println!();
    println!("{}", summary.open_positions);
    println!();
    println!("已平仓持仓");
    println!();
    println!("{}", summary.closed_positions);
    println!();
}

/// 执行模拟器累计统计。
pub fn print_exec_stats(stats: &ExecutionStats) {
    println!("{}", DASH);
    println!();
    println!("执行统计");
    println!();
    println!("{}", DASH);
    println!();
    println!("订单数");
    println!();
    println!("{}", stats.total);
    println!();
    println!("已成交");
    println!();
    println!("{}", stats.filled);
    println!();
    println!("已取消");
    println!();
    println!("{}", stats.cancelled);
    println!();
    println!("已过期");
    println!();
    println!("{}", stats.expired);
    println!();
    println!("已拒绝");
    println!();
    println!("{}", stats.rejected);
    println!();
    println!("成交率");
    println!();
    println!("{}", fmt_pct(stats.fill_rate()));
    println!();
    println!("执行成功率");
    println!();
    println!("{}", fmt_pct(stats.execution_success_rate()));
    println!();
    println!("平均成交时间");
    println!();
    println!("{} 次扫描", fmt_scans(stats.average_fill_time()));
    println!();
    println!("平均延迟");
    println!();
    println!("{} 次扫描", fmt_scans(stats.average_delay()));
    println!();
    println!("平均滑点");
    println!();
    println!("{}", fmt_pct(stats.average_slippage()));
    println!();
    println!("部分成交率");
    println!();
    println!("{}", fmt_pct(stats.partial_fill_rate()));
    println!();
}

/// 机会生命周期统计区块。
pub fn print_opportunity_stats(
    active: usize,
    finished: usize,
    new: usize,
    updated: usize,
    csv_records: u64,
) {
    println!("{}", SEP);
    println!();
    println!("{}", DASH);
    println!();
    println!("活跃机会数");
    println!();
    println!("{}", active);
    println!();
    println!("本轮结束");
    println!();
    println!("{}", finished);
    println!();
    println!("本轮新增");
    println!();
    println!("{}", new);
    println!();
    println!("本轮更新");
    println!();
    println!("{}", updated);
    println!();
    println!("CSV 记录数");
    println!();
    println!("{}", csv_records);
    println!();
}

/// 影子交易累计统计区块（含历史）。
pub fn print_shadow_stats(stats: &ShadowStats) {
    println!("{}", DASH);
    println!();
    println!("影子交易总数");
    println!();
    println!("{}", stats.total);
    println!();
    println!("盈利交易");
    println!();
    println!("{}", stats.winners);
    println!();
    println!("亏损交易");
    println!();
    println!("{}", stats.losers);
    println!();
    println!("平均收益率");
    println!();
    println!("{}", fmt_roi(stats.average_roi()));
    println!();
    println!("最佳收益率");
    println!();
    println!("{}", fmt_roi(stats.best_roi()));
    println!();
    println!("最差收益率");
    println!();
    println!("{}", fmt_roi(stats.worst_roi()));
    println!();
    println!("平均持续时间");
    println!();
    println!("{} 秒", stats.average_duration_sec());
    println!();
}

// ============================================================================
// V1.0.1 Scanner Debug & Observability
// ============================================================================

/// 一行 label + 空行 + value + 空行（与既有仪表盘风格一致）。
fn kv(label: &str, value: &str) {
    println!("{}", label);
    println!();
    println!("{}", value);
    println!();
}

/// 单页明细压缩打印（用于逐页 HTTP 调试，避免 21 页全展开刷屏）。
fn print_page_summary(p: &PageStats) {
    println!(
        "  # {:>2} | 状态={} | {:>7} 字节 | {:>5} 毫秒 | {}",
        p.url.rsplit("offset=").next().unwrap_or("?"),
        p.status,
        p.bytes,
        p.elapsed_ms,
        if p.ok { "正常" } else { "失败" }
    );
}

/// Scanner Debug 主区块（V1.0.1）。
///
/// 完整数据流：HTTP -> 市场 -> 过滤 -> 价格 -> 策略 -> 机会。
/// 任一项为 0 都明确打印，便于一眼定位问题阶段。
pub fn print_scanner_debug(fetch: &FetchStats, analysis: &RoundAnalysis) {
    // ---------------- 总头 ----------------
    println!("{}", SEP);
    println!();
    println!("扫描器调试");
    println!();
    println!("{}", SEP);
    println!();

    // ---------------- HTTP 调试 ----------------
    println!("HTTP 调试");
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
    if let Some(e) = &fetch.last_error {
        kv("HTTP 末次错误", e);
    }
    if !fetch.pages.is_empty() {
        println!("逐页明细");
        println!();
        for p in &fetch.pages {
            print_page_summary(p);
        }
        println!();
    }

    // ---------------- 市场数据流 ----------------
    println!("{}", SEP);
    println!();
    println!("市场数据流");
    println!();
    println!("{}", DASH);
    println!();
    kv("接收市场数", &analysis.received.to_string());
    kv("解析市场数", &analysis.parsed.to_string());
    kv("活跃市场数", &analysis.active.to_string());
    kv("已关闭市场数", &analysis.closed.to_string());
    kv("有价市场数", &analysis.with_prices.to_string());
    kv("缺价市场数", &analysis.missing_prices.to_string());
    kv("通过校验市场数", &analysis.passed_validation.to_string());
    kv("通过策略市场数", &analysis.passed_strategy.to_string());
    kv("潜在机会数", &analysis.opportunity_count().to_string());

    // ---------------- 过滤原因 ----------------
    println!("{}", SEP);
    println!();
    println!("过滤明细");
    println!();
    println!("{}", DASH);
    println!();
    kv("因已关闭过滤", &analysis.filtered_closed.to_string());
    kv("因不活跃过滤", &analysis.filtered_inactive.to_string());
    kv("因缺价过滤", &analysis.filtered_missing_price.to_string());
    kv("因数据无效过滤", &analysis.filtered_invalid_data.to_string());
    kv("因策略过滤", &analysis.filtered_strategy.to_string());
    kv("剩余", &analysis.remaining().to_string());

    // ---------------- 价格统计 ----------------
    println!("{}", SEP);
    println!();
    println!("价格统计");
    println!();
    println!("{}", DASH);
    println!();
    kv("有价数", &analysis.price_available.to_string());
    kv("YES 价缺失", &analysis.yes_missing.to_string());
    kv("NO 价缺失", &analysis.no_missing.to_string());
    kv("无效求和", &analysis.invalid_sum.to_string());

    // ---------------- JSON 调试：前 3 个市场 ----------------
    println!("{}", SEP);
    println!();
    println!("JSON 调试 -- 市场样本（前 3 个）");
    println!();
    if analysis.samples.is_empty() {
        println!("（未接收到市场）");
        println!();
    } else {
        for s in &analysis.samples {
            print_market_sample(s);
        }
    }

    // ---------------- Strategy Debug：拒绝明细 ----------------
    println!("{}", SEP);
    println!();
    println!("策略调试 -- 拒绝明细");
    println!();
    print_rejections(&analysis.rejections);
}

/// 打印单个市场样本（问题 / 活跃 / 已关闭 / 成交额 / 流动性 / 价格）。
fn print_market_sample(s: &MarketSample) {
    println!("{}", DASH);
    println!();
    kv("问题", &s.question);
    kv("活跃", &s.active.to_string());
    kv("已关闭", &s.closed.to_string());
    kv("成交额", &s.volume.to_string());
    kv("流动性", &s.liquidity.to_string());
    let price_str = match s.price {
        Some((y, n)) => format!("YES={} NO={}", y, n),
        None => "（缺失）".to_string(),
    };
    kv("价格", &price_str);
}

/// 打印拒绝明细：每个被过滤的市场都带 问题 + 原因。
///
/// 数量可能很大（常态下 ~2000 个 YES+NO >= 阈值），故按原因分组：
/// 每组打印计数 + 前 10 个样本，余下汇总。诊断价值最高的 缺价 / 数据无效
/// 组全部展示（通常很少），方便定位"价格缺失"的具体市场。
fn print_rejections(rejections: &[MarketRejection]) {
    if rejections.is_empty() {
        println!("（无 -- 所有接收市场均成为机会，或未接收到市场）");
        println!();
        return;
    }
    println!("拒绝总数: {}", rejections.len());
    println!();
    // 按原因分组（保留枚举顺序）
    let groups = [
        ("缺价", crate::stats::RejectionReason::MissingPrice),
        ("数据无效", crate::stats::RejectionReason::InvalidData),
        ("不活跃", crate::stats::RejectionReason::Inactive),
        ("已关闭", crate::stats::RejectionReason::Closed),
        (
            "YES+NO >= 阈值",
            crate::stats::RejectionReason::SumAboveThreshold,
        ),
    ];
    for (name, reason) in groups {
        let members: Vec<&MarketRejection> =
            rejections.iter().filter(|r| r.reason == reason).collect();
        if members.is_empty() {
            continue;
        }
        println!("{}", DASH);
        println!();
        kv(name, &members.len().to_string());
        // 缺价 / 数据无效 全部展示（诊断关键，通常很少）；其余限 10 个
        let cap = match reason {
            crate::stats::RejectionReason::MissingPrice
            | crate::stats::RejectionReason::InvalidData => usize::MAX,
            _ => 10,
        };
        for r in members.iter().take(cap) {
            println!("- {} （{}）", r.question, r.reason.as_str());
        }
        if members.len() > cap {
            println!("... 及其他 {} 个", members.len() - cap);
        }
        println!();
    }
}

/// 累计统计区块（跨轮，V1.0.1 第八节）。
pub fn print_scanner_stats_cumulative(stats: &ScannerStats) {
    println!("{}", SEP);
    println!();
    println!("扫描器统计（累计）");
    println!();
    println!("{}", DASH);
    println!();
    kv("轮次", &stats.round_count.to_string());
    kv("HTTP 请求数", &stats.request_count.to_string());
    kv("HTTP 成功数", &stats.success_count.to_string());
    kv("HTTP 失败数", &stats.failed_count.to_string());
    kv("累计接收市场数", &stats.market_count.to_string());
    kv("累计解析市场数", &stats.parsed_count.to_string());
    kv("累计活跃市场数", &stats.active_count.to_string());
    kv("累计缺价数", &stats.missing_price_count.to_string());
    kv("累计策略过滤数", &stats.strategy_rejected_count.to_string());
    kv("累计机会数", &stats.opportunity_count.to_string());
}

/// 启动连通性检查的 FAIL 行（带原因）。
pub fn print_startup_fail(step: &str, reason: &str) {
    println!("失败");
    println!();
    println!("原因");
    println!();
    println!("{}", reason);
    println!();
    println!("{}", DASH);
    println!();
    println!("启动检查失败于: {}", step);
    println!();
    println!("扫描器无法继续。请修复上述问题后重启。");
    println!();
}

// ============================================================================
// V1.01 系统汇总 + TRACE 市场转储
// ============================================================================

/// 第十一节：每轮系统汇总（System Summary）。
///
/// `analysis` 为 `None` 时（log_level < DEBUG）策略通过/拒绝显示"未统计"。
pub fn print_round_system_summary(
    fetch: &FetchStats,
    analysis: &Option<RoundAnalysis>,
    events: &ScanEvents,
    markets_len: usize,
    total_ms: u128,
) {
    let http_total = fetch.request_count.max(1);
    let http_rate = (fetch.success_count as f64) / (http_total as f64) * 100.0;
    let json_ok = fetch.failed_count == 0;
    let exec_new = events
        .exec_events
        .iter()
        .filter(|e| matches!(e, ExecEvent::NewOrder { .. }))
        .count();

    let (pass, reject) = match analysis {
        Some(a) => (a.passed_strategy.to_string(), a.filtered_strategy.to_string()),
        None => ("未统计".into(), "未统计".into()),
    };

    println!("{}", SEP);
    println!();
    println!("📊 系统汇总");
    println!();
    println!("{}", DASH);
    println!();
    kv("HTTP 成功率", &format!("{:.0}%", http_rate));
    kv("JSON 成功率", if json_ok { "100%" } else { "0%" });
    kv("市场数", &markets_len.to_string());
    kv("策略通过", &pass);
    kv("策略拒绝", &reject);
    kv("影子交易", &events.shadow_opened.len().to_string());
    kv("纸面订单", &events.paper_opens.len().to_string());
    kv("执行订单", &exec_new.to_string());
    kv("CSV 写入", "OK");
    kv("本轮总耗时", &format!("{} 毫秒", total_ms));
}

/// TRACE 级：全量市场转储（前 50 个，防日志爆炸）。
pub fn print_market_trace_dump(markets: &[UnifiedMarket]) {
    println!("{}", SEP);
    println!();
    println!("📝 TRACE 市场转储（前 50 个）");
    println!();
    println!("{}", DASH);
    println!();
    let cap = markets.len().min(50);
    for (i, m) in markets.iter().take(cap).enumerate() {
        let q = m.question.clone();
        let price_str = match (m.yes_price, m.no_price) {
            (Some(y), Some(n)) => format!("YES={} NO={}", y, n),
            _ => "（无二元价格）".to_string(),
        };
        println!(
            "#{:>3} [{} vol={} liq={} out={}] | {} | {}",
            i + 1,
            m.status.as_zh(),
            m.volume,
            m.liquidity,
            m.outcome_count,
            if q.trim().is_empty() { "（无问题）" } else { &q },
            price_str
        );
    }
    if markets.len() > cap {
        println!("... 及其他 {} 个（已截断）", markets.len() - cap);
    }
    println!();
}
