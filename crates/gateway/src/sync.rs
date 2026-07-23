//! Order / Position / Balance Synchronization（V1.08 第五节）。
//!
//! 统一同步接口。Execution 统一经 Gateway 读取状态，禁止 Portfolio 主动 HTTP。

use chrono::{DateTime, Local};
use tracing;

use crate::metrics::GatewayMetrics;
use crate::traits::ExchangeGateway;
use crate::types::{Balance, GatewayResult, Position};

// ============================================================================
// SyncManager（同步管理器）
// ============================================================================

/// 同步管理器：统一管理订单/余额/持仓的定期同步。
pub struct SyncManager {
    /// 上次订单同步时间。
    last_order_sync: Option<DateTime<Local>>,
    /// 上次余额同步时间。
    last_balance_sync: Option<DateTime<Local>>,
    /// 上次持仓同步时间。
    last_position_sync: Option<DateTime<Local>>,
    /// 订单同步间隔（秒）。
    order_sync_interval_secs: u64,
    /// 余额同步间隔（秒）。
    balance_sync_interval_secs: u64,
    /// 持仓同步间隔（秒）。
    position_sync_interval_secs: u64,
    /// 缓存的余额。
    cached_balance: Option<Balance>,
    /// 缓存的持仓。
    cached_positions: Vec<Position>,
}

impl SyncManager {
    /// 创建新的同步管理器。
    pub fn new(
        order_sync_interval_secs: u64,
        balance_sync_interval_secs: u64,
        position_sync_interval_secs: u64,
    ) -> Self {
        Self {
            last_order_sync: None,
            last_balance_sync: None,
            last_position_sync: None,
            order_sync_interval_secs,
            balance_sync_interval_secs,
            position_sync_interval_secs,
            cached_balance: None,
            cached_positions: Vec::new(),
        }
    }

    /// 检查是否需要订单同步。
    pub fn needs_order_sync(&self, now: DateTime<Local>) -> bool {
        match self.last_order_sync {
            None => true,
            Some(last) => {
                let elapsed = (now - last).num_seconds();
                elapsed >= self.order_sync_interval_secs as i64
            }
        }
    }

    /// 检查是否需要余额同步。
    pub fn needs_balance_sync(&self, now: DateTime<Local>) -> bool {
        match self.last_balance_sync {
            None => true,
            Some(last) => {
                let elapsed = (now - last).num_seconds();
                elapsed >= self.balance_sync_interval_secs as i64
            }
        }
    }

    /// 检查是否需要持仓同步。
    pub fn needs_position_sync(&self, now: DateTime<Local>) -> bool {
        match self.last_position_sync {
            None => true,
            Some(last) => {
                let elapsed = (now - last).num_seconds();
                elapsed >= self.position_sync_interval_secs as i64
            }
        }
    }

    /// 同步订单（从 Gateway 拉取所有活跃订单）。
    pub async fn sync_orders(
        &mut self,
        gateway: &dyn ExchangeGateway,
        metrics: &mut GatewayMetrics,
        now: DateTime<Local>,
    ) -> Vec<GatewayResult> {
        tracing::info!("开始订单同步...");
        let start = std::time::Instant::now();

        let orders = gateway.list_orders().await;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        metrics.record_sync(elapsed_ms);
        self.last_order_sync = Some(now);

        tracing::info!(
            count = %orders.len(),
            elapsed_ms = %elapsed_ms,
            "订单同步完成"
        );

        orders
    }

    /// 同步余额（从 Gateway 拉取最新余额）。
    pub async fn sync_balance(
        &mut self,
        gateway: &dyn ExchangeGateway,
        metrics: &mut GatewayMetrics,
        now: DateTime<Local>,
    ) -> anyhow::Result<Balance> {
        tracing::info!("开始余额同步...");
        let start = std::time::Instant::now();

        let balance = gateway.get_balance().await?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        metrics.record_balance_sync(elapsed_ms);
        self.cached_balance = Some(balance.clone());
        self.last_balance_sync = Some(now);

        tracing::info!(
            available = %balance.available,
            total = %balance.total,
            elapsed_ms = %elapsed_ms,
            "余额同步完成"
        );

        Ok(balance)
    }

    /// 同步持仓（从 Gateway 拉取最新持仓）。
    pub async fn sync_positions(
        &mut self,
        gateway: &dyn ExchangeGateway,
        metrics: &mut GatewayMetrics,
        now: DateTime<Local>,
    ) -> anyhow::Result<Vec<Position>> {
        tracing::info!("开始持仓同步...");
        let start = std::time::Instant::now();

        let positions = gateway.get_positions().await?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        metrics.record_position_sync(elapsed_ms);
        self.cached_positions = positions.clone();
        self.last_position_sync = Some(now);

        tracing::info!(
            count = %positions.len(),
            elapsed_ms = %elapsed_ms,
            "持仓同步完成"
        );

        Ok(positions)
    }

    /// 全量同步（订单 + 余额 + 持仓），按需执行。
    pub async fn sync_all(
        &mut self,
        gateway: &dyn ExchangeGateway,
        metrics: &mut GatewayMetrics,
        now: DateTime<Local>,
    ) -> SyncReport {
        let mut report = SyncReport::default();

        // 订单同步
        if self.needs_order_sync(now) {
            match self.sync_orders(gateway, metrics, now).await {
                orders => {
                    report.orders_synced = orders.len();
                }
            }
        } else {
            report.orders_skipped = true;
        }

        // 余额同步
        if self.needs_balance_sync(now) {
            match self.sync_balance(gateway, metrics, now).await {
                Ok(_) => report.balance_synced = true,
                Err(e) => {
                    tracing::warn!(error = %e, "余额同步失败");
                    report.errors.push(format!("余额同步: {}", e));
                }
            }
        } else {
            report.balance_skipped = true;
        }

        // 持仓同步
        if self.needs_position_sync(now) {
            match self.sync_positions(gateway, metrics, now).await {
                Ok(positions) => report.positions_synced = positions.len(),
                Err(e) => {
                    tracing::warn!(error = %e, "持仓同步失败");
                    report.errors.push(format!("持仓同步: {}", e));
                }
            }
        } else {
            report.positions_skipped = true;
        }

        report
    }

    /// 缓存的余额。
    pub fn cached_balance(&self) -> Option<&Balance> {
        self.cached_balance.as_ref()
    }

    /// 缓存的持仓。
    pub fn cached_positions(&self) -> &[Position] {
        &self.cached_positions
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new(5, 30, 15)
    }
}

// ============================================================================
// SyncReport
// ============================================================================

/// 同步报告。
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// 同步的订单数。
    pub orders_synced: usize,
    /// 订单是否被跳过。
    pub orders_skipped: bool,
    /// 余额是否已同步。
    pub balance_synced: bool,
    /// 余额是否被跳过。
    pub balance_skipped: bool,
    /// 同步的持仓数。
    pub positions_synced: usize,
    /// 持仓是否被跳过。
    pub positions_skipped: bool,
    /// 错误列表。
    pub errors: Vec<String>,
}

impl SyncReport {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let mut lines = vec!["【同步报告】".to_string()];

        if self.orders_skipped {
            lines.push("  订单同步: ⏭️ 跳过（未到间隔）".to_string());
        } else {
            lines.push(format!("  订单同步: ✅ {} 个", self.orders_synced));
        }

        if self.balance_skipped {
            lines.push("  余额同步: ⏭️ 跳过（未到间隔）".to_string());
        } else if self.balance_synced {
            lines.push("  余额同步: ✅".to_string());
        } else {
            lines.push("  余额同步: ❌ 失败".to_string());
        }

        if self.positions_skipped {
            lines.push("  持仓同步: ⏭️ 跳过（未到间隔）".to_string());
        } else {
            lines.push(format!("  持仓同步: ✅ {} 个", self.positions_synced));
        }

        for err in &self.errors {
            lines.push(format!("  ❌ {}", err));
        }

        lines.join("\n")
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_manager_initial_needs_sync() {
        let mgr = SyncManager::default();
        let now = Local::now();
        assert!(mgr.needs_order_sync(now));
        assert!(mgr.needs_balance_sync(now));
        assert!(mgr.needs_position_sync(now));
    }

    #[test]
    fn sync_report_summary_zh() {
        let report = SyncReport {
            orders_synced: 5,
            balance_synced: true,
            positions_synced: 3,
            ..Default::default()
        };
        let summary = report.summary_zh();
        assert!(summary.contains("5 个"));
        assert!(summary.contains("余额同步"));
        assert!(summary.contains("持仓同步"));
    }

    #[test]
    fn sync_report_with_errors() {
        let report = SyncReport {
            orders_synced: 0,
            balance_synced: false,
            positions_synced: 0,
            errors: vec!["余额同步: 网络超时".to_string()],
            ..Default::default()
        };
        let summary = report.summary_zh();
        assert!(summary.contains("网络超时"));
    }
}
