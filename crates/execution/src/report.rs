//! Execution Report（V1.06 第九节）。
//!
//! 记录并展示 Execution Engine 的运行统计。
//! 全部以中文展示。
//!
//! 指标包括：订单数量 / 成功 / 失败 / 拒绝 / 超时 / 平均耗时 / 平均滑点 /
//!           部分成交率 / 取消率。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use serde::Serialize;

use crate::order::{Order, OrderStatus};

// ============================================================================
// Execution Report
// ============================================================================

/// 执行报告（V1.06 第九节）。
///
/// 从订单列表聚合计算所有指标。
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionReport {
    // ---- 订单计数 ----
    /// 总订单数。
    pub total_orders: u64,
    /// 成功订单数（Filled）。
    pub success_count: u64,
    /// 失败订单数（Failed）。
    pub failed_count: u64,
    /// 拒绝订单数（Rejected）。
    pub rejected_count: u64,
    /// 超时订单数（Expired）。
    pub timeout_count: u64,
    /// 取消订单数（Cancelled）。
    pub cancelled_count: u64,

    // ---- 比率 ----
    /// 成功率 = success / total。
    pub success_rate: f64,
    /// 失败率 = failed / total。
    pub failure_rate: f64,
    /// 拒绝率 = rejected / total。
    pub rejection_rate: f64,
    /// 取消率 = cancelled / total。
    pub cancel_rate: f64,
    /// 部分成交率 = 经历过 PartiallyFilled 的 / total。
    pub partial_fill_rate: f64,

    // ---- 平均指标 ----
    /// 平均耗时（从创建到终态，秒）。
    pub avg_latency_secs: f64,
    /// 平均滑点（小数形式，仅 Filled）。
    pub avg_slippage: f64,
    /// 平均成交率 = mean(fill_rate)。
    pub avg_fill_rate: f64,
    /// 平均重试次数。
    pub avg_retries: f64,
}

impl ExecutionReport {
    /// 从订单列表生成报告。
    pub fn from_orders(orders: &[Order]) -> Self {
        let total = orders.len() as u64;
        if total == 0 {
            return Self::empty();
        }

        let success_count = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Filled)
            .count() as u64;
        let failed_count = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Failed)
            .count() as u64;
        let rejected_count = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Rejected)
            .count() as u64;
        let timeout_count = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Expired)
            .count() as u64;
        let cancelled_count = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Cancelled)
            .count() as u64;

        // 部分成交率
        let partial_count = orders
            .iter()
            .filter(|o| {
                o.status_history
                    .iter()
                    .any(|s| s.to == OrderStatus::PartiallyFilled)
            })
            .count() as u64;

        // 比率
        let success_rate = pm_utils::ratio(success_count, total);
        let failure_rate = pm_utils::ratio(failed_count, total);
        let rejection_rate = pm_utils::ratio(rejected_count, total);
        let cancel_rate = pm_utils::ratio(cancelled_count, total);
        let partial_fill_rate = pm_utils::ratio(partial_count, total);

        // 平均耗时（从 Created 到终态的时间差）
        let latencies: Vec<f64> = orders
            .iter()
            .filter(|o| o.status.is_terminal())
            .map(|o| {
                let create_ms = o.create_time.timestamp_millis() as f64;
                let update_ms = o.update_time.timestamp_millis() as f64;
                (update_ms - create_ms) / 1000.0
            })
            .collect();
        let avg_latency_secs = pm_utils::mean(&latencies);

        // 平均滑点
        let slippages: Vec<f64> = orders
            .iter()
            .filter(|o| o.status == OrderStatus::Filled && o.slippage.is_finite())
            .map(|o| o.slippage)
            .collect();
        let avg_slippage = pm_utils::mean(&slippages);

        // 平均成交率
        let fill_rates: Vec<f64> = orders
            .iter()
            .filter(|o| o.filled > 0.0)
            .map(|o| o.fill_rate())
            .collect();
        let avg_fill_rate = pm_utils::mean(&fill_rates);

        // 平均重试次数
        let retries: Vec<f64> = orders.iter().map(|o| o.retry_count as f64).collect();
        let avg_retries = pm_utils::mean(&retries);

        Self {
            total_orders: total,
            success_count,
            failed_count,
            rejected_count,
            timeout_count,
            cancelled_count,
            success_rate,
            failure_rate,
            rejection_rate,
            cancel_rate,
            partial_fill_rate,
            avg_latency_secs,
            avg_slippage,
            avg_fill_rate,
            avg_retries,
        }
    }

    /// 空报告。
    pub fn empty() -> Self {
        Self {
            total_orders: 0,
            success_count: 0,
            failed_count: 0,
            rejected_count: 0,
            timeout_count: 0,
            cancelled_count: 0,
            success_rate: 0.0,
            failure_rate: 0.0,
            rejection_rate: 0.0,
            cancel_rate: 0.0,
            partial_fill_rate: 0.0,
            avg_latency_secs: 0.0,
            avg_slippage: 0.0,
            avg_fill_rate: 0.0,
            avg_retries: 0.0,
        }
    }

    /// 打印报告（中文）。
    pub fn print(&self) {
        println!("【执行报告】");
        println!();
        println!("══════════════════════════════════════");
        println!();
        println!("── 订单统计 ──");
        println!();
        println!("  总订单数     : {}", self.total_orders);
        println!("  成功（成交） : {}", self.success_count);
        println!("  失败         : {}", self.failed_count);
        println!("  拒绝         : {}", self.rejected_count);
        println!("  超时         : {}", self.timeout_count);
        println!("  取消         : {}", self.cancelled_count);
        println!();
        println!("── 比率 ──");
        println!();
        println!("  成功率       : {}", pm_utils::fmt_pct(self.success_rate));
        println!("  失败率       : {}", pm_utils::fmt_pct(self.failure_rate));
        println!(
            "  拒绝率       : {}",
            pm_utils::fmt_pct(self.rejection_rate)
        );
        println!("  取消率       : {}", pm_utils::fmt_pct(self.cancel_rate));
        println!(
            "  部分成交率   : {}",
            pm_utils::fmt_pct(self.partial_fill_rate)
        );
        println!();
        println!("── 平均指标 ──");
        println!();
        println!("  平均耗时     : {:.2} 秒", self.avg_latency_secs);
        println!("  平均滑点     : {}", pm_utils::fmt_pct(self.avg_slippage));
        println!("  平均成交率   : {}", pm_utils::fmt_pct(self.avg_fill_rate));
        println!("  平均重试     : {:.1} 次", self.avg_retries);
        println!();
        println!("══════════════════════════════════════");
        println!();
        println!("仅模拟 -- 非真实交易数据");
    }

    /// 保存到 CSV。
    pub fn to_csv(&self, path: &str) -> anyhow::Result<()> {
        let header = &[
            "total_orders",
            "success_count",
            "failed_count",
            "rejected_count",
            "timeout_count",
            "cancelled_count",
            "success_rate",
            "failure_rate",
            "rejection_rate",
            "cancel_rate",
            "partial_fill_rate",
            "avg_latency_secs",
            "avg_slippage",
            "avg_fill_rate",
            "avg_retries",
        ];
        pm_storage::ensure_csv(path, header)?;
        let records = vec![self.clone()];
        pm_storage::append_records(path, &records);
        Ok(())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn make_order(id: &str, status: OrderStatus, filled: f64, qty: f64) -> Order {
        let now = Local::now();
        let mut o = Order::new(
            id.into(),
            format!("C-{}", id),
            "mkt-1".into(),
            "mock".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            qty,
            "S1".into(),
            "R1".into(),
            "O1".into(),
            now,
        );
        // 直接设置状态（跳过 transition 验证）
        o.status = status;
        o.filled = filled;
        o.remaining = qty - filled;
        o
    }

    #[test]
    fn empty_report() {
        let report = ExecutionReport::from_orders(&[]);
        assert_eq!(report.total_orders, 0);
        assert_eq!(report.success_count, 0);
    }

    #[test]
    fn report_counts_correctly() {
        let orders = vec![
            make_order("EX-001", OrderStatus::Filled, 100.0, 100.0),
            make_order("EX-002", OrderStatus::Filled, 200.0, 200.0),
            make_order("EX-003", OrderStatus::Rejected, 0.0, 100.0),
            make_order("EX-004", OrderStatus::Failed, 0.0, 100.0),
            make_order("EX-005", OrderStatus::Expired, 0.0, 100.0),
        ];
        let report = ExecutionReport::from_orders(&orders);
        assert_eq!(report.total_orders, 5);
        assert_eq!(report.success_count, 2);
        assert_eq!(report.rejected_count, 1);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.timeout_count, 1);
        assert!((report.success_rate - 0.4).abs() < 1e-9);
    }
}
