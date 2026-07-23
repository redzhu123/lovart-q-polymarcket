//! pm-execution：Execution Engine（V1.06）。
//!
//! 统一订单入口。所有订单必须经过 Execution Pipeline，禁止 Strategy / Risk 直接下单。
//!
//! 绝不连接钱包 / 发送订单 / 签名 / 上链 / Polygon。
//! Gateway 保持 Mock — 仅完成完整架构。
//!
//! 模块：
//! - [`order`]：统一 Order 模型（11 态生命周期 + Direction + StatusChange）。
//! - [`request`]：ExecutionRequest（Strategy → Execution 的输入）。
//! - [`builder`]：OrderBuilder（Request → Order）。
//! - [`validator`]：ExecutionValidator + 8 条内置规则。
//! - [`queue`]：ExecutionQueue（FIFO / Priority / Pause / Resume / Retry）。
//! - [`pipeline`]：ExecutionPipeline（完整管道编排器）。
//! - [`gateway`]：ExecutionGateway trait + MockGateway。
//! - [`scheduler`]：ExecutionScheduler 速率控制。
//! - [`events`]：ExecutionEvent + EventBus。
//! - [`report`]：ExecutionReport 中文报告。
//! - [`metrics`]：ExecutionMetrics 中文指标。
//! - [`config`]：V1.06 执行配置。
//! - [`portfolio_sync`]：Portfolio 同步接口。
//! - [`replay`]：OrderReplay 订单生命周期回放（Phase 3）。
//!
//! - [`state`]：旧 OrderStatus（V0.9，保留兼容）。
//! - [`fill`]：FillEngine（V0.9，MockGateway 内部使用）。
//! - [`engine`]：旧 ExecutionEngine（V0.9，保留兼容）。
//! - [`records`]：CSV 记录（V0.9，保留兼容）。
//! - [`stress`]：execution-test 压测（V0.9，保留兼容）。

// ---- V1.06 新模块 ----
pub mod order;
pub mod request;
pub mod builder;
pub mod validator;
pub mod queue;
pub mod pipeline;
pub mod gateway;
pub mod scheduler;
pub mod events;
pub mod report;
pub mod metrics;
pub mod config;
pub mod portfolio_sync;
pub mod replay;

// ---- V0.9 旧模块（保留兼容）----
pub mod engine;
pub mod fill;
pub mod records;
pub mod state;
pub mod stress;

// ---- V1.06 Phase 1 导出 ----
pub use builder::OrderBuilder;
pub use order::{Direction, Order, OrderStatus, StatusChange};
pub use pipeline::{ExecutionPipeline, ExecutionResult, PipelineStats, PipelineStatus};
pub use queue::{ExecutionQueue, QueueConfig, QueueError, QueueStatus};
pub use request::ExecutionRequest;
pub use validator::{
    CashRule, DuplicateRule, ExecutionValidator, MarketStateRule, PendingLimitRule,
    PositionLimitRule, PriceRule, ProviderRule, QuantityRule, ValidationContext,
    ValidationOutcome, ValidationResult, ValidationRule,
};

// ---- V1.06 Phase 2 导出 ----
pub use config::ExecutionConfigV106;
pub use events::{EventBus, EventRecord, ExecutionEvent, EVENTS_HEADER};
pub use gateway::{ExecutionGateway, GatewayResult, MockGateway};
pub use metrics::ExecutionMetrics;
pub use portfolio_sync::{FillNotification, PortfolioSync};
pub use replay::OrderReplay;
pub use report::ExecutionReport;
pub use scheduler::{ExecutionScheduler, SchedulerConfig, SchedulerStats};

// ---- V0.9 旧导出（保留兼容）----
pub use engine::{
    ExecEvent, ExecParams, ExecPosition, ExecutionEngine, ExecutionOrder, ExecutionStats,
    PortfolioSummary, SubmitOutcome,
};
pub use records::{append_orders, ensure_csv, load_order_base, ExecutionOrderRecord};
pub use state::{OrderStatus as LegacyOrderStatus, TerminalReason};
pub use stress::{run_execution_test, run_execution_test_with_count};

// ---- 测试 ----
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    // ============================================================
    // V1.06 集成测试
    // ============================================================

    #[test]
    fn full_pipeline_submit_and_dequeue() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();
        let req = ExecutionRequest::new(
            "mkt-1", "测试市场?", "mock",
            Direction::Yes, pm_core::Side::Buy,
            0.45, 100.0,
            "S1", "R1", "O1",
        );
        let ctx = ValidationContext::default();
        let result = pipeline.submit(req, now, &ctx);
        assert!(result.is_accepted());

        let next = pipeline.next_order();
        assert!(next.is_some());
        let order = next.unwrap();
        assert_eq!(order.status, OrderStatus::Queued);
        assert_eq!(order.direction, Direction::Yes);
    }

    #[test]
    fn pipeline_rejects_invalid() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();
        let req = ExecutionRequest::new(
            "mkt-1", "Q", "mock",
            Direction::Yes, pm_core::Side::Buy,
            -1.0, 100.0,
            "S", "R", "O",
        );
        let ctx = ValidationContext::default();
        let result = pipeline.submit(req, now, &ctx);
        assert!(!result.is_accepted());
    }

    #[test]
    fn queue_priority_ordering() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();

        let o1 = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, pm_core::Side::Buy,
            0.45, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        );
        let mut o2 = Order::new(
            "EX-002".into(), "C2".into(), "mkt-1".into(), "mock".into(),
            Direction::No, pm_core::Side::Sell,
            0.50, 200.0,
            "S2".into(), "R2".into(), "O2".into(), now,
        );
        o2.priority = 10;
        let o3 = Order::new(
            "EX-003".into(), "C3".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, pm_core::Side::Buy,
            0.55, 150.0,
            "S3".into(), "R3".into(), "O3".into(), now,
        );

        q.enqueue(o1, now).unwrap();
        q.enqueue(o2, now).unwrap();
        q.enqueue(o3, now).unwrap();

        let first = q.dequeue().unwrap();
        assert_eq!(first.order_id, "EX-002"); // priority 10 first
    }

    #[test]
    fn validator_all_rules_default() {
        let v = ExecutionValidator::with_default_rules();
        assert_eq!(v.rule_count(), 8);
    }

    #[test]
    fn report_from_orders() {
        let now = Local::now();
        let mut o = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, pm_core::Side::Buy,
            0.45, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        );
        o.status = OrderStatus::Filled;
        o.filled = 100.0;
        o.remaining = 0.0;

        let report = ExecutionReport::from_orders(&[o]);
        assert_eq!(report.total_orders, 1);
        assert_eq!(report.success_count, 1);
    }

    #[test]
    fn event_bus_integration() {
        let now = Local::now();
        let mut bus = EventBus::new();
        bus.publish(ExecutionEvent::OrderCreated { order_id: "EX-001".into(), timestamp: now });
        bus.publish(ExecutionEvent::OrderFilled {
            order_id: "EX-001".into(),
            avg_price: 0.45,
            slippage: 0.01,
            timestamp: now,
        });
        assert_eq!(bus.len(), 2);

        let mut metrics = ExecutionMetrics::new();
        for event in bus.drain() {
            metrics.record(&event);
        }
        assert_eq!(metrics.total_submitted, 1);
        assert_eq!(metrics.total_filled, 1);
    }

    #[test]
    fn config_bridge_works() {
        let cfg = ExecutionConfigV106::default();
        let qc = cfg.to_queue_config();
        let sc = cfg.to_scheduler_config();
        assert_eq!(qc.max_size, 1000);
        assert_eq!(sc.max_orders_per_second, 10);
        assert_eq!(cfg.gateway, "mock");
    }

    // ============================================================
    // V0.9 旧测试（保留兼容）
    // ============================================================

    #[test]
    fn reject_invalid_price() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_scan());
        assert!(matches!(
            eng.submit_buy("Q", 0.0, now),
            SubmitOutcome::Rejected(TerminalReason::InvalidPrice)
        ));
        assert!(matches!(
            eng.submit_buy("Q", f64::NAN, now),
            SubmitOutcome::Rejected(TerminalReason::InvalidPrice)
        ));
        assert_eq!(eng.stats().total, 2);
        assert_eq!(eng.stats().rejected, 2);
        assert_eq!(eng.pending_count(), 0);
    }

    #[test]
    fn reject_max_pending() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_stress());
        for i in 0..ExecParams::default_for_stress().max_pending_orders {
            let q = format!("Q{}", i);
            assert!(matches!(
                eng.submit_buy(&q, 0.5, now),
                SubmitOutcome::Accepted(_)
            ));
        }
        assert!(matches!(
            eng.submit_buy("Qx", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::MaxPending)
        ));
        assert_eq!(
            eng.pending_count(),
            ExecParams::default_for_stress().max_pending_orders
        );
    }

    #[test]
    fn reject_insufficient_cash() {
        let now = Local::now();
        let p = ExecParams {
            capital: 50.0,
            ..ExecParams::default_for_scan()
        };
        let mut eng = ExecutionEngine::new(p);
        assert!(matches!(
            eng.submit_buy("Q", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::InsufficientCash)
        ));
    }

    #[test]
    fn sell_no_position_rejected() {
        let now = Local::now();
        let mut eng = ExecutionEngine::new(ExecParams::default_for_scan());
        assert!(matches!(
            eng.submit_sell("Ghost", 0.5, now),
            SubmitOutcome::Rejected(TerminalReason::NoPosition)
        ));
    }

    #[test]
    fn buy_cash_invariant() {
        let now = Local::now();
        let p = ExecParams::default_for_scan();
        let initial = p.capital;
        let mut eng = ExecutionEngine::new(p);
        for i in 0..30 {
            let q = format!("Q{}", i);
            let price = 0.2 + (i as f64 % 50.0) / 100.0;
            let _ = eng.submit_buy(&q, price, now);
        }
        assert_eq!(eng.pending_count(), ExecParams::default_for_scan().max_pending_orders);
        assert_eq!(eng.stats().rejected, 10);

        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        assert_eq!(eng.pending_count(), 0);

        let inv = eng.available_cash() + eng.pending_cash() + eng.open_positions_cost();
        assert!(
            (inv - initial).abs() < 1e-6,
            "buy invariant broken: {} vs {}",
            inv,
            initial
        );
        assert_eq!(eng.stats().total, 30);
        let settled =
            eng.stats().filled + eng.stats().cancelled + eng.stats().expired + eng.stats().rejected;
        assert_eq!(settled, eng.stats().total);
    }

    #[test]
    fn sell_closes_position_invariant() {
        let now = Local::now();
        let p = ExecParams::default_for_scan();
        let initial = p.capital;
        let mut eng = ExecutionEngine::new(p);
        for i in 0..15 {
            let q = format!("P{}", i);
            let price = 0.2 + (i as f64 % 50.0) / 100.0;
            let _ = eng.submit_buy(&q, price, now);
        }
        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        for i in 0..15 {
            let q = format!("P{}", i);
            let _ = eng.submit_sell(&q, 0.5, now);
        }
        for _ in 0..(ExecParams::default_for_scan().max_wait_scans + 2) {
            let _ = eng.tick(now);
        }
        assert_eq!(eng.pending_count(), 0);
        assert_eq!(eng.open_position_count(), 0);
        assert!(eng.closed_position_count() > 0);

        let inv = eng.available_cash() + eng.pending_cash() + eng.open_positions_cost();
        let realized = eng.closed_realized_pnl();
        assert!(
            (inv - initial - realized).abs() < 1e-6,
            "sell invariant broken: {} vs {} + {}",
            inv,
            initial,
            realized
        );
    }
}
