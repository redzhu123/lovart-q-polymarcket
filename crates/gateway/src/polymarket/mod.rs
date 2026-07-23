//! Polymarket Gateway（V1.08 第三节）。
//!
//! 负责所有 Polymarket API：REST / WebSocket / 订单 / 余额 / 持仓 / 状态同步。
//! 所有 Polymarket 逻辑封装在此，禁止泄漏到 Execution。
//!
//! # 安全
//!
//! 默认 DryRun — `enable_live=false` 时，`submit_order` 直接拒绝。

pub mod rest;
pub mod types;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use std::sync::Mutex;
use tracing;

use super::adapter::to_polymarket_side;
use super::config::GatewayConfig;
use super::metrics::GatewayMetrics;
use super::retry::{CircuitBreaker, RetryExecutor};
use super::traits::ExchangeGateway;
use super::types::{Balance, GatewayInfo, GatewayResult, OrderRequest, Position};

use self::rest::PolymarketRestClient;

// ============================================================================
// PolymarketGateway
// ============================================================================

/// Polymarket Gateway（V1.08 第三节）。
///
/// 封装所有 Polymarket CLOB API 交互。
/// 当前实现：REST API 接口框架，WS 留待后续版本。
///
/// # 安全
///
/// - `enable_live=false`（默认）：所有 `submit_order` 直接拒绝，返回 DryRun 提示。
/// - `enable_live=true`：真实提交订单到 Polymarket API。
pub struct PolymarketGateway {
    /// REST API 客户端。
    rest: PolymarketRestClient,
    /// Gateway 配置。
    config: GatewayConfig,
    /// 指标收集器。
    metrics: Mutex<GatewayMetrics>,
    /// 重试执行器。
    #[allow(dead_code)]
    retry: Mutex<RetryExecutor>,
    /// 断路器。
    breaker: Mutex<CircuitBreaker>,
    /// 账户 ID。
    account_id: String,
    /// 累计订单数。
    total_orders: Mutex<u64>,
    /// 累计成交数。
    total_fills: Mutex<u64>,
}

impl PolymarketGateway {
    /// 创建新的 PolymarketGateway。
    pub fn new(config: GatewayConfig) -> Self {
        let rest = PolymarketRestClient::new(&config);
        let has_key = rest.has_api_key();

        if config.enable_live && !has_key {
            tracing::warn!(
                env_var = %config.api_key_env,
                "真实交易模式已启用但未设置 API 密钥！订单将失败。"
            );
        }

        tracing::info!(
            live = %config.enable_live,
            has_api_key = %has_key,
            base_url = %config.polymarket_api_url,
            "PolymarketGateway 已创建"
        );

        Self {
            rest,
            retry: Mutex::new(RetryExecutor::from_config(&config)),
            breaker: Mutex::new(CircuitBreaker::from_config(&config)),
            metrics: Mutex::new(GatewayMetrics::new()),
            config,
            account_id: "polymarket-account".to_string(),
            total_orders: Mutex::new(0),
            total_fills: Mutex::new(0),
        }
    }

    /// 带账户 ID 创建。
    pub fn with_account(mut self, account_id: &str) -> Self {
        self.account_id = account_id.to_string();
        self
    }

    /// 获取 REST 客户端引用。
    pub fn rest_client(&self) -> &PolymarketRestClient {
        &self.rest
    }
}

#[async_trait]
impl ExchangeGateway for PolymarketGateway {
    fn name(&self) -> &str {
        "PolymarketGateway"
    }

    fn gateway_type(&self) -> &str {
        "polymarket"
    }

    fn live_enabled(&self) -> bool {
        self.config.enable_live
    }

    async fn submit_order(&self, request: &OrderRequest, _now: DateTime<Local>) -> GatewayResult {
        let start = std::time::Instant::now();

        // 安全检查：DryRun 模式拒绝
        if !self.config.enable_live {
            tracing::warn!(
                market = %request.market_id,
                side = %request.side.as_str(),
                "🔒 DryRun 模式 — 禁止真实下单"
            );
            return GatewayResult::rejected(
                &request.client_order_id,
                "当前为 DryRun 模式，禁止真实下单。设置 enable_live=true 以启用真实交易。",
                start.elapsed().as_millis() as u64,
            );
        }

        // 断路器检查
        {
            let mut breaker = self.breaker.lock().unwrap();
            if !breaker.allow_request() {
                tracing::warn!("断路器已打开，拒绝下单请求");
                return GatewayResult::rejected(
                    &request.client_order_id,
                    "断路器已打开，暂时拒绝所有请求",
                    start.elapsed().as_millis() as u64,
                );
            }
        }

        tracing::info!(
            market = %request.market_id,
            side = %request.side.as_str(),
            price = %request.price,
            qty = %request.quantity,
            "⚠️ 真实下单 — Polymarket API"
        );

        // 调用 REST API
        let side_str = to_polymarket_side(request.direction, request.side);
        let order_type_str = match request.order_type {
            super::types::OrderType::Market => "GTC", // Polymarket 暂无视市价单
            super::types::OrderType::Limit => "GTC",
        };

        let result = self.rest.create_order(
            &request.market_id,
            request.price,
            request.quantity,
            side_str,
            order_type_str,
        ).await;

        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(order_json) => {
                // 更新指标
                {
                    let mut m = self.metrics.lock().unwrap();
                    m.record_api_call(latency_ms, true);
                    m.record_order_submitted();
                    *self.total_orders.lock().unwrap() += 1;
                }
                // 断路器记录成功
                self.breaker.lock().unwrap().record_success();

                let gw_result = order_json.to_gateway_result();

                tracing::info!(
                    order_id = %gw_result.gateway_order_id,
                    status = %gw_result.status.as_zh(),
                    latency_ms = %latency_ms,
                    "Polymarket 下单完成"
                );

                gw_result
            }
            Err(err) => {
                // 更新指标
                {
                    let mut m = self.metrics.lock().unwrap();
                    m.record_api_call(latency_ms, false);
                    m.record_order_rejected();
                }
                // 断路器记录失败
                self.breaker.lock().unwrap().record_failure();

                tracing::error!(
                    error = %err,
                    latency_ms = %latency_ms,
                    "Polymarket 下单失败"
                );

                GatewayResult::failed(&request.client_order_id, &err, latency_ms)
            }
        }
    }

    async fn cancel_order(&self, order_id: &str) -> GatewayResult {
        let start = std::time::Instant::now();

        if !self.config.enable_live {
            tracing::warn!("🔒 DryRun 模式 — 取消订单未发送");
            return GatewayResult::cancelled(order_id, "DryRun 模式（未真实取消）", 0);
        }

        match self.rest.cancel_order(order_id).await {
            Ok(order_json) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, true);
                self.breaker.lock().unwrap().record_success();

                tracing::info!(order_id = %order_id, "Polymarket 订单已取消");
                order_json.to_gateway_result()
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, false);
                self.breaker.lock().unwrap().record_failure();

                tracing::error!(order_id = %order_id, error = %err, "取消订单失败");
                GatewayResult::failed(order_id, &err, latency_ms)
            }
        }
    }

    async fn replace_order(
        &self,
        old_order_id: &str,
        new_request: &OrderRequest,
        now: DateTime<Local>,
    ) -> GatewayResult {
        tracing::info!(old = %old_order_id, "替换订单");

        let cancel = self.cancel_order(old_order_id).await;
        if !cancel.success {
            return cancel;
        }

        self.submit_order(new_request, now).await
    }

    async fn get_order(&self, order_id: &str) -> GatewayResult {
        let start = std::time::Instant::now();

        match self.rest.get_order(order_id).await {
            Ok(order_json) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, true);
                order_json.to_gateway_result()
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, false);
                GatewayResult::failed(order_id, &err, latency_ms)
            }
        }
    }

    async fn list_orders(&self) -> Vec<GatewayResult> {
        let start = std::time::Instant::now();

        match self.rest.list_orders().await {
            Ok(orders) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, true);
                orders.iter().map(|o| o.to_gateway_result()).collect()
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, false);
                tracing::warn!(error = %err, "查询订单列表失败");
                Vec::new()
            }
        }
    }

    async fn get_balance(&self) -> anyhow::Result<Balance> {
        let start = std::time::Instant::now();

        match self.rest.get_balance().await {
            Ok(balance_json) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, true);
                Ok(balance_json.to_balance(&self.account_id))
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, false);
                anyhow::bail!("Polymarket 余额查询失败: {} ({}ms)", err, latency_ms)
            }
        }
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        let start = std::time::Instant::now();

        match self.rest.get_positions().await {
            Ok(positions_json) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, true);
                Ok(positions_json.iter().map(|p| p.to_position()).collect())
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                self.metrics.lock().unwrap().record_api_call(latency_ms, false);
                anyhow::bail!("Polymarket 持仓查询失败: {} ({}ms)", err, latency_ms)
            }
        }
    }

    async fn ping(&self) -> bool {
        self.rest.ping().await
    }

    async fn health(&self) -> GatewayInfo {
        let ping_ok = self.rest.ping().await;
        let metrics = self.metrics.lock().unwrap();
        let breaker = self.breaker.lock().unwrap();
        let total_orders = *self.total_orders.lock().unwrap();
        let total_fills = *self.total_fills.lock().unwrap();

        let healthy = ping_ok && breaker.state() != super::retry::CircuitState::Open;

        let connection_status = if ping_ok {
            format!(
                "连接正常 | API: {} | Rate Limit 剩余: {:.0}%",
                self.rest.base_url(),
                metrics.http_success_rate() * 100.0,
            )
        } else {
            format!("API 不可达: {}", self.rest.base_url())
        };

        GatewayInfo {
            name: "PolymarketGateway".to_string(),
            gateway_type: "polymarket".to_string(),
            live_enabled: self.config.enable_live,
            healthy,
            api_latency_ms: metrics.last_api_latency_ms,
            http_success_rate: metrics.http_success_rate(),
            ws_connected: false, // WebSocket 下一版本实现
            rate_limit_remaining: metrics.http_success_rate() * 100.0,
            total_orders,
            total_fills,
            connection_status,
        }
    }

    fn info(&self) -> GatewayInfo {
        let metrics = self.metrics.lock().unwrap();
        let breaker = self.breaker.lock().unwrap();
        let total_orders = *self.total_orders.lock().unwrap();
        let total_fills = *self.total_fills.lock().unwrap();

        GatewayInfo {
            name: "PolymarketGateway".to_string(),
            gateway_type: "polymarket".to_string(),
            live_enabled: self.config.enable_live,
            healthy: breaker.state() != super::retry::CircuitState::Open,
            api_latency_ms: metrics.last_api_latency_ms,
            http_success_rate: metrics.http_success_rate(),
            ws_connected: false,
            rate_limit_remaining: metrics.http_success_rate() * 100.0,
            total_orders,
            total_fills,
            connection_status: if self.config.enable_live {
                "Polymarket 真实交易".to_string()
            } else {
                "Polymarket DryRun".to_string()
            },
        }
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

    fn test_config() -> GatewayConfig {
        GatewayConfig {
            gateway_type: "polymarket".into(),
            enable_live: false,
            ..GatewayConfig::default()
        }
    }

    #[tokio::test]
    async fn polymarket_gateway_dry_run_rejects_orders() {
        let gw = PolymarketGateway::new(test_config());
        let req = OrderRequest::new(
            "mkt-1", Direction::Yes, Side::Buy, 0.45, 100.0, "S", "R", "O",
        );

        let result = gw.submit_order(&req, Local::now()).await;
        assert!(!result.success);
        assert!(result.message.contains("DryRun"));
    }

    #[tokio::test]
    async fn polymarket_gateway_never_live_by_default() {
        let gw = PolymarketGateway::new(GatewayConfig::default());
        assert!(!gw.live_enabled());
    }

    #[tokio::test]
    async fn polymarket_gateway_info() {
        let gw = PolymarketGateway::new(test_config());
        let info = gw.info();
        assert_eq!(info.gateway_type, "polymarket");
        assert!(!info.live_enabled);
    }

    #[test]
    fn polymarket_gateway_name() {
        let gw = PolymarketGateway::new(test_config());
        assert_eq!(gw.name(), "PolymarketGateway");
        assert_eq!(gw.gateway_type(), "polymarket");
    }
}
