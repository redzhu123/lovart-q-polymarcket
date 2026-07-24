//! Execution Config（V1.06 第十五节）。
//!
//! V1.06 新增的执行配置字段，从 config.toml [execution] 段读取。
//! 全部可配置，禁止写死。

use serde::Deserialize;

use crate::queue::QueueConfig;
use crate::scheduler::SchedulerConfig;

/// V1.06 执行配置（扩展自 pm-models::ExecutionConfig）。
///
/// 包含全部 V1.06 新增字段的默认值和构建方法。
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionConfigV106 {
    // ---- 继承自 V0.9 的字段 ----
    /// 初始资金（USDC）。
    #[serde(default = "default_capital")]
    pub capital: f64,
    /// 待处理订单数上限。
    #[serde(default = "default_max_pending")]
    pub max_pending_orders: usize,
    /// 单笔订单固定成本（USDC）。
    #[serde(default = "default_order_notional")]
    pub order_notional: f64,
    /// 最大成交延迟（扫描周期数）。
    #[serde(default = "default_max_fill_delay")]
    pub max_fill_delay: u32,
    /// 最大等待扫描周期数。
    #[serde(default = "default_max_wait_scans")]
    pub max_wait_scans: u32,

    // ---- V1.06 新增字段 ----
    /// 最大队列长度。
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    /// 最大重试次数。
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 重试延迟（毫秒）。
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// 订单超时（毫秒）。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 每秒最大订单数。
    #[serde(default = "default_max_orders_per_second")]
    pub max_orders_per_second: u32,
    /// 每秒突发容量。
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
    /// Gateway 类型："mock" | "polymarket" | "kalshi"。
    #[serde(default = "default_gateway")]
    pub gateway: String,
}

// ---- 默认值函数 ----

fn default_capital() -> f64 {
    10000.0
}
fn default_max_pending() -> usize {
    20
}
fn default_order_notional() -> f64 {
    100.0
}
fn default_max_fill_delay() -> u32 {
    3
}
fn default_max_wait_scans() -> u32 {
    5
}
fn default_max_queue_size() -> usize {
    1000
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay_ms() -> u64 {
    1000
}
fn default_timeout_ms() -> u64 {
    30000
}
fn default_max_orders_per_second() -> u32 {
    10
}
fn default_burst_size() -> u32 {
    5
}
fn default_gateway() -> String {
    "mock".into()
}

impl Default for ExecutionConfigV106 {
    fn default() -> Self {
        Self {
            capital: default_capital(),
            max_pending_orders: default_max_pending(),
            order_notional: default_order_notional(),
            max_fill_delay: default_max_fill_delay(),
            max_wait_scans: default_max_wait_scans(),
            max_queue_size: default_max_queue_size(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            timeout_ms: default_timeout_ms(),
            max_orders_per_second: default_max_orders_per_second(),
            burst_size: default_burst_size(),
            gateway: default_gateway(),
        }
    }
}

impl ExecutionConfigV106 {
    /// 从 pm-models 的 ExecutionConfig 构建（兼容桥接）。
    pub fn from_pm_config(cfg: &pm_models::config::ExecutionConfig) -> Self {
        Self {
            capital: cfg.capital,
            max_pending_orders: cfg.max_pending_orders,
            order_notional: cfg.order_notional,
            max_fill_delay: cfg.max_fill_delay,
            max_wait_scans: cfg.max_wait_scans,
            ..Self::default()
        }
    }

    /// 导出 QueueConfig。
    pub fn to_queue_config(&self) -> QueueConfig {
        QueueConfig {
            max_size: self.max_queue_size,
            max_retries: self.max_retries,
            retry_delay_ms: self.retry_delay_ms,
            default_priority: 0,
        }
    }

    /// 导出 SchedulerConfig。
    pub fn to_scheduler_config(&self) -> SchedulerConfig {
        SchedulerConfig {
            max_orders_per_second: self.max_orders_per_second,
            max_orders_per_minute: 0,
            burst_size: self.burst_size,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let c = ExecutionConfigV106::default();
        assert_eq!(c.capital, 10000.0);
        assert_eq!(c.max_pending_orders, 20);
        assert_eq!(c.max_queue_size, 1000);
        assert_eq!(c.max_retries, 3);
        assert_eq!(c.max_orders_per_second, 10);
        assert_eq!(c.gateway, "mock");
    }

    #[test]
    fn to_queue_config() {
        let c = ExecutionConfigV106::default();
        let qc = c.to_queue_config();
        assert_eq!(qc.max_size, 1000);
        assert_eq!(qc.max_retries, 3);
    }

    #[test]
    fn to_scheduler_config() {
        let c = ExecutionConfigV106::default();
        let sc = c.to_scheduler_config();
        assert_eq!(sc.max_orders_per_second, 10);
        assert_eq!(sc.burst_size, 5);
    }

    #[test]
    fn from_pm_config_bridge() {
        let pm_cfg = pm_models::config::ExecutionConfig::default();
        let c = ExecutionConfigV106::from_pm_config(&pm_cfg);
        assert_eq!(c.capital, 10000.0);
        assert_eq!(c.max_pending_orders, 20);
        // V1.06 字段用默认值
        assert_eq!(c.max_queue_size, 1000);
    }
}
