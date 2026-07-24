//! pm-oms：Order Management System（P2-04）。
//!
//! 企业级 OMS，作为 Execution 与 Gateway 之间唯一的订单管理层。
//!
//! # 架构
//!
//! ```text
//! Execution (V1.06)
//!       │
//!       ▼
//! ┌──────────────────────────┐
//! │   OMS  (P2-04)           │
//! │  ┌────────────────────┐  │
//! │  │ Oms / Lifecycle    │  │
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Validator (5+)    │  │
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  State Machine     │  │
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  EventBus          │──┼──► Portfolio / Metrics / Audit
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Repository        │  │   Memory / CSV / SQLite
//! │  └─────────┬──────────┘  │
//! └────────────│─────────────┘
//!              │
//!              ▼
//! ┌──────────────────────────┐
//! │  Gateway (P2-03)         │
//! └──────────────────────────┘
//! ```
//!
//! # 模块
//!
//! - [`order`]：统一 Order 领域模型（11 态生命周期）。
//! - [`state_machine`]：状态机 — 所有合法转移白名单。
//! - [`lifecycle`]：Lifecycle — create / validate / submit / cancel / replace。
//! - [`validator`]：9 条内置校验规则。
//! - [`events`]：OrderEvent + EventBus。
//! - [`repository`]：OrderRepository trait + Memory / CSV / SQLite 实现。
//! - [`recovery`]：启动恢复 + 单订单同步。
//! - [`matcher`]：价格偏离评估。
//! - [`metrics`]：OmsMetrics + Subscriber。
//! - [`api`]：Oms 顶层 API。
//!
//! # 业务约束
//!
//! - 禁止自动交易 / 真实资金 / Wallet / 签名。
//! - 所有日志使用 tracing，中文输出。
//! - 业务层禁止直接调用 Gateway，必须经 OMS。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不上链。

pub mod api;
pub mod events;
pub mod lifecycle;
pub mod matcher;
pub mod metrics;
pub mod order;
pub mod recovery;
pub mod repository;
pub mod state_machine;
pub mod validator;

use crate::api::{Oms, OmsConfig};

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::api::{Oms, OmsConfig};
    pub use crate::create_csv_oms;
    pub use crate::create_default_oms;
    pub use crate::events::{
        EventBus, OMS_EVENTS_HEADER, OrderEvent, Subscriber, event_to_csv_row,
    };
    pub use crate::lifecycle::{CreateOrderInput, Lifecycle, LifecycleContext};
    pub use crate::matcher::{MatchDecision, MatchResult, Matcher};
    pub use crate::metrics::{OmsMetrics, OmsMetricsSubscriber};
    pub use crate::order::{Order, OrderStatus, StatusChange};
    pub use crate::recovery::{Recovery, RecoveryReport, SyncReport, sync_order};
    pub use crate::repository::csv::CsvRepository;
    pub use crate::repository::memory::InMemoryRepository;
    pub use crate::repository::sqlite::SqliteRepository;
    pub use crate::repository::{
        OrderRepository, RepositoryHealth, RepositoryType, create_repository,
    };
    pub use crate::state_machine::{StateMachine, StateTransition, TransitionError};
    pub use crate::validator::{
        ValidationContext, ValidationOutcome, ValidationResult, ValidationRule, Validator,
        ValidatorConfig,
    };
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建默认 OMS（Memory 仓库 + Mock Gateway）。
pub fn create_default_oms() -> anyhow::Result<Oms> {
    use std::sync::Arc;
    let cfg = OmsConfig::default();
    let gateway = pm_gateway::create_mock_gateway();
    Oms::new(cfg, Arc::from(gateway))
}

/// 创建带 CSV 持久化的 OMS。
pub fn create_csv_oms(
    orders_csv: std::path::PathBuf,
    events_csv: std::path::PathBuf,
) -> anyhow::Result<Oms> {
    use std::sync::Arc;
    let cfg = OmsConfig {
        repository_type: crate::repository::RepositoryType::Csv,
        orders_csv: Some(orders_csv),
        events_csv: Some(events_csv),
        sqlite_path: None,
        auto_recover: true,
        subscribe_metrics: true,
    };
    let gateway = pm_gateway::create_mock_gateway();
    Oms::new(cfg, Arc::from(gateway))
}

// ============================================================================
// 中文 tracing 初始化
// ============================================================================

/// 初始化 OMS 中文 tracing。
pub fn init_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("PM_OMS_LOG").unwrap_or_else(|_| EnvFilter::new(level));
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
    use crate::lifecycle::CreateOrderInput;
    use crate::order::OrderStatus;
    use chrono::Local;
    use pm_core::Side;
    use pm_execution::order::Direction;
    use pm_gateway::Balance;

    fn base_input() -> CreateOrderInput {
        CreateOrderInput::limit(
            "CLI-001",
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S1",
            "R1",
            "O1",
        )
    }

    #[test]
    fn prelude_exports_compile() {
        // 验证 prelude 全部导出可用
        let _state: OrderStatus = OrderStatus::Created;
        let _status_zh = OrderStatus::Created.as_zh();
        let _ = crate::state_machine::StateMachine::new();
        let _ = crate::validator::Validator::with_default_rules();
    }

    #[test]
    fn default_factory_works() {
        let oms = create_default_oms().unwrap();
        let list = oms.list_orders().unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn end_to_end_create_validate_cancel() {
        let oms = create_default_oms().unwrap();
        let mut order = oms.create_order(&base_input(), Local::now()).unwrap();
        assert_eq!(order.status, OrderStatus::Created);

        let mut vctx = crate::validator::ValidationContext::minimal();
        vctx.balance = Some(Balance::mock(10_000.0));
        let r = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
        assert!(r.all_passed);
        assert_eq!(order.status, OrderStatus::Validated);

        let _ = oms
            .cancel_order(&mut order, "测试取消", Local::now())
            .await
            .unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    #[test]
    fn state_machine_diagram_chinese() {
        let d = crate::state_machine::StateMachine::diagram_zh();
        assert!(d.contains("OMS"));
        assert!(d.contains("已创建"));
    }

    #[test]
    fn validator_has_default_rules() {
        let v = crate::validator::Validator::with_default_rules();
        assert!(v.rule_count() >= 9);
    }

    #[test]
    fn event_bus_default_subscribers() {
        let bus = crate::events::EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn oms_metrics_integration() {
        let oms = create_default_oms().unwrap();
        oms.create_order(&base_input(), Local::now()).unwrap();
        let m = oms.metrics_snapshot();
        assert_eq!(m.total_created, 1);
    }
}
