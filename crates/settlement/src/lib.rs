//! pm-settlement：Settlement Engine（成交结算引擎 — P2-06）。
//!
//! 企业级成交结算引擎，是成交事件唯一处理中心。
//!
//! # 架构
//!
//! ```text
//! Trade Fill Event
//!       │
//!       ▼
//! ┌──────────────────────────┐
//! │  Settlement Engine        │
//! │  ┌────────────────────┐  │
//! │  │ process_fill()     │  │  ← 唯一入口
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Validator (7+)    │  │  校验成交合法性
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Fee Engine        │  │  统一手续费计算
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Position Manager  │  │  持仓更新
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Balance Manager   │  │  余额更新
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  PnL Engine        │  │  盈亏结算
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Ledger            │  │  资金流水
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  EventBus          │──┼──► PMS / Metrics / Audit
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Repository        │  │  Memory / CSV / SQLite
//! │  └────────────────────┘  │
//! └──────────────────────────┘
//!       │
//!       ▼
//! Settlement Result
//!       │
//!       ▼
//! Ledger → Portfolio → PnL → Metrics → Audit
//! ```
//!
//! # 模块
//!
//! - [`types`]：统一领域类型（TradeFillEvent / SettlementResult / LedgerEntry / FeeBreakdown）
//! - [`engine`]：SettlementEngine — process_fill() 统一入口
//! - [`fee`]：FeeEngine — Maker / Taker / Trading / Settlement 手续费
//! - [`position`]：PositionManager — 持仓开平加减
//! - [`balance`]：BalanceManager — 余额冻结/扣款/入账
//! - [`pnl`]：PnLEngine — 已实现/未实现盈亏
//! - [`ledger`]：Ledger — 追加不可修改资金流水
//! - [`validator`]：SettlementValidator — 7 条内置校验规则
//! - [`repository`]：SettlementRepository trait + Memory / CSV 实现
//! - [`metrics`]：SettlementMetrics — 指标收集
//! - [`events`]：SettlementEvent + EventBus
//!
//! # 业务约束
//!
//! - 禁止 OMS 修改资金。
//! - 禁止 PMS 直接处理成交。
//! - 禁止 Gateway 更新持仓。
//! - 所有成交必须经过 Settlement Engine。
//! - 所有日志使用 tracing，中文输出。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不上链。

pub mod balance;
pub mod engine;
pub mod events;
pub mod fee;
pub mod ledger;
pub mod metrics;
pub mod pnl;
pub mod position;
pub mod repository;
pub mod types;
pub mod validator;

use engine::{SettlementConfig, SettlementEngine};

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::balance::BalanceManager;
    pub use crate::create_csv_settlement;
    pub use crate::create_default_settlement;
    pub use crate::engine::{SettlementConfig, SettlementEngine};
    pub use crate::events::{SettlementEvent, SettlementEventBus, SettlementSubscriber};
    pub use crate::fee::FeeEngine;
    pub use crate::ledger::Ledger;
    pub use crate::metrics::{MetricsSnapshot, SettlementMetrics};
    pub use crate::pnl::{PnLEngine, PnLReport};
    pub use crate::position::PositionManager;
    pub use crate::repository::{
        InMemoryRepository, RepositoryType, SettlementRepository, create_repository,
    };
    pub use crate::types::{
        BalanceState, FeeBreakdown, FeeRule, LedgerDirection, LedgerEntry, PositionState,
        SettlementResult, SettlementStatus, TradeFillEvent,
    };
    pub use crate::validator::{SettlementValidator, ValidationOutcome, ValidationResult};
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建默认 Settlement Engine（Memory 仓库 + 零手续费）。
pub fn create_default_settlement() -> anyhow::Result<SettlementEngine> {
    let config = SettlementConfig::default();
    let repo = repository::create_repository(repository::RepositoryType::Memory, None, None, None)?;
    SettlementEngine::new(config, repo)
}

/// 创建带 CSV 持久化的 Settlement Engine。
pub fn create_csv_settlement(
    fills_csv: std::path::PathBuf,
    settlements_csv: std::path::PathBuf,
    ledger_csv: std::path::PathBuf,
) -> anyhow::Result<SettlementEngine> {
    let config = SettlementConfig::default();
    let repo = repository::create_repository(
        repository::RepositoryType::Csv,
        Some(fills_csv),
        Some(settlements_csv),
        Some(ledger_csv),
    )?;
    SettlementEngine::new(config, repo)
}

// ============================================================================
// 中文 tracing 初始化
// ============================================================================

/// 初始化 Settlement Engine 中文 tracing。
pub fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_env("PM_SETTLEMENT_LOG").unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Direction, FeeBreakdown, FeeRule, LedgerDirection, SettlementStatus, TradeFillEvent,
    };
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn prelude_exports_compile() {
        let _event = TradeFillEvent {
            trade_id: "T-1".into(),
            order_id: "O-1".into(),
            client_order_id: "C-1".into(),
            exchange_order_id: None,
            market_id: "M-1".into(),
            account_id: "A-1".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: false,
            gateway_name: "Mock".into(),
        };
        let _fee = FeeBreakdown::zero();
        let _rule = FeeRule::default();
        let _status = SettlementStatus::Success;
        let _ledger_dir = LedgerDirection::Debit;
    }

    #[test]
    fn default_factory_works() {
        let engine = create_default_settlement().unwrap();
        let bal = engine.balance_mgr.get("ACCT-MAIN-001").unwrap();
        assert!(approx(bal.available, 10_000.0));
        assert_eq!(engine.position_mgr.open_count(), 0);
        assert_eq!(engine.ledger.count(), 0);
    }

    #[test]
    fn end_to_end_buy_then_sell() {
        let mut engine = create_default_settlement().unwrap();

        // 买入
        let buy = TradeFillEvent {
            trade_id: "T-BUY-001".into(),
            order_id: "OMS-BUY-001".into(),
            client_order_id: "CLI-BUY-001".into(),
            exchange_order_id: None,
            market_id: "mkt-test".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 200.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        };
        let r1 = engine.process_fill(&buy);
        assert!(r1.status.is_success());
        assert!(approx(r1.balance_after, 9900.0)); // 10000 - 100

        // 卖出（平仓）
        let sell = TradeFillEvent {
            trade_id: "T-SELL-001".into(),
            order_id: "OMS-SELL-001".into(),
            client_order_id: "CLI-SELL-001".into(),
            exchange_order_id: None,
            market_id: "mkt-test".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side: Side::Sell,
            fill_price: 0.60,
            fill_quantity: 200.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        };
        let r2 = engine.process_fill(&sell);
        assert!(r2.status.is_success());
        assert!(approx(r2.realized_pnl, 20.0)); // 200 * (0.60 - 0.50)
        assert!(approx(r2.balance_after, 10020.0)); // 10000 + 20

        // 验证持仓已平仓
        assert_eq!(engine.position_mgr.open_count(), 0);
        assert_eq!(engine.position_mgr.closed_count(), 1);

        // 验证流水
        assert!(engine.ledger.count() >= 2);
    }

    #[test]
    fn validation_rejects_invalid_orders() {
        let mut engine = create_default_settlement().unwrap();

        // 价格异常
        let bad = TradeFillEvent {
            trade_id: "T-BAD-001".into(),
            order_id: "OMS-BAD-001".into(),
            client_order_id: "CLI-BAD-001".into(),
            exchange_order_id: None,
            market_id: "mkt-test".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 1.5, // > 1.0
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        };
        let r = engine.process_fill(&bad);
        assert!(!r.status.is_success());
        assert_eq!(r.status, SettlementStatus::ValidationFailed);
    }

    #[test]
    fn csv_factory_with_temp_paths() {
        let dir = std::env::temp_dir();
        let engine = create_csv_settlement(
            dir.join("test_settle_fills.csv"),
            dir.join("test_settle_results.csv"),
            dir.join("test_settle_ledger.csv"),
        )
        .unwrap();
        assert_eq!(engine.position_mgr.open_count(), 0);
    }

    #[test]
    fn metrics_track_failures() {
        let mut engine = create_default_settlement().unwrap();

        let bad = TradeFillEvent {
            trade_id: "T-BAD-001".into(),
            order_id: "OMS-BAD-001".into(),
            client_order_id: "CLI-BAD-001".into(),
            exchange_order_id: None,
            market_id: "mkt-test".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: -1.0,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        };
        engine.process_fill(&bad);
        let snap = engine.metrics.snapshot();
        assert_eq!(snap.failed_settlements, 1);
    }

    #[test]
    fn event_bus_fires_on_fill() {
        use events::SettlementSubscriber;
        use std::sync::{Arc, Mutex};

        let mut engine = create_default_settlement().unwrap();
        let sink = Arc::new(Mutex::new(Vec::new()));

        struct TestSub {
            name: String,
            sink: Arc<Mutex<Vec<String>>>,
        }
        impl SettlementSubscriber for TestSub {
            fn name(&self) -> &str {
                &self.name
            }
            fn on_event(&self, event: &events::SettlementEvent) -> anyhow::Result<()> {
                self.sink
                    .lock()
                    .unwrap()
                    .push(event.event_name_zh().to_string());
                Ok(())
            }
        }

        engine.event_bus.subscribe(Box::new(TestSub {
            name: "test".into(),
            sink: sink.clone(),
        }));

        let fill = TradeFillEvent {
            trade_id: "T-EVT-001".into(),
            order_id: "OMS-EVT-001".into(),
            client_order_id: "CLI-EVT-001".into(),
            exchange_order_id: None,
            market_id: "mkt-test".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: false,
            gateway_name: "Mock".into(),
        };
        engine.process_fill(&fill);

        let events = sink.lock().unwrap();
        assert!(events.len() >= 4); // FillReceived, ValidationPassed, FeeCalculated, PositionUpdated, BalanceUpdated, PnLUpdated, LedgerRecorded, SettlementCompleted
        assert!(events.contains(&"接收成交".to_string()));
        assert!(events.contains(&"结算完成".to_string()));
    }
}
