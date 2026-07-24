//! ExchangeGateway Trait（P2-03 扩展版）。
//!
//! 统一所有交易所的接口。
//! Execution 只能通过此 Trait 调用，禁止直接 HTTP / Provider。
//!
//! 安全：默认 DryRun — 真实下单必须 `enable_live=true`。

use async_trait::async_trait;
use chrono::{DateTime, Local};

use crate::error::GatewayError;
use crate::types::{
    Balance, GatewayInfo, GatewayResult, Market, OrderBook, OrderRequest, Position,
};

// ============================================================================
// ExchangeGateway Trait
// ============================================================================

/// 交易所网关 Trait（P2-03 扩展版）。
///
/// 统一所有交易所（Polymarket / Kalshi / DEX / CEX）的订单/余额/持仓/市场/订单簿接口。
/// Execution 只能通过此 Trait 调用，禁止直接 HTTP / Provider。
///
/// # 安全
///
/// - 默认 DryRun：`enable_live` 为 `false` 时，`submit_order` 必须直接拒绝。
/// - 真实下单必须显式配置 `enable_live=true`。
///
/// # 实现
///
/// - [`MockGateway`](crate::mock::MockGateway)：模拟网关（Paper / Replay / Test）。
/// - [`PolymarketGateway`](crate::polymarket::PolymarketGateway)：Polymarket API 网关。
/// - 未来：KalshiGateway / DexGateway / CexGateway。
#[async_trait]
pub trait ExchangeGateway: Send + Sync {
    // ---- 生命周期 ----

    /// 连接 Gateway（初始化传输层、认证等）。
    ///
    /// 默认实现为空操作，子类型可覆写。
    async fn connect(&self) -> Result<(), GatewayError> {
        Ok(())
    }

    /// 断开 Gateway 连接（清理资源）。
    ///
    /// 默认实现为空操作，子类型可覆写。
    async fn disconnect(&self) -> Result<(), GatewayError> {
        Ok(())
    }

    // ---- 元信息 ----

    /// Gateway 名称（如 "PolymarketGateway"）。
    fn name(&self) -> &str;

    /// Gateway 类型标识（如 "polymarket"）。
    fn gateway_type(&self) -> &str;

    /// 是否启用真实交易。
    fn live_enabled(&self) -> bool;

    // ---- 市场数据 ----

    /// 获取市场列表。
    ///
    /// 默认返回空列表，子类型可覆写。
    async fn get_markets(&self) -> Result<Vec<Market>, GatewayError> {
        Ok(Vec::new())
    }

    /// 获取订单簿。
    ///
    /// 默认返回错误，子类型可覆写。
    async fn get_orderbook(&self, _market_id: &str) -> Result<OrderBook, GatewayError> {
        Err(GatewayError::exchange("此 Gateway 不支持查询订单簿"))
    }

    // ---- 订单操作 ----

    /// 提交订单到交易所。
    ///
    /// # 安全
    ///
    /// 当 `live_enabled() == false` 时，必须返回 `GatewayResult::rejected(..., "当前为 DryRun 模式，禁止真实下单")`。
    async fn submit_order(&self, request: &OrderRequest, now: DateTime<Local>) -> GatewayResult;

    /// 取消订单。
    async fn cancel_order(&self, order_id: &str) -> GatewayResult;

    /// 替换订单（取消旧订单 + 提交新订单）。
    async fn replace_order(
        &self,
        old_order_id: &str,
        new_request: &OrderRequest,
        now: DateTime<Local>,
    ) -> GatewayResult;

    /// 查询单个订单。
    async fn get_order(&self, order_id: &str) -> GatewayResult;

    /// 查询所有活跃订单。
    async fn list_orders(&self) -> Vec<GatewayResult>;

    // ---- WebSocket 订阅 ----

    /// 订阅频道（如 `book:<market_id>`、`trades:<market_id>`）。
    ///
    /// 默认实现为空操作，子类型可覆写。
    async fn subscribe(&self, _channel: &str) -> Result<(), GatewayError> {
        Ok(())
    }

    /// 取消订阅频道。
    ///
    /// 默认实现为空操作，子类型可覆写。
    async fn unsubscribe(&self, _channel: &str) -> Result<(), GatewayError> {
        Ok(())
    }

    // ---- 账户 ----

    /// 获取账户余额。
    async fn get_balance(&self) -> anyhow::Result<Balance>;

    /// 获取所有持仓。
    async fn get_positions(&self) -> anyhow::Result<Vec<Position>>;

    // ---- 健康检查 ----

    /// Ping（快速连通性测试）。
    async fn ping(&self) -> bool;

    /// 完整健康检查。
    async fn health(&self) -> GatewayInfo;

    // ---- 信息 ----

    /// Gateway 信息摘要。
    fn info(&self) -> GatewayInfo;
}
