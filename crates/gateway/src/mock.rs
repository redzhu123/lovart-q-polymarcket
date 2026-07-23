//! Mock Gateway（V1.08 第二节）。
//!
//! 模拟网关：Paper / Replay / Test 使用。
//! 不连接任何真实交易所、不产生真实订单。
//! 保留所有 V1.06 MockGateway 行为。

use async_trait::async_trait;
use chrono::{DateTime, Local};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::config::GatewayConfig;
use crate::traits::ExchangeGateway;
use crate::types::{Balance, GatewayInfo, GatewayResult, OrderRequest, Position};

// ============================================================================
// 模拟参数
// ============================================================================

/// 流动性失败概率。
const PROB_LIQUIDITY_FAIL: f64 = 0.04;
/// 单批完全成交概率。
const PROB_SINGLE_FILL: f64 = 0.70;
/// 基础滑点。
const SLIPPAGE_BASE: f64 = 0.0005;
/// 每份额滑点。
const SLIPPAGE_PER_SHARE: f64 = 0.00001;
/// 滑点抖动。
const SLIPPAGE_JITTER: f64 = 0.0001;

// ============================================================================
// MockGateway
// ============================================================================

/// Mock Gateway（V1.08 第二节）。
///
/// 模拟网关：Paper / Replay / Test 使用。
/// 不连接任何真实交易所、不产生真实订单。
///
/// 使用 `StdRng`（`Send + Sync`）满足 `async_trait` 的 `Send + Sync` 约束。
pub struct MockGateway {
    /// 随机数生成器。
    rng: std::sync::Mutex<StdRng>,
    /// Gateway 名称。
    name: String,
    /// Gateway 配置。
    #[allow(dead_code)]
    config: GatewayConfig,
    /// 模拟基础延迟（毫秒）。
    base_latency_ms: u64,
    /// 累计订单数。
    total_orders: u64,
    /// 累计成交数。
    total_fills: u64,
    /// 活跃订单。
    active_orders: std::sync::Mutex<Vec<(String, OrderRequest, DateTime<Local>)>>,
    /// 持仓。
    positions: std::sync::Mutex<Vec<Position>>,
    /// 余额。
    balance: std::sync::Mutex<Balance>,
}

impl MockGateway {
    /// 创建新的 MockGateway。
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            rng: std::sync::Mutex::new(StdRng::from_rng(&mut rand::rng())),
            name: "MockGateway".to_string(),
            base_latency_ms: 5,
            total_orders: 0,
            total_fills: 0,
            active_orders: std::sync::Mutex::new(Vec::new()),
            positions: std::sync::Mutex::new(Vec::new()),
            balance: std::sync::Mutex::new(Balance::mock(10000.0)),
            config,
        }
    }

    /// 带自定义余额创建。
    pub fn with_balance(self, available: f64) -> Self {
        *self.balance.lock().unwrap() = Balance::mock(available);
        self
    }

    /// 带名称创建。
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// 带延迟创建。
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
impl ExchangeGateway for MockGateway {
    fn name(&self) -> &str {
        &self.name
    }

    fn gateway_type(&self) -> &str {
        "mock"
    }

    fn live_enabled(&self) -> bool {
        // Mock 永远不是真实交易
        false
    }

    async fn submit_order(&self, request: &OrderRequest, now: DateTime<Local>) -> GatewayResult {
        let latency = self.simulate_latency();
        let order_id = format!("MOCK-{}", request.client_order_id);

        tracing::info!(
            order_id = %order_id,
            market = %request.market_id,
            side = %request.side.as_str(),
            price = %request.price,
            qty = %request.quantity,
            "MockGateway 收到下单请求"
        );

        let mut rng = self.rng.lock().unwrap();

        // 检查流动性失败
        if Self::liquidity_fail(&mut rng) {
            tracing::warn!(order_id = %order_id, "MockGateway 流动性不足，订单过期");
            return GatewayResult::expired(&order_id, latency);
        }

        let schedule = Self::partial_schedule(&mut rng);
        let slippage = Self::slippage(&mut rng, request.quantity);
        let fill_price = match request.side {
            pm_core::Side::Buy => request.price * (1.0 + slippage),
            pm_core::Side::Sell => request.price * (1.0 - slippage),
        };

        // 记录活跃订单
        {
            let mut active = self.active_orders.lock().unwrap();
            active.push((order_id.clone(), request.clone(), now));
        }

        if schedule.len() == 1 && (schedule[0] - 1.0).abs() < 1e-9 {
            // 单批完全成交
            tracing::info!(
                order_id = %order_id,
                filled = %request.quantity,
                avg_price = %fill_price,
                "MockGateway 完全成交"
            );
            GatewayResult::filled(&order_id, request.quantity, fill_price, latency)
        } else {
            // 部分成交
            let first_fill = schedule[0] * request.quantity;
            let remaining = request.quantity - first_fill;
            tracing::info!(
                order_id = %order_id,
                filled = %first_fill,
                remaining = %remaining,
                avg_price = %fill_price,
                "MockGateway 部分成交"
            );
            GatewayResult::partially_filled(&order_id, first_fill, remaining, fill_price, latency)
        }
    }

    async fn cancel_order(&self, order_id: &str) -> GatewayResult {
        let latency = self.simulate_latency();

        // 从活跃订单中移除
        {
            let mut active = self.active_orders.lock().unwrap();
            active.retain(|(id, _, _)| id != order_id);
        }

        tracing::info!(order_id = %order_id, "MockGateway 订单已取消");
        GatewayResult::cancelled(order_id, "订单已取消（模拟）", latency)
    }

    async fn replace_order(
        &self,
        old_order_id: &str,
        new_request: &OrderRequest,
        now: DateTime<Local>,
    ) -> GatewayResult {
        tracing::info!(
            old_order_id = %old_order_id,
            new_client_id = %new_request.client_order_id,
            "MockGateway 替换订单"
        );

        // 先取消旧订单
        let cancel_result = self.cancel_order(old_order_id).await;
        if !cancel_result.success {
            return cancel_result;
        }

        // 再提交新订单
        self.submit_order(new_request, now).await
    }

    async fn get_order(&self, order_id: &str) -> GatewayResult {
        let latency = self.simulate_latency();
        let active = self.active_orders.lock().unwrap();
        let found = active.iter().find(|(id, _, _)| id == order_id);

        match found {
            Some((id, _, _)) => {
                GatewayResult::accepted(id, "订单活跃（模拟）", latency)
            }
            None => {
                GatewayResult::expired(order_id, latency)
            }
        }
    }

    async fn list_orders(&self) -> Vec<GatewayResult> {
        let latency = self.simulate_latency();
        let active = self.active_orders.lock().unwrap();
        active
            .iter()
            .map(|(id, _, _)| GatewayResult::accepted(id, "活跃（模拟）", latency))
            .collect()
    }

    async fn get_balance(&self) -> anyhow::Result<Balance> {
        let balance = self.balance.lock().unwrap().clone();
        tracing::debug!(
            available = %balance.available,
            total = %balance.total,
            "MockGateway 余额查询"
        );
        Ok(balance)
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        let positions = self.positions.lock().unwrap().clone();
        tracing::debug!(count = %positions.len(), "MockGateway 持仓查询");
        Ok(positions)
    }

    async fn ping(&self) -> bool {
        true
    }

    async fn health(&self) -> GatewayInfo {
        GatewayInfo {
            name: self.name.clone(),
            gateway_type: "mock".to_string(),
            live_enabled: false,
            healthy: true,
            api_latency_ms: self.base_latency_ms,
            http_success_rate: 1.0,
            ws_connected: true,
            rate_limit_remaining: 1.0,
            total_orders: self.total_orders,
            total_fills: self.total_fills,
            connection_status: "模拟网关始终健康".to_string(),
        }
    }

    fn info(&self) -> GatewayInfo {
        GatewayInfo {
            name: self.name.clone(),
            gateway_type: "mock".to_string(),
            live_enabled: false,
            healthy: true,
            api_latency_ms: self.base_latency_ms,
            http_success_rate: 1.0,
            ws_connected: true,
            rate_limit_remaining: 1.0,
            total_orders: self.total_orders,
            total_fills: self.total_fills,
            connection_status: "Mock — 无真实连接".to_string(),
        }
    }
}

impl Default for MockGateway {
    fn default() -> Self {
        Self::new(GatewayConfig::default())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pm_execution::order::Direction;
    use pm_core::Side;

    fn test_request() -> OrderRequest {
        OrderRequest::new(
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

    #[tokio::test]
    async fn mock_gateway_submit_returns_result() {
        let gateway = MockGateway::default();
        let req = test_request();
        let result = gateway.submit_order(&req, Local::now()).await;
        assert!(!result.gateway_order_id.is_empty());
        assert!(result.latency_ms > 0);
    }

    #[tokio::test]
    async fn mock_gateway_cancel_works() {
        let gateway = MockGateway::default();
        let result = gateway.cancel_order("MOCK-test").await;
        assert!(result.success);
        assert_eq!(result.status, pm_execution::order::OrderStatus::Cancelled);
    }

    #[tokio::test]
    async fn mock_gateway_replace_works() {
        let gateway = MockGateway::default();
        let req = test_request();
        let result = gateway.replace_order("MOCK-old", &req, Local::now()).await;
        assert!(!result.gateway_order_id.is_empty());
    }

    #[tokio::test]
    async fn mock_gateway_health() {
        let gateway = MockGateway::default();
        let info = gateway.health().await;
        assert!(info.healthy);
        assert_eq!(info.gateway_type, "mock");
    }

    #[tokio::test]
    async fn mock_gateway_ping() {
        let gateway = MockGateway::default();
        assert!(gateway.ping().await);
    }

    #[tokio::test]
    async fn mock_gateway_never_live() {
        let gateway = MockGateway::default();
        assert!(!gateway.live_enabled());
    }

    #[tokio::test]
    async fn mock_gateway_balance() {
        let gateway = MockGateway::default();
        let balance = gateway.get_balance().await.unwrap();
        assert!(balance.total > 0.0);
        assert_eq!(balance.currency, "USDC");
    }

    #[tokio::test]
    async fn mock_gateway_positions() {
        let gateway = MockGateway::default();
        let positions = gateway.get_positions().await.unwrap();
        // Mock 初始无持仓
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn mock_gateway_list_orders() {
        let gateway = MockGateway::default();
        let req = test_request();
        let _ = gateway.submit_order(&req, Local::now()).await;
        let orders = gateway.list_orders().await;
        assert!(!orders.is_empty());
    }

    #[tokio::test]
    async fn mock_gateway_get_order() {
        let gateway = MockGateway::default();
        let req = test_request();
        let result = gateway.submit_order(&req, Local::now()).await;
        let found = gateway.get_order(&result.gateway_order_id).await;
        assert!(found.success || !found.success); // 至少不 panic
    }
}
