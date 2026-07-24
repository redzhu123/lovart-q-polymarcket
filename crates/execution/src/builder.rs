//! Order Builder（V1.06 第二节）。
//!
//! 将 ExecutionRequest 转换为 Order。
//! 负责：生成 order_id、设置初始状态、计算 remaining。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};

use crate::order::Order;
use crate::request::ExecutionRequest;

/// 订单构建器。
///
/// 维护 order_id 计数器，确保每个订单有唯一 ID。
pub struct OrderBuilder {
    counter: u64,
    /// 订单 ID 前缀。
    prefix: String,
}

impl OrderBuilder {
    /// 创建新的 OrderBuilder。
    pub fn new() -> Self {
        Self {
            counter: 0,
            prefix: "EX".to_string(),
        }
    }

    /// 设置计数器基线（从历史 CSV 恢复时使用）。
    pub fn with_counter(mut self, base: u64) -> Self {
        self.counter = base;
        self
    }

    /// 设置 ID 前缀。
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    /// 生成下一个 order_id。
    pub fn next_order_id(&mut self) -> String {
        self.counter += 1;
        format!("{}-{:06}", self.prefix, self.counter)
    }

    /// 当前计数器值。
    pub fn counter(&self) -> u64 {
        self.counter
    }

    /// 从 ExecutionRequest 构建 Order。
    ///
    /// 生成的 Order 处于 Created 状态，filled=0，remaining=quantity。
    /// 同时继承 request 的 priority。
    pub fn build(&mut self, request: ExecutionRequest, now: DateTime<Local>) -> Order {
        let order_id = self.next_order_id();
        let client_order_id = if request.client_order_id.is_empty() {
            order_id.clone()
        } else {
            request.client_order_id
        };

        let mut order = Order::new(
            order_id,
            client_order_id,
            request.market_id,
            request.provider,
            request.direction,
            request.side,
            request.price,
            request.quantity,
            request.strategy_id,
            request.risk_id,
            request.opportunity_id,
            now,
        );
        order.priority = request.priority;
        order
    }
}

impl Default for OrderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use pm_core::Side;

    #[test]
    fn order_id_increment() {
        let mut builder = OrderBuilder::new();
        assert_eq!(builder.next_order_id(), "EX-000001");
        assert_eq!(builder.next_order_id(), "EX-000002");
        assert_eq!(builder.next_order_id(), "EX-000003");
    }

    #[test]
    fn with_counter_resumes() {
        let mut builder = OrderBuilder::new().with_counter(100);
        assert_eq!(builder.next_order_id(), "EX-000101");
    }

    #[test]
    fn build_creates_order() {
        let now = Local::now();
        let mut builder = OrderBuilder::new();
        let req = ExecutionRequest::new(
            "mkt-1",
            "测试?",
            "mock",
            Direction::Yes,
            Side::Buy,
            0.45,
            222.22,
            "S1",
            "R1",
            "O1",
        );
        let order = builder.build(req, now);
        assert_eq!(order.order_id, "EX-000001");
        assert_eq!(order.status, crate::order::OrderStatus::Created);
        assert!((order.quantity - 222.22).abs() < 1e-9);
        assert!((order.remaining - 222.22).abs() < 1e-9);
        assert_eq!(order.filled, 0.0);
        assert!(order.simulation_only);
    }

    #[test]
    fn client_order_id_preserved() {
        let now = Local::now();
        let mut builder = OrderBuilder::new();
        let req = ExecutionRequest::new(
            "mkt-1",
            "Q",
            "mock",
            Direction::Yes,
            Side::Buy,
            0.5,
            100.0,
            "S",
            "R",
            "O",
        )
        .with_client_order_id("MY-CLIENT-ID");
        let order = builder.build(req, now);
        assert_eq!(order.client_order_id, "MY-CLIENT-ID");
    }
}
