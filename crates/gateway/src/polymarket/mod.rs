//! Polymarket Gateway（P2-03 重构版）。
//!
//! 负责所有 Polymarket API：REST / WebSocket / 订单 / 余额 / 持仓 / 市场 / 订单簿。
//! 所有 Polymarket 逻辑封装在此，禁止泄漏到 Execution。
//!
//! # 安全
//!
//! 默认 DryRun — `enable_live=false` 时，`submit_order` 直接拒绝。
//!
//! # 架构
//!
//! CLI → ExchangeGateway Trait → PolymarketGateway → MiddlewareStack → Transport → API
//!
//! # P2-02 Workflow 集成
//!
//! 内部调用流程遵循 P2-02 状态机定义。

pub mod rest;
pub mod types;

use async_trait::async_trait;
use chrono::{DateTime, Local};
use std::sync::Arc;
use std::sync::Mutex;
use tracing;

use super::adapter::to_polymarket_side;
use super::config::GatewayConfig;
use super::error::GatewayError;
use super::metrics::GatewayMetrics;
use super::middleware::{self, MiddlewareContext, MiddlewareStack};
use super::traits::ExchangeGateway;
use super::transport::rest::{HttpRequest, HttpTransport, ReqwestTransport};
use super::transport::websocket::{NoopWsTransport, WsTransport};
use super::types::{
    Balance, BookLevel, GatewayInfo, GatewayResult, Market, OrderBook, OrderRequest, Position,
};

use crate::auth::{AuthProvider, PolymarketAuth};
use crate::ratelimit::RateLimiter;

// ============================================================================
// PolymarketGateway
// ============================================================================

/// Polymarket Gateway（P2-03 重构版）。
///
/// 封装所有 Polymarket CLOB API 交互。
/// 使用 Transport + Middleware 架构，禁止直接访问 HTTP。
///
/// # 安全
///
/// - `enable_live=false`（默认）：所有 `submit_order` 直接拒绝，返回 DryRun 提示。
/// - `enable_live=true`：真实提交订单到 Polymarket API。
pub struct PolymarketGateway {
    /// HTTP 传输层（包装 P2-01 ApiClient）。
    transport: Arc<ReqwestTransport>,
    /// WebSocket 传输层（占位实现）。
    ws_transport: Arc<dyn WsTransport>,
    /// 中间件栈。
    middleware: Arc<MiddlewareStack>,
    /// 速率限制器。
    rate_limiter: Arc<RateLimiter>,
    /// 认证提供者。
    #[allow(dead_code)]
    auth: Arc<PolymarketAuth>,
    /// Gateway 配置。
    config: GatewayConfig,
    /// 指标收集器。
    metrics: Arc<Mutex<GatewayMetrics>>,
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
        // 构建 ApiTestConfig（bridge）
        let api_config = config.to_api_test_config();

        // 创建 Transport 层
        let transport = Arc::new(ReqwestTransport::new(api_config));

        // 创建 WebSocket Transport（占位）
        let ws_url = config.polymarket_ws_url.clone();
        let ws_transport: Arc<dyn WsTransport> = Arc::new(NoopWsTransport::new(&ws_url));

        // 创建速率限制器
        let rate_limiter = Arc::new(RateLimiter::new(
            config.rate_limit_per_sec,
            config.rate_limit_per_min,
        ));

        // 创建认证提供者
        let auth = Arc::new(PolymarketAuth::new(&config.api_key_env));

        // 创建中间件栈
        let middleware = Arc::new(
            MiddlewareStack::new()
                .with(Box::new(middleware::logger::RequestLogger::new()))
                .with(Box::new(middleware::auth::AuthMiddleware::new(
                    auth.clone() as Arc<dyn crate::auth::AuthProvider>,
                )))
                .with(Box::new(middleware::ratelimit::RateLimitMiddleware::new(
                    rate_limiter.clone(),
                )))
                .with(Box::new(middleware::metrics::MetricsMiddleware::new(
                    GatewayMetrics::new(),
                )))
                .with(Box::new(middleware::tracing_mw::TracingMiddleware::new())),
        );

        let has_key = auth.as_ref().is_authenticated();

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
            "PolymarketGateway 已创建（P2-03 Transport + Middleware 架构）"
        );

        Self {
            transport,
            ws_transport,
            middleware,
            rate_limiter,
            auth,
            metrics: Arc::new(Mutex::new(GatewayMetrics::new())),
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

    /// 执行中间件链 + HTTP 请求。
    ///
    /// 遵循 P2-02 Workflow 状态机：
    /// 1. 构建请求上下文（MiddlewareContext）
    /// 2. 执行 before 钩子（Logger / Auth / RateLimit / Tracing）
    /// 3. 发送 HTTP 请求（Transport）
    /// 4. 执行 after 钩子（Logger / Metrics / Tracing）
    /// 5. 错误时执行 on_error 钩子
    async fn send_with_middleware(
        &self,
        req: HttpRequest,
    ) -> Result<super::transport::rest::HttpResponse, GatewayError> {
        let start = std::time::Instant::now();

        let ctx = MiddlewareContext::new(
            &req.request_id,
            req.method.as_str(),
            &req.path,
            "PolymarketGateway",
        );

        // 1. before 钩子
        self.middleware.run_before(&ctx).await;

        // 2. 发送请求
        match self.transport.send(req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let ctx = ctx.with_response(resp.status, latency_ms);

                // 3. after 钩子
                self.middleware.run_after(&ctx).await;
                Ok(resp)
            }
            Err(err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let ctx = ctx.with_response(0, latency_ms);

                // 4. error 钩子
                self.middleware.run_on_error(&err, &ctx).await;
                Err(err)
            }
        }
    }
}

#[async_trait]
impl ExchangeGateway for PolymarketGateway {
    // ---- 生命周期 ----

    async fn connect(&self) -> Result<(), GatewayError> {
        tracing::info!("PolymarketGateway 正在连接...");
        self.transport.connect().await?;
        self.ws_transport.connect().await?;
        tracing::info!("PolymarketGateway 已连接");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), GatewayError> {
        tracing::info!("PolymarketGateway 正在断开...");
        self.ws_transport.disconnect().await?;
        self.transport.disconnect().await?;
        tracing::info!("PolymarketGateway 已断开");
        Ok(())
    }

    // ---- 元信息 ----

    fn name(&self) -> &str {
        "PolymarketGateway"
    }

    fn gateway_type(&self) -> &str {
        "polymarket"
    }

    fn live_enabled(&self) -> bool {
        self.config.enable_live
    }

    // ---- 市场数据 ----

    async fn get_markets(&self) -> Result<Vec<Market>, GatewayError> {
        let req = HttpRequest::get("/markets");
        let resp = self.send_with_middleware(req).await?;

        if !resp.is_success() {
            return Err(GatewayError::exchange(format!(
                "获取市场列表失败: HTTP {}",
                resp.status
            )));
        }

        let markets: Vec<Market> = resp
            .body
            .as_array()
            .ok_or_else(|| GatewayError::serialization("市场数据不是数组格式"))?
            .iter()
            .filter_map(|m| {
                let market_id = m.get("condition_id")?.as_str()?.to_string();
                let question = m
                    .get("question")
                    .and_then(|q| q.as_str())
                    .unwrap_or("未知")
                    .to_string();
                let closed = m.get("closed").and_then(|c| c.as_bool()).unwrap_or(false);

                let tokens = m.get("tokens").and_then(|t| t.as_array());
                let mut yes_price = None;
                let mut no_price = None;
                if let Some(tokens) = tokens {
                    for token in tokens {
                        let outcome = token.get("outcome").and_then(|o| o.as_str()).unwrap_or("");
                        let price = token
                            .get("price")
                            .and_then(|p| p.as_str())
                            .and_then(|s| s.parse::<f64>().ok());
                        match outcome.to_lowercase().as_str() {
                            "yes" => yes_price = price,
                            "no" => no_price = price,
                            _ => {}
                        }
                    }
                }

                let status = if closed { "已关闭" } else { "开放" };

                Some(Market {
                    market_id,
                    question,
                    closed,
                    yes_price,
                    no_price,
                    volume: 0.0,
                    liquidity: 0.0,
                    status: status.to_string(),
                })
            })
            .collect();

        tracing::info!(count = %markets.len(), "市场列表获取成功");
        Ok(markets)
    }

    async fn get_orderbook(&self, market_id: &str) -> Result<OrderBook, GatewayError> {
        let path = format!("/book?token_id={}", market_id);
        let req = HttpRequest::get(&path);
        let resp = self.send_with_middleware(req).await?;

        if !resp.is_success() {
            return Err(GatewayError::exchange(format!(
                "获取订单簿失败: HTTP {}",
                resp.status
            )));
        }

        let bids: Vec<BookLevel> = resp
            .body
            .get("bids")
            .and_then(|b| b.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let price = b
                            .get("price")
                            .and_then(|p| p.as_str())?
                            .parse::<f64>()
                            .ok()?;
                        let size = b
                            .get("size")
                            .and_then(|s| s.as_str())?
                            .parse::<f64>()
                            .ok()?;
                        Some(BookLevel { price, size })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let asks: Vec<BookLevel> = resp
            .body
            .get("asks")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        let price = a
                            .get("price")
                            .and_then(|p| p.as_str())?
                            .parse::<f64>()
                            .ok()?;
                        let size = a
                            .get("size")
                            .and_then(|s| s.as_str())?
                            .parse::<f64>()
                            .ok()?;
                        Some(BookLevel { price, size })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let tick_size = resp
            .body
            .get("tick_size")
            .and_then(|t| t.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.01);

        tracing::info!(
            market_id,
            bids = %bids.len(),
            asks = %asks.len(),
            "订单簿获取成功"
        );

        Ok(OrderBook {
            market_id: market_id.to_string(),
            bids,
            asks,
            tick_size,
            updated_at: Some(Local::now()),
        })
    }

    // ---- 订单操作 ----

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

        // 速率限制检查
        let wait_ms = self.rate_limiter.acquire();
        if wait_ms > 0 {
            tracing::debug!(wait_ms, "下单前速率限制等待");
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        }

        tracing::info!(
            market = %request.market_id,
            side = %request.side.as_str(),
            price = %request.price,
            qty = %request.quantity,
            "⚠️ 真实下单 — Polymarket API"
        );

        let side_str = to_polymarket_side(request.direction, request.side);
        let body = serde_json::json!({
            "token_id": request.market_id,
            "price": format!("{:.4}", request.price),
            "size": format!("{:.2}", request.quantity),
            "side": side_str,
            "type": "GTC",
        });

        let http_req = HttpRequest::post("/order", body);
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if !http_resp.is_success() {
                    let mut m = self.metrics.lock().unwrap();
                    m.record_api_call(latency_ms, false);
                    m.record_order_rejected();
                    return GatewayResult::failed(
                        &request.client_order_id,
                        &format!("Polymarket 下单失败: HTTP {}", http_resp.status),
                        latency_ms,
                    );
                }

                let order_id = http_resp
                    .body
                    .get("id")
                    .and_then(|id| id.as_str())
                    .unwrap_or(&request.client_order_id)
                    .to_string();

                let mut m = self.metrics.lock().unwrap();
                m.record_api_call(latency_ms, true);
                m.record_order_submitted();
                *self.total_orders.lock().unwrap() += 1;

                tracing::info!(order_id = %order_id, latency_ms, "Polymarket 下单完成");
                GatewayResult::accepted(&order_id, "订单已提交到 Polymarket", latency_ms)
            }
            Err(err) => {
                let mut m = self.metrics.lock().unwrap();
                m.record_api_call(latency_ms, false);
                m.record_order_rejected();

                tracing::error!(error = %err, latency_ms, "Polymarket 下单失败");
                GatewayResult::failed(&request.client_order_id, &format!("{}", err), latency_ms)
            }
        }
    }

    async fn cancel_order(&self, order_id: &str) -> GatewayResult {
        let start = std::time::Instant::now();

        if !self.config.enable_live {
            tracing::warn!("🔒 DryRun 模式 — 取消订单未发送");
            return GatewayResult::cancelled(order_id, "DryRun 模式（未真实取消）", 0);
        }

        let path = format!("/order/{}", order_id);
        let http_req = HttpRequest::delete(&path);
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if http_resp.is_success() {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, true);
                    tracing::info!(order_id, "Polymarket 订单已取消");
                    GatewayResult::cancelled(order_id, "订单已取消", latency_ms)
                } else {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, false);
                    GatewayResult::failed(
                        order_id,
                        &format!("取消订单失败: HTTP {}", http_resp.status),
                        latency_ms,
                    )
                }
            }
            Err(err) => {
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, false);
                GatewayResult::failed(order_id, &format!("{}", err), latency_ms)
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
        let path = format!("/order/{}", order_id);
        let http_req = HttpRequest::get(&path);
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if http_resp.is_success() {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, true);
                    let status = http_resp
                        .body
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("UNKNOWN");
                    GatewayResult::accepted(order_id, &format!("订单状态: {}", status), latency_ms)
                } else {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, false);
                    GatewayResult::failed(
                        order_id,
                        &format!("查询订单失败: HTTP {}", http_resp.status),
                        latency_ms,
                    )
                }
            }
            Err(err) => {
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, false);
                GatewayResult::failed(order_id, &format!("{}", err), latency_ms)
            }
        }
    }

    async fn list_orders(&self) -> Vec<GatewayResult> {
        let start = std::time::Instant::now();
        let http_req = HttpRequest::get("/orders");
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if http_resp.is_success() {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, true);
                    http_resp
                        .body
                        .as_array()
                        .map(|orders| {
                            orders
                                .iter()
                                .map(|o| {
                                    let id =
                                        o.get("id").and_then(|i| i.as_str()).unwrap_or("unknown");
                                    let status = o
                                        .get("status")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("UNKNOWN");
                                    GatewayResult::accepted(
                                        id,
                                        &format!("状态: {}", status),
                                        latency_ms,
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, false);
                    Vec::new()
                }
            }
            Err(_) => {
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, false);
                Vec::new()
            }
        }
    }

    // ---- WebSocket 订阅 ----

    async fn subscribe(&self, channel: &str) -> Result<(), GatewayError> {
        tracing::info!(channel, "订阅频道");
        self.ws_transport.subscribe(channel).await
    }

    async fn unsubscribe(&self, channel: &str) -> Result<(), GatewayError> {
        tracing::info!(channel, "取消订阅");
        self.ws_transport.unsubscribe(channel).await
    }

    // ---- 账户 ----

    async fn get_balance(&self) -> anyhow::Result<Balance> {
        let start = std::time::Instant::now();
        let http_req = HttpRequest::get("/balance");
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if !http_resp.is_success() {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, false);
                    anyhow::bail!(
                        "Polymarket 余额查询失败: HTTP {} ({}ms)",
                        http_resp.status,
                        latency_ms
                    );
                }
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, true);

                let available = http_resp
                    .body
                    .get("balance")
                    .or_else(|| http_resp.body.get("available"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                Ok(Balance {
                    account_id: self.account_id.clone(),
                    available,
                    total: available,
                    locked: 0.0,
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    currency: "USDC".to_string(),
                    updated_at: Some(Local::now()),
                })
            }
            Err(err) => {
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, false);
                anyhow::bail!("Polymarket 余额查询失败: {} ({}ms)", err, latency_ms)
            }
        }
    }

    async fn get_positions(&self) -> anyhow::Result<Vec<Position>> {
        let start = std::time::Instant::now();
        let http_req = HttpRequest::get("/positions");
        let resp = self.send_with_middleware(http_req).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match resp {
            Ok(http_resp) => {
                if !http_resp.is_success() {
                    self.metrics
                        .lock()
                        .unwrap()
                        .record_api_call(latency_ms, false);
                    anyhow::bail!(
                        "Polymarket 持仓查询失败: HTTP {} ({}ms)",
                        http_resp.status,
                        latency_ms
                    );
                }
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, true);

                let positions: Vec<Position> = http_resp
                    .body
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|p| {
                                use pm_execution::order::Direction;
                                let size = p
                                    .get("size")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                let avg_price = p
                                    .get("avg_price")
                                    .or_else(|| p.get("average_price"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                let current_price = p
                                    .get("current_price")
                                    .or_else(|| p.get("cur_price"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);

                                Position {
                                    position_id: p
                                        .get("id")
                                        .or_else(|| p.get("position_id"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    market_id: p
                                        .get("condition_id")
                                        .or_else(|| p.get("market"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    question: String::new(),
                                    direction: Direction::Yes,
                                    quantity: size,
                                    avg_entry_price: avg_price,
                                    mark_price: current_price,
                                    unrealized_pnl: p
                                        .get("unrealized_pnl")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| s.parse::<f64>().ok())
                                        .unwrap_or(0.0),
                                    realized_pnl: p
                                        .get("realized_pnl")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| s.parse::<f64>().ok())
                                        .unwrap_or(0.0),
                                    cost_basis: size * avg_price,
                                    market_value: size * current_price,
                                    updated_at: Some(Local::now()),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                tracing::debug!(count = %positions.len(), "Polymarket 持仓查询成功");
                Ok(positions)
            }
            Err(err) => {
                self.metrics
                    .lock()
                    .unwrap()
                    .record_api_call(latency_ms, false);
                anyhow::bail!("Polymarket 持仓查询失败: {} ({}ms)", err, latency_ms)
            }
        }
    }

    // ---- 健康检查 ----

    async fn ping(&self) -> bool {
        let req = HttpRequest::get("/time");
        match self.send_with_middleware(req).await {
            Ok(resp) => resp.is_success(),
            Err(_) => false,
        }
    }

    async fn health(&self) -> GatewayInfo {
        let ping_ok = self.ping().await;
        let metrics = self.metrics.lock().unwrap();
        let total_orders = *self.total_orders.lock().unwrap();
        let total_fills = *self.total_fills.lock().unwrap();

        let healthy = ping_ok;

        let connection_status = if ping_ok {
            format!(
                "连接正常 | API: {} | Rate Limit 剩余: {:.0}%",
                self.transport.base_url(),
                self.rate_limiter.remaining() * 100.0,
            )
        } else {
            format!("API 不可达: {}", self.transport.base_url())
        };

        GatewayInfo {
            name: "PolymarketGateway".to_string(),
            gateway_type: "polymarket".to_string(),
            live_enabled: self.config.enable_live,
            healthy,
            api_latency_ms: metrics.last_api_latency_ms,
            http_success_rate: metrics.http_success_rate(),
            ws_connected: self.ws_transport.is_connected(),
            rate_limit_remaining: self.rate_limiter.remaining() * 100.0,
            total_orders,
            total_fills,
            connection_status,
        }
    }

    fn info(&self) -> GatewayInfo {
        let metrics = self.metrics.lock().unwrap();
        let total_orders = *self.total_orders.lock().unwrap();
        let total_fills = *self.total_fills.lock().unwrap();

        GatewayInfo {
            name: "PolymarketGateway".to_string(),
            gateway_type: "polymarket".to_string(),
            live_enabled: self.config.enable_live,
            healthy: self.transport.is_connected(),
            api_latency_ms: metrics.last_api_latency_ms,
            http_success_rate: metrics.http_success_rate(),
            ws_connected: self.ws_transport.is_connected(),
            rate_limit_remaining: self.rate_limiter.remaining() * 100.0,
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
    use pm_core::Side;
    use pm_execution::order::Direction;

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
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S",
            "R",
            "O",
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

    #[tokio::test]
    async fn polymarket_gateway_connect_disconnect() {
        let gw = PolymarketGateway::new(test_config());
        gw.connect().await.unwrap();
        gw.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn polymarket_gateway_subscribe_unsubscribe() {
        let gw = PolymarketGateway::new(test_config());
        gw.connect().await.unwrap();
        gw.subscribe("book:test").await.unwrap();
        gw.unsubscribe("book:test").await.unwrap();
    }

    #[test]
    fn polymarket_gateway_name() {
        let gw = PolymarketGateway::new(test_config());
        assert_eq!(gw.name(), "PolymarketGateway");
        assert_eq!(gw.gateway_type(), "polymarket");
    }
}
