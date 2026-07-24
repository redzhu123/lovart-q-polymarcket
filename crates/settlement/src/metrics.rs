//! Settlement Metrics（结算指标 — P2-06）。
//!
//! 跟踪结算引擎的运行指标。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicU64, Ordering};

// ============================================================================
// SettlementMetrics — 结算指标
// ============================================================================

/// 结算指标收集器。
#[derive(Debug)]
pub struct SettlementMetrics {
    /// 总成交事件数。
    pub total_fills: AtomicU64,
    /// 成功结算数。
    pub successful_settlements: AtomicU64,
    /// 失败结算数。
    pub failed_settlements: AtomicU64,
    /// 校验失败数。
    pub validation_failures: AtomicU64,
    /// 总手续费收入。
    pub total_fees_collected: std::sync::Mutex<f64>,
    /// 总已实现盈亏。
    pub total_realized_pnl: std::sync::Mutex<f64>,
    /// 总流水条目数。
    pub total_ledger_entries: AtomicU64,
    /// 总结算耗时（微秒）。
    pub total_elapsed_us: AtomicU64,
    /// 启动时间。
    pub started_at: DateTime<Local>,
}

impl SettlementMetrics {
    /// 创建新指标收集器。
    pub fn new() -> Self {
        Self {
            total_fills: AtomicU64::new(0),
            successful_settlements: AtomicU64::new(0),
            failed_settlements: AtomicU64::new(0),
            validation_failures: AtomicU64::new(0),
            total_fees_collected: std::sync::Mutex::new(0.0),
            total_realized_pnl: std::sync::Mutex::new(0.0),
            total_ledger_entries: AtomicU64::new(0),
            total_elapsed_us: AtomicU64::new(0),
            started_at: Local::now(),
        }
    }

    /// 记录一次成交事件。
    pub fn record_fill(&self) {
        self.total_fills.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录一次成功结算。
    pub fn record_success(&self, fee: f64, realized_pnl: f64, ledger_count: u64, elapsed_us: u64) {
        self.successful_settlements.fetch_add(1, Ordering::SeqCst);
        *self.total_fees_collected.lock().unwrap() += fee;
        *self.total_realized_pnl.lock().unwrap() += realized_pnl;
        self.total_ledger_entries
            .fetch_add(ledger_count, Ordering::SeqCst);
        self.total_elapsed_us
            .fetch_add(elapsed_us, Ordering::SeqCst);
    }

    /// 记录一次失败结算。
    pub fn record_failure(&self, is_validation: bool) {
        self.failed_settlements.fetch_add(1, Ordering::SeqCst);
        if is_validation {
            self.validation_failures.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// 成功率。
    pub fn success_rate(&self) -> f64 {
        let total = self.successful_settlements.load(Ordering::SeqCst)
            + self.failed_settlements.load(Ordering::SeqCst);
        if total > 0 {
            self.successful_settlements.load(Ordering::SeqCst) as f64 / total as f64
        } else {
            1.0
        }
    }

    /// 平均结算耗时（微秒）。
    pub fn avg_elapsed_us(&self) -> f64 {
        let count = self.successful_settlements.load(Ordering::SeqCst);
        if count > 0 {
            self.total_elapsed_us.load(Ordering::SeqCst) as f64 / count as f64
        } else {
            0.0
        }
    }

    /// 运行时长（秒）。
    pub fn uptime_secs(&self) -> f64 {
        let now = Local::now();
        (now - self.started_at).num_seconds() as f64
    }

    /// 生成快照。
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_fills: self.total_fills.load(Ordering::SeqCst),
            successful_settlements: self.successful_settlements.load(Ordering::SeqCst),
            failed_settlements: self.failed_settlements.load(Ordering::SeqCst),
            validation_failures: self.validation_failures.load(Ordering::SeqCst),
            total_fees_collected: *self.total_fees_collected.lock().unwrap(),
            total_realized_pnl: *self.total_realized_pnl.lock().unwrap(),
            total_ledger_entries: self.total_ledger_entries.load(Ordering::SeqCst),
            avg_elapsed_us: self.avg_elapsed_us(),
            success_rate: self.success_rate(),
            uptime_secs: self.uptime_secs(),
            captured_at: Local::now(),
        }
    }

    /// 打印指标摘要（中文 CLI 输出）。
    pub fn print_zh(&self) {
        let snap = self.snapshot();
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  Settlement Engine 指标");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  运行时长        : {:.0}s", snap.uptime_secs);
        println!("  总成交事件      : {}", snap.total_fills);
        println!("  成功结算        : {}", snap.successful_settlements);
        println!("  失败结算        : {}", snap.failed_settlements);
        println!("  校验失败        : {}", snap.validation_failures);
        println!("  成功率          : {:.1}%", snap.success_rate * 100.0);
        println!();
        println!("  总手续费收入    : {:.4} USDC", snap.total_fees_collected);
        println!("  总已实现盈亏    : {:+.2} USDC", snap.total_realized_pnl);
        println!();
        println!("  总流水条目      : {}", snap.total_ledger_entries);
        println!("  平均结算耗时    : {:.0}μs", snap.avg_elapsed_us);
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

impl Default for SettlementMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MetricsSnapshot — 指标快照
// ============================================================================

/// 指标快照。
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_fills: u64,
    pub successful_settlements: u64,
    pub failed_settlements: u64,
    pub validation_failures: u64,
    pub total_fees_collected: f64,
    pub total_realized_pnl: f64,
    pub total_ledger_entries: u64,
    pub avg_elapsed_us: f64,
    pub success_rate: f64,
    pub uptime_secs: f64,
    pub captured_at: DateTime<Local>,
}

impl MetricsSnapshot {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "结算指标: 成交 {} | 成功 {} ({:.1}%) | 失败 {} | 手续费 {:.4} | 盈亏 {:+.2} | 流水 {} | 平均耗时 {:.0}μs",
            self.total_fills,
            self.successful_settlements,
            self.success_rate * 100.0,
            self.failed_settlements,
            self.total_fees_collected,
            self.total_realized_pnl,
            self.total_ledger_entries,
            self.avg_elapsed_us,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn metrics_initial_state() {
        let m = SettlementMetrics::new();
        let snap = m.snapshot();
        assert_eq!(snap.total_fills, 0);
        assert_eq!(snap.successful_settlements, 0);
        assert!(approx(snap.success_rate, 1.0)); // 0/0 → 1.0
    }

    #[test]
    fn record_fill_increments() {
        let m = SettlementMetrics::new();
        m.record_fill();
        m.record_fill();
        assert_eq!(m.total_fills.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn record_success_updates_stats() {
        let m = SettlementMetrics::new();
        m.record_success(2.5, 10.0, 3, 1500);
        m.record_success(1.0, -5.0, 1, 500);

        let snap = m.snapshot();
        assert_eq!(snap.successful_settlements, 2);
        assert!(approx(snap.total_fees_collected, 3.5));
        assert!(approx(snap.total_realized_pnl, 5.0));
        assert_eq!(snap.total_ledger_entries, 4);
        assert!(approx(snap.avg_elapsed_us, 1000.0)); // (1500+500)/2
        assert!(approx(snap.success_rate, 1.0)); // 0 失败
    }

    #[test]
    fn success_rate_with_failures() {
        let m = SettlementMetrics::new();
        m.record_success(0.0, 0.0, 1, 100);
        m.record_success(0.0, 0.0, 1, 200);
        m.record_failure(true);
        m.record_failure(false);

        let snap = m.snapshot();
        assert!(approx(snap.success_rate, 0.5)); // 2/4
        assert_eq!(snap.validation_failures, 1);
    }

    #[test]
    fn print_zh_does_not_panic() {
        let m = SettlementMetrics::new();
        m.record_fill();
        m.record_success(1.0, 10.0, 2, 500);
        m.print_zh();
    }
}
