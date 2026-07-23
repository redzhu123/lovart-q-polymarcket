//! Execution Metrics（V1.06 第十二节）。
//!
//! 实时执行指标，全部中文展示。
//!
//! 指标包括：
//! - 订单成功率 / 平均成交时间 / 平均等待时间
//! - 平均重试次数 / 平均滑点 / 成交率 / 拒绝率
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use crate::events::ExecutionEvent;

// ============================================================================
// Execution Metrics
// ============================================================================

/// 执行指标（V1.06 第十二节）。
///
/// 从 ExecutionEvent 流中实时更新。
#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    // ---- 累计计数 ----
    /// 总提交数。
    pub total_submitted: u64,
    /// 总接受数（Accepted + 以上）。
    pub total_accepted: u64,
    /// 总拒绝数。
    pub total_rejected: u64,
    /// 完全成交数。
    pub total_filled: u64,
    /// 取消数。
    pub total_cancelled: u64,
    /// 过期数。
    pub total_expired: u64,
    /// 失败数。
    pub total_failed: u64,
    /// 重试次数。
    pub total_retries: u64,

    // ---- 累计值 ----
    /// 成交时间总和（毫秒）。
    fill_time_sum_ms: f64,
    /// 等待时间总和（毫秒）。
    wait_time_sum_ms: f64,
    /// 滑点总和。
    slippage_sum: f64,
    /// 有成交时间的订单数。
    fill_time_count: u64,
    /// 有等待时间的订单数。
    wait_time_count: u64,

    // ---- 派生指标 ----
    /// 订单成功率 = filled / total。
    pub success_rate: f64,
    /// 拒绝率 = rejected / total。
    pub rejection_rate: f64,
    /// 成交率 = (filled + partial) / total。
    pub fill_rate: f64,
    /// 平均成交时间（毫秒）。
    pub avg_fill_time_ms: f64,
    /// 平均等待时间（毫秒）。
    pub avg_wait_time_ms: f64,
    /// 平均重试次数。
    pub avg_retries: f64,
    /// 平均滑点（小数形式）。
    pub avg_slippage: f64,
}

impl ExecutionMetrics {
    pub fn new() -> Self {
        Self {
            total_submitted: 0,
            total_accepted: 0,
            total_rejected: 0,
            total_filled: 0,
            total_cancelled: 0,
            total_expired: 0,
            total_failed: 0,
            total_retries: 0,
            fill_time_sum_ms: 0.0,
            wait_time_sum_ms: 0.0,
            slippage_sum: 0.0,
            fill_time_count: 0,
            wait_time_count: 0,
            success_rate: 0.0,
            rejection_rate: 0.0,
            fill_rate: 0.0,
            avg_fill_time_ms: 0.0,
            avg_wait_time_ms: 0.0,
            avg_retries: 0.0,
            avg_slippage: 0.0,
        }
    }

    /// 处理一个执行事件，更新指标。
    pub fn record(&mut self, event: &ExecutionEvent) {
        match event {
            ExecutionEvent::OrderCreated { .. } => {
                self.total_submitted += 1;
            }
            ExecutionEvent::OrderAccepted { .. } => {
                self.total_accepted += 1;
            }
            ExecutionEvent::OrderRejected { .. } => {
                self.total_rejected += 1;
            }
            ExecutionEvent::OrderFilled { slippage, .. } => {
                self.total_filled += 1;
                self.slippage_sum += slippage;
            }
            ExecutionEvent::OrderCancelled { .. } => {
                self.total_cancelled += 1;
            }
            ExecutionEvent::OrderExpired { .. } => {
                self.total_expired += 1;
            }
            ExecutionEvent::OrderFailed { .. } => {
                self.total_failed += 1;
            }
            ExecutionEvent::OrderRetry { .. } => {
                self.total_retries += 1;
            }
            _ => {}
        }
        self.recalculate();
    }

    /// 从一批事件批量更新。
    pub fn record_batch(&mut self, events: &[ExecutionEvent]) {
        for event in events {
            self.record(event);
        }
    }

    /// 合并另一个 Metrics（用于累加多轮扫描）。
    pub fn merge(&mut self, other: &ExecutionMetrics) {
        self.total_submitted += other.total_submitted;
        self.total_accepted += other.total_accepted;
        self.total_rejected += other.total_rejected;
        self.total_filled += other.total_filled;
        self.total_cancelled += other.total_cancelled;
        self.total_expired += other.total_expired;
        self.total_failed += other.total_failed;
        self.total_retries += other.total_retries;
        self.fill_time_sum_ms += other.fill_time_sum_ms;
        self.wait_time_sum_ms += other.wait_time_sum_ms;
        self.slippage_sum += other.slippage_sum;
        self.fill_time_count += other.fill_time_count;
        self.wait_time_count += other.wait_time_count;
        self.recalculate();
    }

    /// 记录一次成交时间（毫秒）。
    pub fn record_fill_time(&mut self, ms: f64) {
        self.fill_time_sum_ms += ms;
        self.fill_time_count += 1;
        self.recalculate();
    }

    /// 记录一次等待时间（毫秒）。
    pub fn record_wait_time(&mut self, ms: f64) {
        self.wait_time_sum_ms += ms;
        self.wait_time_count += 1;
        self.recalculate();
    }

    /// 重新计算派生指标。
    fn recalculate(&mut self) {
        let total = self.total_submitted.max(1) as f64;
        self.success_rate = self.total_filled as f64 / total;
        self.rejection_rate = self.total_rejected as f64 / total;
        self.fill_rate = (self.total_filled + self.total_cancelled) as f64 / total;
        self.avg_fill_time_ms = if self.fill_time_count > 0 {
            self.fill_time_sum_ms / self.fill_time_count as f64
        } else {
            0.0
        };
        self.avg_wait_time_ms = if self.wait_time_count > 0 {
            self.wait_time_sum_ms / self.wait_time_count as f64
        } else {
            0.0
        };
        self.avg_retries = self.total_retries as f64 / total;
        self.avg_slippage = if self.total_filled > 0 {
            self.slippage_sum / self.total_filled as f64
        } else {
            0.0
        };
    }

    /// 打印指标（中文）。
    pub fn print_zh(&self) {
        println!("【执行指标】");
        println!();
        println!("── 计数 ──");
        println!("  总提交   : {}", self.total_submitted);
        println!("  已接受   : {}", self.total_accepted);
        println!("  已拒绝   : {}", self.total_rejected);
        println!("  已成交   : {}", self.total_filled);
        println!("  已取消   : {}", self.total_cancelled);
        println!("  已过期   : {}", self.total_expired);
        println!("  失败     : {}", self.total_failed);
        println!("  重试次数 : {}", self.total_retries);
        println!();
        println!("── 比率 ──");
        println!("  成功率   : {}", pm_utils::fmt_pct(self.success_rate));
        println!("  拒绝率   : {}", pm_utils::fmt_pct(self.rejection_rate));
        println!("  成交率   : {}", pm_utils::fmt_pct(self.fill_rate));
        println!();
        println!("── 平均 ──");
        println!("  成交时间 : {:.1} ms", self.avg_fill_time_ms);
        println!("  等待时间 : {:.1} ms", self.avg_wait_time_ms);
        println!("  重试次数 : {:.2}", self.avg_retries);
        println!("  滑点     : {}", pm_utils::fmt_pct(self.avg_slippage));
    }
}

impl Default for ExecutionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn record_events_updates_metrics() {
        let now = Local::now();
        let mut m = ExecutionMetrics::new();

        m.record(&ExecutionEvent::OrderCreated {
            order_id: "EX-001".into(),
            timestamp: now,
        });
        assert_eq!(m.total_submitted, 1);

        m.record(&ExecutionEvent::OrderAccepted {
            order_id: "EX-001".into(),
            gateway: "Mock".into(),
            timestamp: now,
        });
        assert_eq!(m.total_accepted, 1);

        m.record(&ExecutionEvent::OrderFilled {
            order_id: "EX-001".into(),
            avg_price: 0.45,
            slippage: 0.005,
            timestamp: now,
        });
        assert_eq!(m.total_filled, 1);
    }

    #[test]
    fn merge_accumulates() {
        let now = Local::now();
        let mut m1 = ExecutionMetrics::new();
        m1.record(&ExecutionEvent::OrderCreated {
            order_id: "EX-001".into(),
            timestamp: now,
        });
        m1.record(&ExecutionEvent::OrderFilled {
            order_id: "EX-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: now,
        });

        let mut m2 = ExecutionMetrics::new();
        m2.record(&ExecutionEvent::OrderCreated {
            order_id: "EX-002".into(),
            timestamp: now,
        });
        m2.record(&ExecutionEvent::OrderRejected {
            order_id: "EX-002".into(),
            reason: "资金不足".into(),
            timestamp: now,
        });

        m1.merge(&m2);
        assert_eq!(m1.total_submitted, 2);
        assert_eq!(m1.total_filled, 1);
        assert_eq!(m1.total_rejected, 1);
    }

    #[test]
    fn record_batch() {
        let now = Local::now();
        let events = vec![
            ExecutionEvent::OrderCreated { order_id: "1".into(), timestamp: now },
            ExecutionEvent::OrderCreated { order_id: "2".into(), timestamp: now },
            ExecutionEvent::OrderCreated { order_id: "3".into(), timestamp: now },
        ];
        let mut m = ExecutionMetrics::new();
        m.record_batch(&events);
        assert_eq!(m.total_submitted, 3);
    }

    #[test]
    fn empty_metrics_is_safe() {
        let m = ExecutionMetrics::new();
        // 不应 panic
        m.print_zh();
    }
}
