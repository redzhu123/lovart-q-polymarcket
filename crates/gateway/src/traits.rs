//! ExchangeGateway Trait（V1.08 第一节）。
//!
//! 统一所有交易所的接口。
//! Execution 只能通过此 Trait 调用，禁止直接 HTTP / Provider。
//!
//! 安全：默认 DryRun — 真实下单必须 `enable_live=true`。

use async_trait::async_trait;
use chrono::{DateTime, Local};

use crate::types::{Balance, GatewayInfo, GatewayResult, OrderRequest, Position};

// ============================================================================
// ExchangeGateway Trait
// ============================================================================

/// 交易所网关 Trait（V1.08 第一节）。
///
/// 统一所有交易所（Polymarket / Kalshi / DEX / CEX）的订单/余额/持仓接口。
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
    // ---- 元信息 ----

    /// Gateway 名称（如 "PolymarketGateway"）。
    fn name(&self) -> &str;

    /// Gateway 类型标识（如 "polymarket"）。
    fn gateway_type(&self) -> &str;

    /// 是否启用真实交易。
    fn live_enabled(&self) -> bool;

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
