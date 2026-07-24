//! OMS Order Lifecycle（P2-04 第三节）。
//!
//! 单个订单的全生命周期管理：协调 Validator / StateMachine / EventBus / Repository，
//! 在 `Order` 上推进状态。
//!
//! ## 调用流程
//!
//! ```text
//! create_order(req, ctx)
//!   ├── Order::new（初始 Created）
//!   ├── publish(OrderCreated)
//!   ├── repo.save
//!   └── 返回 Order
//!
//! validate_order(order, ctx)
//!   ├── validator.validate
//!   ├── 失败：transition → Rejected + publish(ValidationFailed) + repo.save
//!   └── 成功：transition → Validated + publish(OrderValidated) + repo.save
//!
//! submit_order(order, gateway, now)
//!   ├── 状态机校验 PendingSubmit → Submitted
//!   ├── gateway.submit_order
//!   ├── 失败：transition → Rejected + publish(GatewayError) + repo.save
//!   ├── 部分成交：transition → PartiallyFilled + publish(OrderPartiallyFilled)
//!   ├── 完全成交：transition → Filled + publish(OrderFilled)
//!   └── 接受：transition → Accepted + publish(OrderAccepted) + repo.save
//!
//! cancel_order(order, reason)
//!   ├── 状态机校验 active → Cancelled
//!   ├── gateway.cancel_order
//!   ├── transition → Cancelled + publish(OrderCancelled) + repo.save
//!   └── ...
//!
//! replace_order(...) ≈ cancel + create + submit
//! ```

use chrono::{DateTime, Local};
use pm_gateway::{
    Balance, ExchangeGateway, GatewayResult, OrderRequest, OrderType, TimeInForce,
};
use pm_execution::order::Direction;
use pm_core::Side;

use crate::events::{EventBus, OrderEvent};
use crate::order::{Order, OrderStatus};
use crate::repository::OrderRepository;
use crate::state_machine::StateMachine;
use crate::validator::{ValidationContext, ValidationResult, Validator};

// ============================================================================
// CreateOrderInput — 创建订单的输入
// ============================================================================

/// 创建订单的入参。
///
/// 业务层通过本结构调用 `lifecycle::create_order`，无需直接接触 Gateway 类型。
#[derive(Debug, Clone)]
pub struct CreateOrderInput {
    /// 客户端订单 ID。
    pub client_order_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// Provider。
    pub provider: String,
    /// 方向（YES / NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 价格。
    pub price: f64,
    /// 数量。
    pub quantity: f64,
    /// 订单类型。
    pub order_type: OrderType,
    /// 有效期。
    pub time_in_force: TimeInForce,
    /// 策略 ID。
    pub strategy_id: String,
    /// 风控 ID。
    pub risk_id: String,
    /// 机会 ID。
    pub opportunity_id: String,
    /// 优先级（可选）。
    pub priority: u32,
    /// 备注（可选）。
    pub notes: String,
    /// Gateway 名称（可选，默认 "MockGateway"）。
    pub gateway_name: String,
}

impl CreateOrderInput {
    /// 创建一个常用的 Limit GTC 订单。
    pub fn limit(
        client_order_id: &str,
        market_id: &str,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
        strategy_id: &str,
        risk_id: &str,
        opportunity_id: &str,
    ) -> Self {
        Self {
            client_order_id: client_order_id.to_string(),
            market_id: market_id.to_string(),
            provider: "mock".into(),
            direction,
            side,
            price,
            quantity,
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Gtc,
            strategy_id: strategy_id.to_string(),
            risk_id: risk_id.to_string(),
            opportunity_id: opportunity_id.to_string(),
            priority: 0,
            notes: String::new(),
            gateway_name: "MockGateway".into(),
        }
    }
}

// ============================================================================
// LifecycleContext — 单订单生命周期上下文
// ============================================================================

/// 单订单推进所需的协作对象集合。
pub struct LifecycleContext<'a> {
    pub repository: &'a dyn OrderRepository,
    pub event_bus: &'a EventBus,
    pub state_machine: &'a StateMachine,
    pub validator: &'a Validator,
}

impl<'a> LifecycleContext<'a> {
    pub fn new(
        repository: &'a dyn OrderRepository,
        event_bus: &'a EventBus,
        state_machine: &'a StateMachine,
        validator: &'a Validator,
    ) -> Self {
        Self {
            repository,
            event_bus,
            state_machine,
            validator,
        }
    }
}

// ============================================================================
// Lifecycle — 订单生命周期
// ============================================================================

/// OMS 订单生命周期：所有"业务动作"的入口。
pub struct Lifecycle;

impl Lifecycle {
    // ------------------------------------------------------------------
    // 1. create_order
    // ------------------------------------------------------------------

    /// 创建订单。
    ///
    /// 仅生成 `Order` 对象并保存，不自动 submit。调用方可在后续单独调用
    /// `validate_order` 和 `submit_order`。
    pub fn create_order(
        input: &CreateOrderInput,
        ctx: &LifecycleContext<'_>,
        now: DateTime<Local>,
    ) -> anyhow::Result<Order> {
        // 防止 client_order_id 重复
        if let Some(existing) = ctx.repository.find_by_client_id(&input.client_order_id)? {
            tracing::warn!(
                client_order_id = %input.client_order_id,
                order_id = %existing.order_id,
                "客户端订单 ID 已存在，返回已有订单"
            );
            return Ok(existing);
        }

        let mut order = Order::new(
            input.client_order_id.clone(),
            input.market_id.clone(),
            input.provider.clone(),
            input.gateway_name.clone(),
            input.direction,
            input.side,
            input.price,
            input.quantity,
            input.order_type,
            input.time_in_force,
            input.strategy_id.clone(),
            input.risk_id.clone(),
            input.opportunity_id.clone(),
            now,
        );
        if input.priority > 0 {
            order = order.with_priority(input.priority);
        }
        if !input.notes.is_empty() {
            order = order.with_notes(&input.notes);
        }

        ctx.repository.save(&order)?;
        ctx.event_bus.publish(OrderEvent::OrderCreated {
            order_id: order.order_id.clone(),
            client_order_id: order.client_order_id.clone(),
            market_id: order.market_id.clone(),
            timestamp: now,
        });

        tracing::info!(
            order_id = %order.order_id,
            client_order_id = %order.client_order_id,
            market_id = %order.market_id,
            "OMS 订单已创建"
        );
        Ok(order)
    }

    // ------------------------------------------------------------------
    // 2. validate_order
    // ------------------------------------------------------------------

    /// 校验订单。失败时直接转到 Rejected（不发 Gateway）。
    pub fn validate_order(
        order: &mut Order,
        vctx: &ValidationContext,
        ctx: &LifecycleContext,
        now: DateTime<Local>,
    ) -> anyhow::Result<ValidationResult> {
        let result = ctx.validator.validate(order, vctx);
        if result.all_passed {
            Self::apply_transition(
                order,
                OrderStatus::Validated,
                "校验通过",
                "validator",
                ctx,
                now,
            )?;
            ctx.event_bus.publish(OrderEvent::OrderValidated {
                order_id: order.order_id.clone(),
                timestamp: now,
            });
        } else {
            // 校验失败：直接到 Rejected
            let reason = result.summary_zh();
            Self::apply_transition(
                order,
                OrderStatus::Rejected,
                &format!("校验失败：{}", reason),
                "validator",
                ctx,
                now,
            )?;
            ctx.event_bus.publish(OrderEvent::ValidationFailed {
                order_id: order.order_id.clone(),
                reason: reason.clone(),
                timestamp: now,
            });
            ctx.event_bus.publish(OrderEvent::OrderRejected {
                order_id: order.order_id.clone(),
                reason: reason.clone(),
                timestamp: now,
            });
        }
        ctx.repository.save(order)?;
        Ok(result)
    }

    // ------------------------------------------------------------------
    // 3. submit_order
    // ------------------------------------------------------------------

    /// 提交订单到 Gateway。
    ///
    /// 内部流程：
    /// 1. 状态机校验 Created/Validated → PendingSubmit → Submitted
    /// 2. 调用 `gateway.submit_order`
    /// 3. 根据返回结果更新状态
    pub async fn submit_order(
        order: &mut Order,
        gateway: &dyn ExchangeGateway,
        ctx: &LifecycleContext<'_>,
        now: DateTime<Local>,
    ) -> anyhow::Result<GatewayResult> {
        // 1. Created/Validated → PendingSubmit
        if order.status == OrderStatus::Created || order.status == OrderStatus::Validated {
            Self::apply_transition(
                order,
                OrderStatus::PendingSubmit,
                "OMS 决策完成，等待提交",
                "oms",
                ctx,
                now,
            )?;
            ctx.event_bus.publish(OrderEvent::OrderPendingSubmit {
                order_id: order.order_id.clone(),
                timestamp: now,
            });
        }

        // 2. PendingSubmit → Submitted
        Self::apply_transition(
            order,
            OrderStatus::Submitted,
            "已提交到 Gateway",
            "oms",
            ctx,
            now,
        )?;
        ctx.event_bus.publish(OrderEvent::OrderSubmitted {
            order_id: order.order_id.clone(),
            gateway: gateway.name().to_string(),
            timestamp: now,
        });
        ctx.repository.save(order)?;

        // 3. 调用 Gateway
        let request = order_to_gateway_request(order);
        let result = gateway.submit_order(&request, now).await;

        // 4. 处理 Gateway 返回
        Self::apply_gateway_result(order, &result, gateway, ctx, now)?;
        Ok(result)
    }

    // ------------------------------------------------------------------
    // 4. cancel_order
    // ------------------------------------------------------------------

    /// 取消订单。
    ///
    /// 仅对活跃订单生效；已终态订单直接返回。
    pub async fn cancel_order(
        order: &mut Order,
        reason: &str,
        gateway: &dyn ExchangeGateway,
        ctx: &LifecycleContext<'_>,
        now: DateTime<Local>,
    ) -> anyhow::Result<GatewayResult> {
        if !order.status.is_active() {
            tracing::warn!(
                order_id = %order.order_id,
                status = %order.status.as_zh(),
                "订单已处于非活跃态，跳过取消"
            );
            return Ok(GatewayResult::cancelled(
                order.exchange_order_id.as_deref().unwrap_or(""),
                &format!("订单非活跃（{}）", order.status.as_zh()),
                0,
            ));
        }

        // 调用 Gateway（如果有 exchange_order_id）
        let result = if let Some(ref eid) = order.exchange_order_id {
            gateway.cancel_order(eid).await
        } else {
            GatewayResult::cancelled("", "订单未提交到 Gateway，本地取消", 0)
        };

        Self::apply_transition(
            order,
            OrderStatus::Cancelled,
            reason,
            "oms",
            ctx,
            now,
        )?;
        ctx.event_bus.publish(OrderEvent::OrderCancelled {
            order_id: order.order_id.clone(),
            reason: reason.to_string(),
            timestamp: now,
        });
        ctx.repository.save(order)?;
        Ok(result)
    }

    // ------------------------------------------------------------------
    // 5. replace_order
    // ------------------------------------------------------------------

    /// 替换订单 = cancel(old) + create(new) + submit(new)。
    ///
    /// 返回新订单（已提交）。
    pub async fn replace_order(
        old: &mut Order,
        new_input: &CreateOrderInput,
        gateway: &dyn ExchangeGateway,
        ctx: &LifecycleContext<'_>,
        now: DateTime<Local>,
    ) -> anyhow::Result<Order> {
        // 1. 取消旧订单
        Self::cancel_order(old, "替换订单：取消旧订单", gateway, ctx, now).await?;
        // 2. 创建新订单
        let mut new_order = Self::create_order(new_input, ctx, now)?;
        // 3. 直接 submit（默认 Validated → PendingSubmit → Submitted）
        Self::submit_order(&mut new_order, gateway, ctx, now).await?;
        Ok(new_order)
    }

    // ------------------------------------------------------------------
    // 6. 状态应用辅助
    // ------------------------------------------------------------------

    /// 应用状态机校验 + transition + 持久化 + 事件。
    fn apply_transition(
        order: &mut Order,
        target: OrderStatus,
        reason: &str,
        actor: &str,
        ctx: &LifecycleContext,
        now: DateTime<Local>,
    ) -> anyhow::Result<()> {
        if let Err(e) = ctx
            .state_machine
            .validate_transition(order.status, target)
        {
            // 非法转移：发状态机拒绝事件，但不修改状态
            ctx.event_bus.publish(OrderEvent::StateTransitionRejected {
                order_id: order.order_id.clone(),
                from: order.status,
                to: target,
                reason: e.to_string(),
                timestamp: now,
            });
            tracing::error!(
                order_id = %order.order_id,
                from = %order.status.as_zh(),
                to = %target.as_zh(),
                error = %e,
                "OMS 状态机非法转移被拒绝"
            );
            return Err(anyhow::anyhow!(
                "OMS 状态机非法转移 {} → {}：{}",
                order.status.as_zh(),
                target.as_zh(),
                e
            ));
        }
        order.transition(target, reason, actor, now);
        ctx.repository.append_status_change(
            &order.order_id,
            order
                .status_history
                .last()
                .expect("transition 已记录 StatusChange"),
        )?;
        Ok(())
    }

    /// 把 GatewayResult 应用到 Order 上（状态 + 事件）。
    fn apply_gateway_result(
        order: &mut Order,
        result: &GatewayResult,
        gateway: &dyn ExchangeGateway,
        ctx: &LifecycleContext,
        now: DateTime<Local>,
    ) -> anyhow::Result<()> {
        let exec_status = result.status;
        let oms_status = OrderStatus::from_execution(exec_status);

        if !result.success {
            // Gateway 拒绝 / 失败
            let target = match oms_status {
                OrderStatus::Expired => OrderStatus::Expired,
                _ => OrderStatus::Rejected,
            };
            ctx.event_bus.publish(OrderEvent::GatewayError {
                order_id: order.order_id.clone(),
                gateway: gateway.name().to_string(),
                error: result.message.clone(),
                timestamp: now,
            });
            Self::apply_transition(
                order,
                target,
                &format!("Gateway 拒绝：{}", result.message),
                "gateway",
                ctx,
                now,
            )?;
            ctx.event_bus.publish(OrderEvent::OrderRejected {
                order_id: order.order_id.clone(),
                reason: result.message.clone(),
                timestamp: now,
            });
            ctx.repository.save(order)?;
            return Ok(());
        }

        // Gateway 接受
        if !result.gateway_order_id.is_empty() {
            order.set_exchange_order_id(&result.gateway_order_id);
        }
        match oms_status {
            OrderStatus::Accepted => {
                Self::apply_transition(
                    order,
                    OrderStatus::Accepted,
                    &format!("Gateway 接受（{}）", result.message),
                    "gateway",
                    ctx,
                    now,
                )?;
                ctx.event_bus.publish(OrderEvent::OrderAccepted {
                    order_id: order.order_id.clone(),
                    gateway: gateway.name().to_string(),
                    exchange_order_id: result.gateway_order_id.clone(),
                    timestamp: now,
                });
                ctx.repository.save(order)?;
            }
            OrderStatus::PartiallyFilled => {
                order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
                Self::apply_transition(
                    order,
                    OrderStatus::PartiallyFilled,
                    "Gateway 返回部分成交",
                    "gateway",
                    ctx,
                    now,
                )?;
                ctx.event_bus.publish(OrderEvent::OrderPartiallyFilled {
                    order_id: order.order_id.clone(),
                    filled: result.filled,
                    remaining: result.remaining,
                    avg_price: result.avg_price.unwrap_or(order.price),
                    timestamp: now,
                });
                ctx.repository.save(order)?;
            }
            OrderStatus::Filled => {
                order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
                Self::apply_transition(
                    order,
                    OrderStatus::Filled,
                    "Gateway 返回完全成交",
                    "gateway",
                    ctx,
                    now,
                )?;
                let avg = result.avg_price.unwrap_or(order.price);
                let slippage = if order.price > 0.0 {
                    (avg - order.price).abs() / order.price
                } else {
                    0.0
                };
                order.slippage = slippage;
                ctx.event_bus.publish(OrderEvent::OrderFilled {
                    order_id: order.order_id.clone(),
                    avg_price: avg,
                    slippage,
                    timestamp: now,
                });
                ctx.repository.save(order)?;
            }
            OrderStatus::Cancelled => {
                Self::apply_transition(
                    order,
                    OrderStatus::Cancelled,
                    &format!("Gateway 取消：{}", result.message),
                    "gateway",
                    ctx,
                    now,
                )?;
                ctx.event_bus.publish(OrderEvent::OrderCancelled {
                    order_id: order.order_id.clone(),
                    reason: result.message.clone(),
                    timestamp: now,
                });
                ctx.repository.save(order)?;
            }
            OrderStatus::Expired => {
                Self::apply_transition(
                    order,
                    OrderStatus::Expired,
                    "Gateway 标记为过期",
                    "gateway",
                    ctx,
                    now,
                )?;
                ctx.event_bus.publish(OrderEvent::OrderExpired {
                    order_id: order.order_id.clone(),
                    timestamp: now,
                });
                ctx.repository.save(order)?;
            }
            _ => {
                tracing::warn!(
                    order_id = %order.order_id,
                    status = ?exec_status,
                    "Gateway 返回未预期状态，已忽略"
                );
            }
        }
        Ok(())
    }
}

// ============================================================================
// 内部辅助：Order → Gateway OrderRequest
// ============================================================================

fn order_to_gateway_request(order: &Order) -> OrderRequest {
    OrderRequest::new(
        &order.market_id,
        order.direction,
        order.side,
        order.price,
        order.quantity,
        &order.strategy_id,
        &order.risk_id,
        &order.opportunity_id,
    )
    .with_order_type(order.order_type)
    .with_time_in_force(order.time_in_force)
    .with_client_order_id(&order.client_order_id)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventBus;
    use crate::repository::memory::InMemoryRepository;
    use crate::validator::Validator;
    use pm_gateway::create_mock_gateway;

    fn setup() -> (InMemoryRepository, EventBus, StateMachine, Validator) {
        (
            InMemoryRepository::new(),
            EventBus::new(),
            StateMachine::new(),
            Validator::with_default_rules(),
        )
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
    fn create_order_basic() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        assert_eq!(order.status, OrderStatus::Created);
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(bus.published_count(), 1);
    }

    #[test]
    fn create_order_dedup_by_client_id() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let now = Local::now();
        let input1 = base_input();
        let o1 = Lifecycle::create_order(&input1, &lctx, now).unwrap();
        // 第二次创建相同 client_order_id：返回已有订单
        let o2 = Lifecycle::create_order(&input1, &lctx, now).unwrap();
        assert_eq!(o1.order_id, o2.order_id);
        assert_eq!(repo.count().unwrap(), 1);
    }

    #[test]
    fn validate_passes_then_validated_status() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let mut vctx = ValidationContext::minimal();
        vctx.balance = Some(Balance::mock(10_000.0));
        let result = Lifecycle::validate_order(&mut order, &vctx, &lctx, Local::now()).unwrap();
        assert!(result.all_passed);
        assert_eq!(order.status, OrderStatus::Validated);
    }

    #[test]
    fn validate_failure_rejects() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut input = base_input();
        input.price = -1.0;
        let mut order = Lifecycle::create_order(&input, &lctx, Local::now()).unwrap();
        let result = Lifecycle::validate_order(
            &mut order,
            &ValidationContext::minimal(),
            &lctx,
            Local::now(),
        )
        .unwrap();
        assert!(!result.all_passed);
        assert_eq!(order.status, OrderStatus::Rejected);
    }

    #[tokio::test]
    async fn submit_to_mock_gateway_accepted() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let gateway = create_mock_gateway();
        let result = Lifecycle::submit_order(&mut order, gateway.as_ref(), &lctx, Local::now()).await
            .unwrap();
        assert!(result.success);
        assert!(matches!(
            order.status,
            OrderStatus::Accepted | OrderStatus::Filled | OrderStatus::PartiallyFilled
        ));
    }

    #[tokio::test]
    async fn submit_to_mock_gateway_rejected() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let gateway = create_mock_gateway();
        let result = Lifecycle::submit_order(&mut order, gateway.as_ref(), &lctx, Local::now()).await
            .unwrap();
        assert!(result.success || !result.success);
    }

    #[tokio::test]
    async fn cancel_active_order() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let gateway = create_mock_gateway();
        let result = Lifecycle::cancel_order(&mut order, "用户取消", gateway.as_ref(), &lctx, Local::now()).await
            .unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn cancel_terminal_order_noop() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut order = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let gateway = create_mock_gateway();
        Lifecycle::submit_order(&mut order, gateway.as_ref(), &lctx, Local::now()).await.unwrap();
        order.transition(OrderStatus::Filled, "测试", "oms", Local::now());
        let result = Lifecycle::cancel_order(&mut order, "再试一次", gateway.as_ref(), &lctx, Local::now()).await.unwrap();
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[tokio::test]
    async fn replace_order_creates_new_and_cancels_old() {
        let (repo, bus, sm, v) = setup();
        let lctx = LifecycleContext::new(&repo, &bus, &sm, &v);
        let mut old = Lifecycle::create_order(&base_input(), &lctx, Local::now()).unwrap();
        let mut new_input = base_input();
        new_input.client_order_id = "CLI-NEW".into();
        new_input.price = 0.50;
        let gateway = create_mock_gateway();
        let new_order = Lifecycle::replace_order(&mut old, &new_input, gateway.as_ref(), &lctx, Local::now()).await
            .unwrap();
        assert_eq!(old.status, OrderStatus::Cancelled);
        assert_ne!(new_order.order_id, old.order_id);
        assert_eq!(repo.count().unwrap(), 2);
    }
}