//! Execution Gateway（V1.06 第八节）。
//!
//! Gateway Trait 定义订单提交的统一接口。
//! Execution 禁止直接 HTTP — 所有订单通过 Gateway 发送。
//!
//! 当前实现：MockGateway（模拟成交）
//! 未来实现：PolymarketGateway / KalshiGateway / DEXGateway
//!
//! Simulation Only -- MockGateway 不连接任何真实交易所。

use async_trait::async_trait;
use chrono::{DateTime, Local};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::order::{Order, OrderStatus};

// ============================================================================
// Gateway Result
// ============================================================================

/// Gateway 操作结果。
#[derive(Debug, Clone)]
pub struct GatewayResult {
    /// 操作是否成功。
    pub success: bool,
    /// 订单 ID（Gateway 侧的 ID，可能与 Execution ID 不同）。
    pub gateway_order_id: String,
    /// Gateway 返回的订单状态。
    pub status: OrderStatus,
    /// 已成交数量。
    pub filled: f64,
    /// 加权平均成交价（如有）。
    pub avg_price: Option<f64>,
    /// Gateway 返回的消息（错误或确认）。
    pub message: String,
    /// 本次操作耗时（毫秒）。
    pub latency_ms: u64,
}

impl GatewayResult {
    /// 成功提交。
    pub fn accepted(order_id: &str, message: &str, latency_ms: u64) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Accepted,
            filled: 0.0,
            avg_price: None,
            message: message.to_string(),
            latency_ms,
        }
    }

    /// 完全成交。
    pub fn filled(order_id: &str, filled: f64, avg_price: f64, latency_ms: u64) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Filled,
            filled,
            avg_price: Some(avg_price),
            message: "完全成交".to_string(),
            latency_ms,
        }
    }

    /// 部分成交。
    pub fn partially_filled(
        order_id: &str,
        filled: f64,
        avg_price: f64,
        latency_ms: u64,
    ) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::PartiallyFilled,
            filled,
            avg_price: Some(avg_price),
            message: "部分成交".to_string(),
            latency_ms,
        }
    }

    /// 订单被拒绝。
    pub fn rejected(order_id: &str, reason: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Rejected,
            filled: 0.0,
            avg_price: None,
            message: reason.to_string(),
            latency_ms,
        }
    }

    /// 订单过期。
    pub fn expired(order_id: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Expired,
            filled: 0.0,
            avg_price: None,
            message: "订单已过期".to_string(),
            latency_ms,
        }
    }

    /// 操作失败。
    pub fn failed(order_id: &str, error: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Failed,
            filled: 0.0,
            avg_price: None,
            message: error.to_string(),
            latency_ms,
        }
    }
}

// ============================================================================
// Gateway Trait
// ============================================================================

/// 执行网关 trait（V1.06 第八节）。
///
/// 所有订单提交必须通过 Gateway。Execution 禁止直接 HTTP。
/// 未来实现：PolymarketGateway / KalshiGateway / DEXGateway。
#[async_trait]
pub trait ExecutionGateway: Send + Sync {
    /// 提交订单到交易所。
    async fn submit_order(&self, order: &Order, now: DateTime<Local>) -> GatewayResult;

    /// 取消订单。
    async fn cancel_order(&self, order_id: &str) -> GatewayResult;

    /// 查询订单状态。
    async fn order_status(&self, order_id: &str) -> GatewayResult;

    /// Gateway 名称。
    fn name(&self) -> &str;

    /// Gateway 健康检查。
    async fn health_check(&self) -> bool;
}

// ============================================================================
// Mock Gateway
// ============================================================================

/// 模拟成交参数。
const PROB_LIQUIDITY_FAIL: f64 = 0.04;
const PROB_SINGLE_FILL: f64 = 0.70;
const SLIPPAGE_BASE: f64 = 0.0005;
const SLIPPAGE_PER_SHARE: f64 = 0.00001;
const SLIPPAGE_JITTER: f64 = 0.0001;

/// Mock Gateway（V1.06 第八节）。
///
/// 模拟成交过程：随机延迟 / 滑点 / 部分成交 / 流动性失败。
/// 不连接任何真实交易所、不产生真实订单。
///
/// 使用 `StdRng`（`Send + Sync`）而非 `ThreadRng`，
/// 以满足 `async_trait` 的 `Send + Sync` 约束。
pub struct MockGateway {
    /// 随机数生成器（StdRng 是 Send + Sync 的）。
    rng: std::sync::Mutex<StdRng>,
    /// Gateway 名称。
    name: String,
    /// 模拟基础延迟（毫秒）。
    base_latency_ms: u64,
}

impl MockGateway {
    /// 创建新的 MockGateway。
    pub fn new(_max_fill_delay: u32) -> Self {
        Self {
            rng: std::sync::Mutex::new(StdRng::from_rng(&mut rand::rng())),
            name: "MockGateway".to_string(),
            base_latency_ms: 5,
        }
    }

    /// 设置基础延迟。
    #[allow(dead_code)]
    pub fn with_latency(mut self, ms: u64) -> Self {
        self.base_latency_ms = ms;
        self
    }

    /// 模拟网络延迟（毫秒）。
    fn simulate_latency(&self) -> u64 {
        let mut rng = self.rng.lock().unwrap();
        self.base_latency_ms + rng.random_range(0u64..20)
    }

    /// 是否流动性失败。
    fn liquidity_fail(rng: &mut StdRng) -> bool {
        rng.random_bool(PROB_LIQUIDITY_FAIL)
    }

    /// 生成分批成交计划。
    fn partial_schedule(rng: &mut StdRng) -> Vec<f64> {
        let p: f64 = rng.random();
        if p < PROB_SINGLE_FILL {
            vec![1.0]
        } else {
            let split = rng.random_range(0.3..=0.7);
            vec![split, 1.0 - split]
        }
    }

    /// 计算滑点。
    fn slippage(rng: &mut StdRng, quantity: f64) -> f64 {
        let base = SLIPPAGE_BASE + SLIPPAGE_PER_SHARE * quantity;
        let jitter = rng.random_range(-SLIPPAGE_JITTER..=SLIPPAGE_JITTER);
        (base + jitter).max(0.0)
    }
}

#[async_trait]
impl ExecutionGateway for MockGateway {
    async fn submit_order(&self, order: &Order, _now: DateTime<Local>) -> GatewayResult {
        let latency = self.simulate_latency();

        let mut rng = self.rng.lock().unwrap();

        // 检查流动性失败
        if Self::liquidity_fail(&mut rng) {
            return GatewayResult::expired(&order.order_id, latency);
        }

        let schedule = Self::partial_schedule(&mut rng);
        let slippage = Self::slippage(&mut rng, order.quantity);
        let fill_price = match order.side {
            pm_core::Side::Buy => order.price * (1.0 + slippage),
            pm_core::Side::Sell => order.price * (1.0 - slippage),
        };

        if schedule.len() == 1 && (schedule[0] - 1.0).abs() < 1e-9 {
            // 单批完全成交
            GatewayResult::filled(&order.order_id, order.quantity, fill_price, latency)
        } else {
            // 部分成交
            let first_fill = schedule[0] * order.quantity;
            GatewayResult::partially_filled(&order.order_id, first_fill, fill_price, latency)
        }
    }

    async fn cancel_order(&self, order_id: &str) -> GatewayResult {
        let latency = self.simulate_latency();
        GatewayResult {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Cancelled,
            filled: 0.0,
            avg_price: None,
            message: "订单已取消（模拟）".to_string(),
            latency_ms: latency,
        }
    }

    async fn order_status(&self, order_id: &str) -> GatewayResult {
        GatewayResult {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Accepted,
            filled: 0.0,
            avg_price: None,
            message: "订单已接受（模拟）".to_string(),
            latency_ms: self.simulate_latency(),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn health_check(&self) -> bool {
        true
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn test_order() -> Order {
        let now = Local::now();
        Order::new(
            "EX-001".into(), "CLI-001".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, Side::Buy,
            0.45, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        )
    }

    #[tokio::test]
    async fn mock_gateway_submit_returns_result() {
        let gateway = MockGateway::new(3);
        let order = test_order();
        let result = gateway.submit_order(&order, Local::now()).await;
        assert!(!result.gateway_order_id.is_empty());
        assert!(result.latency_ms > 0);
    }

    #[tokio::test]
    async fn mock_gateway_cancel_works() {
        let gateway = MockGateway::new(3);
        let result = gateway.cancel_order("EX-001").await;
        assert!(result.success);
        assert_eq!(result.status, OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn mock_gateway_health_check() {
        let gateway = MockGateway::new(3);
        assert!(gateway.health_check().await);
    }

    #[tokio::test]
    async fn mock_gateway_name() {
        let gateway = MockGateway::new(3);
        assert_eq!(gateway.name(), "MockGateway");
    }

    #[test]
    fn gateway_result_factory_methods() {
        let r = GatewayResult::accepted("EX-001", "已接受", 10);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::Accepted);

        let r = GatewayResult::rejected("EX-002", "资金不足", 5);
        assert!(!r.success);
        assert_eq!(r.status, OrderStatus::Rejected);

        let r = GatewayResult::filled("EX-003", 100.0, 0.452, 15);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::Filled);
        assert_eq!(r.filled, 100.0);
    }
}
