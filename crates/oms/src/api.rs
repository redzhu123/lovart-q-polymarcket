//! OMS API（P2-04 第四节）。
//!
//! 业务层（Execution / Strategy / Portfolio / Metrics）通过本模块调用 OMS。
//! Execution 只能调用 OMS；不允许直接调用 Gateway。
//!
//! # API 列表
//!
//! - [`Oms::create_order`] — 创建订单。
//! - [`Oms::submit_order`] — 校验 + 提交订单到 Gateway。
//! - [`Oms::cancel_order`] — 取消订单。
//! - [`Oms::replace_order`] — 替换订单。
//! - [`Oms::get_order`] — 按 order_id 查询。
//! - [`Oms::get_order_by_client_id`] — 按 client_order_id 查询。
//! - [`Oms::list_orders`] — 列出全部。
//! - [`Oms::list_active`] — 列出活跃订单。
//! - [`Oms::sync_order`] — 同步单个订单状态。
//! - [`Oms::recover`] — 启动恢复所有订单。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use std::sync::Arc;

use chrono::{DateTime, Local};
use pm_gateway::{Balance, ExchangeGateway, GatewayResult};

use crate::events::{EventBus, Subscriber};
use crate::lifecycle::{CreateOrderInput, Lifecycle, LifecycleContext};
use crate::matcher::{MatchDecision, MatchResult, Matcher};
use crate::metrics::{OmsMetrics, OmsMetricsSubscriber};
use crate::order::Order;
use crate::recovery::{Recovery, RecoveryReport, SyncReport, sync_order};
use crate::repository::OrderRepository;
use crate::state_machine::StateMachine;
use crate::validator::{ValidationContext, ValidationResult, Validator};

// ============================================================================
// OmsConfig — OMS 配置
// ============================================================================

/// OMS 配置（用于 CLI / 测试构造）。
#[derive(Debug, Clone)]
pub struct OmsConfig {
    /// Repository 类型（默认 Memory）。
    pub repository_type: crate::repository::RepositoryType,
    /// orders.csv 路径。
    pub orders_csv: Option<std::path::PathBuf>,
    /// events.csv 路径。
    pub events_csv: Option<std::path::PathBuf>,
    /// SQLite 路径（仅 Sqlite 类型使用）。
    pub sqlite_path: Option<std::path::PathBuf>,
    /// 启动时是否自动恢复。
    pub auto_recover: bool,
    /// 启动时是否订阅默认 Subscriber（OmsMetrics）。
    pub subscribe_metrics: bool,
}

impl Default for OmsConfig {
    fn default() -> Self {
        Self {
            repository_type: crate::repository::RepositoryType::Memory,
            orders_csv: None,
            events_csv: None,
            sqlite_path: None,
            auto_recover: true,
            subscribe_metrics: true,
        }
    }
}

// ============================================================================
// Oms — 顶层 OMS 对象
// ============================================================================

/// OMS 顶层对象：统一管理 Repository / EventBus / StateMachine / Validator / Gateway / Metrics。
pub struct Oms {
    config: OmsConfig,
    /// 订单仓库（trait 对象）。
    repository: Box<dyn OrderRepository>,
    /// 事件总线。
    pub event_bus: EventBus,
    /// 状态机。
    pub state_machine: StateMachine,
    /// 校验器。
    pub validator: Validator,
    /// Gateway（trait 对象）。
    pub gateway: Arc<dyn ExchangeGateway>,
    /// Metrics（共享 sink，可外部读取）。
    pub metrics: Arc<std::sync::Mutex<OmsMetrics>>,
}

impl Oms {
    /// 创建新 OMS 实例。
    pub fn new(config: OmsConfig, gateway: Arc<dyn ExchangeGateway>) -> anyhow::Result<Self> {
        let repository = crate::repository::create_repository(
            config.repository_type,
            config.orders_csv.clone(),
            config.events_csv.clone(),
            config.sqlite_path.clone(),
        )?;
        let event_bus = EventBus::new();
        let metrics = Arc::new(std::sync::Mutex::new(OmsMetrics::new()));

        let mut oms = Self {
            config,
            repository,
            event_bus,
            state_machine: StateMachine::new(),
            validator: Validator::with_default_rules(),
            gateway,
            metrics,
        };

        if oms.config.subscribe_metrics {
            oms.event_bus
                .subscribe(Box::new(OmsMetricsSubscriber::new(oms.metrics.clone())));
        }

        Ok(oms)
    }

    /// 注册自定义订阅者。
    pub fn subscribe(&mut self, sub: Box<dyn Subscriber>) {
        self.event_bus.subscribe(sub);
    }

    /// 获取 metrics snapshot（克隆）。
    pub fn metrics_snapshot(&self) -> OmsMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// 获取 repository（trait 对象引用）。
    pub fn repository(&self) -> &dyn OrderRepository {
        self.repository.as_ref()
    }

    /// 获取 gateway 引用。
    pub fn gateway(&self) -> &dyn ExchangeGateway {
        self.gateway.as_ref()
    }

    /// 创建 LifecycleContext（内部辅助）。
    fn lctx(&self) -> LifecycleContext<'_> {
        LifecycleContext::new(
            self.repository.as_ref(),
            &self.event_bus,
            &self.state_machine,
            &self.validator,
        )
    }

    // ---- API：订单 CRUD ----

    /// 创建订单（不提交）。
    pub fn create_order(
        &self,
        input: &CreateOrderInput,
        now: DateTime<Local>,
    ) -> anyhow::Result<Order> {
        Lifecycle::create_order(input, &self.lctx(), now)
    }

    /// 校验订单。
    pub fn validate_order(
        &self,
        order: &mut Order,
        vctx: &ValidationContext,
        now: DateTime<Local>,
    ) -> anyhow::Result<ValidationResult> {
        Lifecycle::validate_order(order, vctx, &self.lctx(), now)
    }

    /// 提交订单到 Gateway。
    pub async fn submit_order(
        &self,
        order: &mut Order,
        now: DateTime<Local>,
    ) -> anyhow::Result<GatewayResult> {
        Lifecycle::submit_order(order, self.gateway.as_ref(), &self.lctx(), now).await
    }

    /// 取消订单。
    pub async fn cancel_order(
        &self,
        order: &mut Order,
        reason: &str,
        now: DateTime<Local>,
    ) -> anyhow::Result<GatewayResult> {
        Lifecycle::cancel_order(order, reason, self.gateway.as_ref(), &self.lctx(), now).await
    }

    /// 替换订单。
    pub async fn replace_order(
        &self,
        old: &mut Order,
        new_input: &CreateOrderInput,
        now: DateTime<Local>,
    ) -> anyhow::Result<Order> {
        Lifecycle::replace_order(old, new_input, self.gateway.as_ref(), &self.lctx(), now).await
    }

    /// 按 order_id 查询。
    pub fn get_order(&self, order_id: &str) -> anyhow::Result<Option<Order>> {
        self.repository.find_by_id(order_id)
    }

    /// 按 client_order_id 查询。
    pub fn get_order_by_client_id(&self, client_order_id: &str) -> anyhow::Result<Option<Order>> {
        self.repository.find_by_client_id(client_order_id)
    }

    /// 列出全部订单。
    pub fn list_orders(&self) -> anyhow::Result<Vec<Order>> {
        self.repository.list_all()
    }

    /// 列出活跃订单。
    pub fn list_active(&self) -> anyhow::Result<Vec<Order>> {
        self.repository.list_active()
    }

    /// 同步单个订单（公开 API）。
    pub async fn sync_order(&self, order: &mut Order) -> Result<SyncReport, String> {
        sync_order(
            order,
            self.gateway.as_ref(),
            self.repository.as_ref(),
            &self.event_bus,
            &self.state_machine,
        )
        .await
    }

    /// 启动恢复。
    pub async fn recover(&self) -> RecoveryReport {
        Recovery::run(
            self.repository.as_ref(),
            &self.event_bus,
            &self.state_machine,
            self.gateway.as_ref(),
        )
        .await
    }

    /// 是否自动恢复（创建时已确定）。
    pub fn auto_recover_enabled(&self) -> bool {
        self.config.auto_recover
    }

    // ---- API：辅助 ----

    /// 撮合评估（仅决策建议，不修改订单）。
    pub fn evaluate_match(
        &self,
        order: &Order,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
    ) -> MatchResult {
        Matcher::evaluate(order, best_bid, best_ask)
    }

    /// 撮合决策（中文）。
    pub fn evaluate_match_decision(
        &self,
        order: &Order,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
    ) -> MatchDecision {
        self.evaluate_match(order, best_bid, best_ask).decision
    }

    /// 自动构造 ValidationContext（含当前余额 + 活跃订单数）。
    pub async fn build_validation_context(&self) -> anyhow::Result<ValidationContext> {
        let balance = self.gateway.get_balance().await.ok();
        let active_count = self.list_active()?.len();
        Ok(ValidationContext {
            balance,
            market_open: true,
            active_order_count: active_count,
            max_active_orders: 100,
            now: Local::now(),
        })
    }

    /// 健康检查汇总（中文）。
    pub async fn health(&self) -> String {
        let repo_health = self.repository.health();
        let balance = self.gateway.get_balance().await.ok();
        let gateway_health = self.gateway.health().await;
        let balance_str = balance
            .map(|b| format!("{:.2} {}", b.available, b.currency))
            .unwrap_or_else(|| "(查询失败)".into());
        format!(
            "【OMS 健康检查】\n\
             Repository: {}\n\
             Gateway  : {} ({})\n\
             模式     : {}\n\
             余额     : {}\n\
             订阅者   : {} 个\n\
             已发布事件: {}",
            repo_health.summary_zh(),
            self.gateway.name(),
            self.gateway.gateway_type(),
            if self.gateway.live_enabled() {
                "⚠️ 真实交易"
            } else {
                "🔒 模拟交易"
            },
            balance_str,
            self.event_bus.subscriber_count(),
            self.event_bus.published_count(),
        )
    }

    /// 仅模拟余额构造 helper（仅测试 / Mock Gateway 使用）。
    pub fn mock_balance(&self, available: f64) -> Balance {
        Balance::mock(available)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::CreateOrderInput;
    use crate::order::OrderStatus;
    use pm_core::Side;
    use pm_execution::order::Direction;
    use pm_gateway::{OrderType, TimeInForce, create_mock_gateway};

    fn build_oms() -> Oms {
        let cfg = OmsConfig::default();
        let gw = create_mock_gateway();
        Oms::new(cfg, Arc::from(gw)).unwrap()
    }

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
    fn oms_creation_default() {
        let oms = build_oms();
        assert_eq!(oms.event_bus.subscriber_count(), 1); // metrics
        assert_eq!(oms.event_bus.published_count(), 0);
    }

    #[test]
    fn create_and_get_order() {
        let oms = build_oms();
        let o = oms.create_order(&base_input(), Local::now()).unwrap();
        assert_eq!(o.status, OrderStatus::Created);
        let found = oms.get_order(&o.order_id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn list_orders_returns_created() {
        let oms = build_oms();
        oms.create_order(&base_input(), Local::now()).unwrap();
        let list = oms.list_orders().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn metrics_subscribed_records_events() {
        let oms = build_oms();
        oms.create_order(&base_input(), Local::now()).unwrap();
        let m = oms.metrics_snapshot();
        assert_eq!(m.total_created, 1);
    }

    #[tokio::test]
    async fn validate_then_submit_flow() {
        let oms = build_oms();
        let mut order = oms.create_order(&base_input(), Local::now()).unwrap();
        let mut vctx = ValidationContext::minimal();
        vctx.balance = Some(Balance::mock(10_000.0));
        let result = oms.validate_order(&mut order, &vctx, Local::now()).unwrap();
        assert!(result.all_passed);
        let gr = oms.submit_order(&mut order, Local::now()).await.unwrap();
        assert!(gr.success);
    }

    #[tokio::test]
    async fn cancel_and_replace() {
        let oms = build_oms();
        let mut order = oms.create_order(&base_input(), Local::now()).unwrap();
        let _ = oms
            .cancel_order(&mut order, "用户取消", Local::now())
            .await
            .unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);

        let mut new_input = base_input();
        new_input.client_order_id = "CLI-NEW".into();
        let mut order2 = oms.create_order(&new_input, Local::now()).unwrap();
        let _ = oms.submit_order(&mut order2, Local::now()).await.unwrap();
        // order2 在 replace 时是活跃的（Accepted/PartiallyFilled）
        let before_status = order2.status;
        assert!(before_status.is_active() || before_status.is_terminal());

        let mut new_input2 = base_input();
        new_input2.client_order_id = "CLI-NEW2".into();
        let new3 = oms
            .replace_order(&mut order2, &new_input2, Local::now())
            .await
            .unwrap();
        assert_ne!(new3.order_id, order.order_id);
    }

    #[tokio::test]
    async fn sync_order_no_exchange_id_noop() {
        let oms = build_oms();
        let mut order = oms.create_order(&base_input(), Local::now()).unwrap();
        let r = oms.sync_order(&mut order).await.unwrap();
        assert!(!r.status_changed);
    }

    #[tokio::test]
    async fn recover_with_no_pending() {
        let oms = build_oms();
        let report = oms.recover().await;
        assert_eq!(report.pending_recovery, 0);
    }

    #[tokio::test]
    async fn health_summary_chinese() {
        let oms = build_oms();
        let h = oms.health().await;
        assert!(h.contains("OMS 健康检查"));
        assert!(h.contains("Repository"));
    }

    #[tokio::test]
    async fn evaluate_match_passthrough() {
        let oms = build_oms();
        let order = oms.create_order(&base_input(), Local::now()).unwrap();
        let decision = oms.evaluate_match_decision(&order, Some(0.43), Some(0.46));
        assert_eq!(decision, MatchDecision::Allow);
    }
}
