//! Execution Pipeline（V1.06 第二节）。
//!
//! 统一执行管道：所有订单必须经过此管道，禁止任何模块绕过。
//!
//! 流程：
//! ```text
//! ExecutionRequest
//!   → OrderBuilder.build()             → Order(Created)
//!   → ExecutionValidator.validate()    → Pass / Reject
//!   → ExecutionQueue.enqueue()         → Order(Queued)
//!   → [process_queue 循环]
//!   → ExecutionScheduler.acquire()     → 速率控制
//!   → ExecutionGateway.submit_order()  → Order(Submitted/Accepted/Rejected)
//!   → [Gateway 回调/轮询]
//!   → Order(Filled/Cancelled/Expired)
//!   → Portfolio Sync
//!   → Metrics 更新
//! ```
//!
//! Simulation Only -- Mock Gateway / 不连接钱包 / 不真实下单。

use chrono::{DateTime, Local};

use crate::builder::OrderBuilder;
use crate::order::{Order, OrderStatus};
use crate::queue::{ExecutionQueue, QueueConfig, QueueStatus};
use crate::request::ExecutionRequest;
use crate::validator::{ExecutionValidator, ValidationContext, ValidationOutcome};

// ============================================================================
// Execution Result
// ============================================================================

/// 单次 submit 的返回结果。
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// 订单已成功进入 Pipeline（已入队）。
    Accepted {
        order_id: String,
        message: String,
    },
    /// 订单被拒绝（Validator 失败 或 队列满）。
    Rejected {
        order_id: String,
        reasons: Vec<String>,
    },
}

impl ExecutionResult {
    pub fn is_accepted(&self) -> bool {
        matches!(self, ExecutionResult::Accepted { .. })
    }
}

// ============================================================================
// Pipeline Status
// ============================================================================

/// Pipeline 整体状态（供 CLI `cargo run -- execution` 展示）。
#[derive(Debug, Clone)]
pub struct PipelineStatus {
    /// 队列状态。
    pub queue: QueueStatus,
    /// 总提交数。
    pub total_submitted: u64,
    /// 总接受数。
    pub total_accepted: u64,
    /// 总拒绝数。
    pub total_rejected: u64,
    /// 当前活跃订单数。
    pub active_orders: usize,
    /// 当前终态订单数。
    pub terminal_orders: usize,
    /// Gateway 名称。
    pub gateway_name: String,
}

// ============================================================================
// Execution Pipeline
// ============================================================================

/// 执行管道（V1.06 第二节）。
///
/// 唯一订单入口。持有 Builder / Validator / Queue / Scheduler 引用 / Gateway 引用。
/// Strategy / Risk 禁止直接下单 — 必须通过此 Pipeline。
pub struct ExecutionPipeline {
    /// 订单构建器。
    builder: OrderBuilder,
    /// 校验器。
    validator: ExecutionValidator,
    /// 执行队列。
    queue: ExecutionQueue,
    /// 所有订单（活跃 + 终态）。
    all_orders: Vec<Order>,
    /// 已存在的 client_order_id 集合（去重用）。
    existing_client_ids: std::collections::HashSet<String>,
    /// 总提交数。
    total_submitted: u64,
    /// 总接受数。
    total_accepted: u64,
    /// 总拒绝数。
    total_rejected: u64,
    /// Scheduler 配置（暂未接入实际 Scheduler，用配置占位）。
    max_orders_per_second: u32,
}

impl ExecutionPipeline {
    /// 创建新的 Execution Pipeline。
    pub fn new(queue_config: QueueConfig) -> Self {
        Self {
            builder: OrderBuilder::new(),
            validator: ExecutionValidator::with_default_rules(),
            queue: ExecutionQueue::new(queue_config),
            all_orders: Vec::new(),
            existing_client_ids: std::collections::HashSet::new(),
            total_submitted: 0,
            total_accepted: 0,
            total_rejected: 0,
            max_orders_per_second: 10,
        }
    }

    /// 使用默认配置创建。
    pub fn with_defaults() -> Self {
        Self::new(QueueConfig::default())
    }

    /// 设置 order_id 起始计数器（从历史 CSV 恢复）。
    pub fn set_order_counter(&mut self, base: u64) {
        self.builder = OrderBuilder::new().with_counter(base);
    }

    /// 设置每秒最大订单数。
    pub fn set_rate_limit(&mut self, max_per_second: u32) {
        self.max_orders_per_second = max_per_second;
    }

    // ---- 核心方法 ----

    /// 提交 ExecutionRequest 到 Pipeline。
    ///
    /// 这是 Execution Engine 的唯一入口。流程：
    /// 1. OrderBuilder 构建 Order（Created）
    /// 2. ExecutionValidator 校验
    /// 3. 通过 → ExecutionQueue 入队（Queued）
    /// 4. 失败 → 返回 Rejected
    pub fn submit(
        &mut self,
        request: ExecutionRequest,
        now: DateTime<Local>,
        ctx: &ValidationContext,
    ) -> ExecutionResult {
        self.total_submitted += 1;

        // Step 1: 构建 Order
        let mut order = self.builder.build(request, now);
        let order_id = order.order_id.clone();

        // 将 client_order_id 加入去重集合
        self.existing_client_ids.insert(order.client_order_id.clone());

        tracing::info!(
            order_id = %order_id,
            market = %order.market_id,
            side = %order.side.as_str(),
            price = %order.price,
            qty = %order.quantity,
            "收到执行请求"
        );

        // Step 2: 校验
        let outcome = self.validator.validate(&order, ctx);
        match outcome {
            ValidationOutcome::Pass => {
                order.transition(OrderStatus::Validated, "校验通过", now);
                tracing::info!(order_id = %order_id, "校验通过");
            }
            ValidationOutcome::Reject { reasons } => {
                order.transition(
                    OrderStatus::Rejected,
                    &format!("校验拒绝: {}", reasons.join("; ")),
                    now,
                );
                self.all_orders.push(order);
                self.total_rejected += 1;
                return ExecutionResult::Rejected {
                    order_id,
                    reasons,
                };
            }
        }

        // Step 3: 入队
        match self.queue.enqueue(order, now) {
            Ok(()) => {
                self.total_accepted += 1;
                ExecutionResult::Accepted {
                    order_id: order_id.clone(),
                    message: format!("订单 {} 已进入执行队列", order_id),
                }
            }
            Err(e) => {
                // 入队失败：从队列中找回订单，标记 Rejected
                if let Some(mut failed_order) = self.queue.remove(&order_id) {
                    failed_order.transition(
                        OrderStatus::Rejected,
                        &format!("入队失败: {}", e),
                        now,
                    );
                    self.all_orders.push(failed_order);
                }
                self.total_rejected += 1;
                ExecutionResult::Rejected {
                    order_id,
                    reasons: vec![e.to_string()],
                }
            }
        }
    }

    /// 从队列中取出下一个待发送订单。
    ///
    /// 调用方（通常是 Gateway 发送循环）调用此方法获取下一个订单。
    pub fn next_order(&mut self) -> Option<Order> {
        self.queue.dequeue()
    }

    /// 处理订单提交结果（Gateway 回调）。
    ///
    /// 根据 Gateway 返回结果更新订单状态。
    pub fn handle_gateway_result(
        &mut self,
        mut order: Order,
        success: bool,
        status: OrderStatus,
        filled: f64,
        avg_price: f64,
        message: &str,
        now: DateTime<Local>,
    ) {
        if success {
            order.transition(OrderStatus::Submitted, "已提交到 Gateway", now);
            order.transition(status, message, now);
            if filled > 0.0 {
                order.update_fill(filled, avg_price, 0.0);
            }
        } else {
            order.transition(status, message, now);
            // 尝试重试
            let order_id = order.order_id.clone();
            match self.queue.retry(&order_id, now) {
                Ok(()) => {
                    tracing::info!(order_id = %order_id, "订单已重试入队");
                }
                Err(e) => {
                    tracing::warn!(order_id = %order_id, error = %e, "订单重试失败");
                    order.transition(OrderStatus::Failed, &format!("重试失败: {}", e), now);
                }
            }
        }
        self.all_orders.push(order);
    }

    /// 获取 Pipeline 状态。
    pub fn status(&self) -> PipelineStatus {
        let active = self
            .all_orders
            .iter()
            .filter(|o| o.status.is_active())
            .count();
        let terminal = self
            .all_orders
            .iter()
            .filter(|o| o.status.is_terminal())
            .count();

        PipelineStatus {
            queue: self.queue.status(),
            total_submitted: self.total_submitted,
            total_accepted: self.total_accepted,
            total_rejected: self.total_rejected,
            active_orders: active + self.queue.total_pending(),
            terminal_orders: terminal,
            gateway_name: "MockGateway".to_string(),
        }
    }

    /// 获取所有订单的引用。
    pub fn all_orders(&self) -> &[Order] {
        &self.all_orders
    }

    /// 获取队列的不可变引用。
    pub fn queue(&self) -> &ExecutionQueue {
        &self.queue
    }

    /// 获取队列的可变引用（用于手动操作队列）。
    pub fn queue_mut(&mut self) -> &mut ExecutionQueue {
        &mut self.queue
    }

    /// 获取统计摘要。
    pub fn stats(&self) -> PipelineStats {
        let filled = self
            .all_orders
            .iter()
            .filter(|o| o.status == OrderStatus::Filled)
            .count() as u64;
        let cancelled = self
            .all_orders
            .iter()
            .filter(|o| o.status == OrderStatus::Cancelled)
            .count() as u64;
        let expired = self
            .all_orders
            .iter()
            .filter(|o| o.status == OrderStatus::Expired)
            .count() as u64;
        let failed = self
            .all_orders
            .iter()
            .filter(|o| o.status == OrderStatus::Failed)
            .count() as u64;

        PipelineStats {
            total_submitted: self.total_submitted,
            total_accepted: self.total_accepted,
            total_rejected: self.total_rejected,
            filled,
            cancelled,
            expired,
            failed,
        }
    }

    /// 打印 Pipeline 状态（中文）。
    pub fn print_status(&self) {
        let s = self.status();
        println!("【Execution Pipeline 状态】");
        println!();
        println!("  Gateway      : {}", s.gateway_name);
        println!("  总提交       : {}", s.total_submitted);
        println!("  总接受       : {}", s.total_accepted);
        println!("  总拒绝       : {}", s.total_rejected);
        println!("  活跃订单     : {}", s.active_orders);
        println!("  终态订单     : {}", s.terminal_orders);
        println!("  速率限制     : {} 单/秒", self.max_orders_per_second);
        println!();
        self.queue.print_status();
    }
}

/// Pipeline 统计摘要。
#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub total_submitted: u64,
    pub total_accepted: u64,
    pub total_rejected: u64,
    pub filled: u64,
    pub cancelled: u64,
    pub expired: u64,
    pub failed: u64,
}

impl PipelineStats {
    /// 中文打印。
    pub fn print_zh(&self) {
        println!("【执行统计】");
        println!();
        println!("  总提交 : {}", self.total_submitted);
        println!("  总接受 : {}", self.total_accepted);
        println!("  总拒绝 : {}", self.total_rejected);
        println!("  已成交 : {}", self.filled);
        println!("  已取消 : {}", self.cancelled);
        println!("  已过期 : {}", self.expired);
        println!("  失败   : {}", self.failed);
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use pm_core::Side;

    #[test]
    fn submit_accepts_valid_request() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();

        let req = ExecutionRequest::new(
            "mkt-1", "测试市场?", "mock",
            Direction::Yes, Side::Buy,
            0.45, 100.0,
            "S1", "R1", "O1",
        );

        let ctx = ValidationContext::default();
        let result = pipeline.submit(req, now, &ctx);
        assert!(result.is_accepted());
        assert_eq!(pipeline.total_submitted, 1);
        assert_eq!(pipeline.total_accepted, 1);
        assert_eq!(pipeline.total_rejected, 0);
    }

    #[test]
    fn submit_rejects_invalid_price() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();

        let req = ExecutionRequest::new(
            "mkt-1", "Q", "mock",
            Direction::Yes, Side::Buy,
            0.0, 100.0, // 非法价格
            "S1", "R1", "O1",
        );

        let ctx = ValidationContext::default();
        let result = pipeline.submit(req, now, &ctx);
        assert!(!result.is_accepted());
        assert_eq!(pipeline.total_submitted, 1);
        assert_eq!(pipeline.total_accepted, 0);
        assert_eq!(pipeline.total_rejected, 1);
    }

    #[test]
    fn submit_rejects_insufficient_cash() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();

        let req = ExecutionRequest::new(
            "mkt-1", "Q", "mock",
            Direction::Yes, Side::Buy,
            0.5, 50000.0, // 需要 25000 USDC
            "S1", "R1", "O1",
        );

        let ctx = ValidationContext {
            available_cash: 100.0,
            ..ValidationContext::default()
        };
        let result = pipeline.submit(req, now, &ctx);
        assert!(!result.is_accepted());
        match result {
            ExecutionResult::Rejected { reasons, .. } => {
                assert!(reasons.iter().any(|r| r.contains("资金")));
            }
            _ => panic!("Expected Rejected"),
        }
    }

    #[test]
    fn next_order_returns_queued_order() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();

        let req = ExecutionRequest::new(
            "mkt-1", "Q", "mock",
            Direction::Yes, Side::Buy,
            0.45, 100.0,
            "S1", "R1", "O1",
        )
        .with_priority(5);
        let ctx = ValidationContext::default();
        pipeline.submit(req, now, &ctx);

        let next = pipeline.next_order();
        assert!(next.is_some());
        let order = next.unwrap();
        assert_eq!(order.priority, 5);
        assert_eq!(order.status, OrderStatus::Queued);
    }

    #[test]
    fn handle_gateway_result_success() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();

        let req = ExecutionRequest::new(
            "mkt-1", "Q", "mock",
            Direction::Yes, Side::Buy,
            0.45, 100.0,
            "S1", "R1", "O1",
        );
        let ctx = ValidationContext::default();
        pipeline.submit(req, now, &ctx);

        let order = pipeline.next_order().unwrap();
        let order_id = order.order_id.clone();
        pipeline.handle_gateway_result(
            order, true, OrderStatus::Accepted,
            0.0, 0.0, "Gateway 确认接收", now,
        );

        // 订单应已记录
        let found = pipeline.all_orders().iter().find(|o| o.order_id == order_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().status, OrderStatus::Accepted);
    }

    #[test]
    fn pipeline_stats_accurate() {
        let now = Local::now();
        let mut pipeline = ExecutionPipeline::with_defaults();
        let ctx = ValidationContext::default();

        // 提交 3 个
        for i in 0..3 {
            let req = ExecutionRequest::new(
                &format!("mkt-{}", i), "Q", "mock",
                Direction::Yes, Side::Buy,
                0.45, 100.0,
                "S", "R", "O",
            );
            pipeline.submit(req, now, &ctx);
        }

        let stats = pipeline.stats();
        assert_eq!(stats.total_submitted, 3);
        assert_eq!(stats.total_accepted, 3);
        assert_eq!(stats.total_rejected, 0);
    }
}
