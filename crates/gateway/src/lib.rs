//! pm-gateway：Exchange Gateway 统一交易所网关（P2-03）。
//!
//! 统一所有交易所（Polymarket / Kalshi / DEX / CEX）的订单/余额/持仓接口。
//! Execution 只能通过此 crate 调用，禁止直接 HTTP / Provider。
//!
//! # 安全
//!
//! 默认 DryRun — 真实下单必须显式配置 `enable_live=true`。
//!
//! # 模块
//!
//! - [`traits`]：ExchangeGateway trait — 统一所有交易所接口。
//! - [`error`]：GatewayError — 统一错误类型（P2-03）。
//! - [`types`]：共享类型 — GatewayResult / OrderRequest / Balance / Position / GatewayInfo / Market。
//! - [`config`]：GatewayConfig — 全部可配置，禁止写死。
//! - [`transport`]：Transport 抽象层 — REST / WebSocket（P2-03）。
//! - [`middleware`]：Middleware 中间件链（P2-03）。
//! - [`auth`]：认证模块 — PolymarketAuth / NoopAuth（P2-03）。
//! - [`ratelimit`]：速率限制 — TokenBucket（P2-03）。
//! - [`mock`]：MockGateway — Paper / Replay / Test 使用。
//! - [`polymarket`]：PolymarketGateway — 真实 Polymarket API。
//! - [`adapter`]：JSON ↔ Order 统一转换。
//! - [`retry`]：Retry / Backoff / CircuitBreaker。
//! - [`metrics`]：GatewayMetrics — API 延迟 / HTTP 成功率 / 同步耗时。
//! - [`sync`]：SyncManager — Order / Balance / Position 同步。
//! - [`health`]：HealthChecker — 健康检查 + 报告。
//! - [`diagnostics`]：诊断函数 — gateway / account / balance / orders。

pub mod adapter;
pub mod auth;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod health;
pub mod metrics;
pub mod middleware;
pub mod mock;
pub mod polymarket;
pub mod ratelimit;
pub mod retry;
pub mod router;
pub mod sync;
pub mod traits;
pub mod transport;
pub mod types;

// ---- 核心导出 ----
pub use adapter::{
    PolymarketBalanceJson, PolymarketOrderJson, PolymarketPositionJson, apply_result_to_order,
    order_to_request, parse_polymarket_side, request_to_order, to_polymarket_side,
};
pub use config::GatewayConfig;
pub use diagnostics::{
    diagnose_account, diagnose_balance, diagnose_circuit_breaker, diagnose_config,
    diagnose_gateway, diagnose_health_extended, diagnose_metrics, diagnose_orders,
    diagnose_prometheus,
};
pub use error::GatewayError;
pub use health::{HealthChecker, HealthRecord, HealthReport};
pub use metrics::prometheus::{Counter, GatewayPrometheusMetrics, Gauge, Histogram};
pub use metrics::{GatewayMetrics, GatewayMetricsRecord};
pub use mock::MockGateway;
pub use polymarket::PolymarketGateway;
pub use ratelimit::RateLimiter;
pub use retry::{Backoff, CircuitBreaker, CircuitState, RetryError, RetryExecutor};
pub use router::{GatewayRouter, RoutedOrderRequest};
pub use sync::{SyncManager, SyncReport};
pub use traits::ExchangeGateway;
pub use transport::rest::{HttpMethod, HttpRequest, HttpResponse, HttpTransport};
pub use transport::websocket::{WsMessage, WsTransport};
pub use types::{
    Balance, GatewayInfo, GatewayResult, Market, OrderBook, OrderRequest, OrderType, Position,
    TimeInForce,
};

// ============================================================================
// 工厂函数
// ============================================================================

/// 根据配置创建对应的 Gateway 实例。
///
/// # 参数
///
/// - `cfg`：Gateway 配置。
///
/// # 返回
///
/// Box<dyn ExchangeGateway> — 类型擦除的 Gateway 实例。
pub fn create_gateway(cfg: &GatewayConfig) -> Box<dyn ExchangeGateway> {
    match cfg.gateway_type.as_str() {
        "polymarket" => {
            tracing::info!("创建 PolymarketGateway");
            Box::new(PolymarketGateway::new(cfg.clone()))
        }
        "mock" | _ => {
            tracing::info!("创建 MockGateway（默认）");
            Box::new(MockGateway::new(cfg.clone()))
        }
    }
}

/// 创建 MockGateway（便捷函数）。
pub fn create_mock_gateway() -> Box<dyn ExchangeGateway> {
    Box::new(MockGateway::default())
}

/// 创建 PolymarketGateway（便捷函数，默认 DryRun）。
pub fn create_polymarket_gateway() -> Box<dyn ExchangeGateway> {
    let cfg = GatewayConfig {
        gateway_type: "polymarket".into(),
        ..GatewayConfig::default()
    };
    Box::new(PolymarketGateway::new(cfg))
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use pm_core::Side;
    use pm_execution::order::Direction;

    /// 完整 Gateway 生命周期测试（Mock）。
    #[tokio::test]
    async fn full_mock_gateway_lifecycle() {
        let gateway = create_mock_gateway();

        // 1. 信息
        let info = gateway.info();
        assert_eq!(info.gateway_type, "mock");
        assert!(!info.live_enabled);

        // 2. Ping
        assert!(gateway.ping().await);

        // 3. 健康检查
        let health = gateway.health().await;
        assert!(health.healthy);

        // 4. 余额
        let balance = gateway.get_balance().await.unwrap();
        assert!(balance.total > 0.0);

        // 5. 持仓
        let positions = gateway.get_positions().await.unwrap();
        assert!(positions.is_empty());

        // 6. 下单
        let req = OrderRequest::new(
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S",
            "R",
            "O",
        );
        let result = gateway.submit_order(&req, Local::now()).await;
        assert!(!result.gateway_order_id.is_empty());

        // 7. 查询订单
        let found = gateway.get_order(&result.gateway_order_id).await;
        assert!(!found.gateway_order_id.is_empty());

        // 8. 订单列表
        let orders = gateway.list_orders().await;
        assert!(!orders.is_empty());

        // 9. 取消
        let cancel = gateway.cancel_order(&result.gateway_order_id).await;
        assert!(cancel.success);

        // 10. 替换
        let req2 = OrderRequest::new(
            "mkt-2",
            Direction::No,
            Side::Sell,
            0.50,
            200.0,
            "S",
            "R",
            "O",
        );
        let replace = gateway.replace_order("MOCK-old", &req2, Local::now()).await;
        assert!(!replace.gateway_order_id.is_empty());
    }

    /// 安全测试：DryRun 模式下 PolymarketGateway 拒绝下单。
    #[tokio::test]
    async fn polymarket_dry_run_rejects_orders() {
        let gateway = create_polymarket_gateway();
        assert!(!gateway.live_enabled());

        let req = OrderRequest::new(
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S",
            "R",
            "O",
        );
        let result = gateway.submit_order(&req, Local::now()).await;
        assert!(!result.success);
        assert!(result.message.contains("DryRun"));
    }

    /// 工厂函数：create_gateway 按类型创建。
    #[test]
    fn factory_creates_mock_by_default() {
        let cfg = GatewayConfig::default();
        let gw = create_gateway(&cfg);
        assert_eq!(gw.gateway_type(), "mock");
    }

    /// 工厂函数：create_gateway 创建 Polymarket。
    #[test]
    fn factory_creates_polymarket() {
        let cfg = GatewayConfig {
            gateway_type: "polymarket".into(),
            ..GatewayConfig::default()
        };
        let gw = create_gateway(&cfg);
        assert_eq!(gw.gateway_type(), "polymarket");
        assert!(!gw.live_enabled());
    }

    /// Adapter 集成：request_to_order + apply_result。
    #[test]
    fn adapter_integration() {
        let req = OrderRequest::new(
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S",
            "R",
            "O",
        );
        let now = Local::now();
        let mut order = request_to_order(&req, "EX-001", now);
        assert_eq!(order.status, pm_execution::order::OrderStatus::Created);

        let result = GatewayResult::filled("GW-001", 100.0, 0.452, 10);
        apply_result_to_order(&mut order, &result, now);
        assert_eq!(order.status, pm_execution::order::OrderStatus::Filled);
    }

    /// Retry + Breaker 集成。
    #[test]
    fn retry_breaker_integration() {
        let cfg = GatewayConfig::default();
        let executor = RetryExecutor::from_config(&cfg);
        let breaker = executor.breaker();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    /// Sync 集成。
    #[test]
    fn sync_manager_defaults() {
        let mgr = SyncManager::default();
        let now = Local::now();
        assert!(mgr.needs_order_sync(now));
        assert!(mgr.needs_balance_sync(now));
        assert!(mgr.needs_position_sync(now));
    }

    /// Metrics 集成。
    #[test]
    fn metrics_integration() {
        let mut m = GatewayMetrics::new();
        m.record_api_call(50, true);
        m.record_order_submitted();
        m.record_order_filled();
        assert_eq!(m.total_api_calls, 1);
        assert_eq!(m.total_orders_submitted, 1);
        assert_eq!(m.total_orders_filled, 1);
    }

    /// Health 集成。
    #[test]
    fn health_checker_defaults() {
        let checker = HealthChecker::default();
        assert!(checker.needs_check(Local::now()));
    }

    /// Diagnostics 集成（Mock）。
    #[tokio::test]
    async fn diagnostics_integration() {
        let gateway = create_mock_gateway();
        let diag = diagnose_gateway(gateway.as_ref()).await;
        assert!(diag.contains("Gateway 诊断"));
        assert!(diag.contains("MockGateway"));
    }

    /// Config 安全默认。
    #[test]
    fn config_safety_defaults() {
        let cfg = GatewayConfig::default();
        assert!(cfg.is_dry_run());
        assert!(!cfg.enable_live);
    }
}
