//! Trading Provider（V1.07 第二节）。
//!
//! 统一所有交易平台的 Trait。
//! Execution 只能通过 Trading 调用，禁止直接 HTTP / WS。
//!
//! 当前实现：MockTradingProvider。
//! 未来实现：PolymarketTradingProvider。

use async_trait::async_trait;
use tracing::info;

use crate::capability::Capability;
use crate::connection::ConnectionManager;
use crate::credential::CredentialManager;
use crate::session::SessionManager;
use crate::state::TradingState;

// ============================================================================
// Health Status
// ============================================================================

/// 健康检查结果。
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// 是否健康。
    pub healthy: bool,
    /// Provider 名称。
    pub provider: String,
    /// 当前状态。
    pub state: TradingState,
    /// HTTP 连接状态。
    pub http_ok: bool,
    /// WebSocket 连接状态（如适用）。
    pub ws_ok: bool,
    /// Session 是否有效。
    pub session_valid: bool,
    /// 延迟（毫秒）。
    pub latency_ms: u64,
    /// 详情消息。
    pub detail: String,
}

impl HealthStatus {
    /// 健康摘要（中文）。
    pub fn summary_zh(&self) -> String {
        let status = if self.healthy {
            "✅ 健康"
        } else {
            "❌ 异常"
        };
        format!(
            "{} | Provider: {} | 状态: {} | HTTP: {} | WS: {} | Session: {} | 延迟: {}ms",
            status,
            self.provider,
            self.state.as_zh(),
            if self.http_ok { "✅" } else { "❌" },
            if self.ws_ok { "✅" } else { "❌" },
            if self.session_valid { "✅" } else { "❌" },
            self.latency_ms,
        )
    }
}

// ============================================================================
// Account Summary
// ============================================================================

/// 账户摘要。
#[derive(Debug, Clone, Default)]
pub struct AccountSummary {
    /// 账户 ID。
    pub account_id: String,
    /// 可用余额（USDC）。
    pub available_balance: f64,
    /// 总余额（USDC）。
    pub total_balance: f64,
    /// 持仓市值。
    pub position_value: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 保证金使用率。
    pub margin_usage: f64,
    /// 货币。
    pub currency: String,
}

impl AccountSummary {
    /// 空账户（Mock 用）。
    pub fn empty() -> Self {
        Self {
            account_id: "mock-account".to_string(),
            available_balance: 10000.0,
            total_balance: 10000.0,
            currency: "USDC".to_string(),
            ..Default::default()
        }
    }
}

// ============================================================================
// Market Info
// ============================================================================

/// 交易市场信息（Trading 层）。
#[derive(Debug, Clone)]
pub struct TradingMarket {
    /// 市场 ID。
    pub market_id: String,
    /// 问题/标题。
    pub question: String,
    /// 最优买价。
    pub best_bid: Option<f64>,
    /// 最优卖价。
    pub best_ask: Option<f64>,
    /// 最小下单量。
    pub min_size: f64,
    /// 价格精度（小数位）。
    pub price_precision: u32,
    /// 数量精度（小数位）。
    pub size_precision: u32,
    /// 是否可交易。
    pub tradable: bool,
}

// ============================================================================
// TradingProvider Trait
// ============================================================================

/// Trading Provider Trait（V1.07 第二节）。
///
/// 统一所有交易平台的接口。
/// 当前：MockTradingProvider。
/// 未来：PolymarketTradingProvider。
///
/// Execution 禁止直接 HTTP / WS — 只能通过 Trading 调用。
#[async_trait]
pub trait TradingProvider: Send + Sync {
    // ---- 连接管理 ----

    /// 连接到 Provider。
    async fn connect(&mut self) -> anyhow::Result<()>;

    /// 断开连接。
    async fn disconnect(&mut self) -> anyhow::Result<()>;

    /// 健康检查。
    async fn health(&self) -> HealthStatus;

    /// 心跳（周期调用）。
    async fn heartbeat(&mut self) -> anyhow::Result<()>;

    // ---- 能力 ----

    /// Provider 能力声明。
    fn capability(&self) -> &Capability;

    /// Provider 名称。
    fn name(&self) -> &str;

    // ---- 账户 ----

    /// 获取账户摘要。
    async fn account(&self) -> anyhow::Result<AccountSummary>;

    // ---- 市场 ----

    /// 获取可交易市场列表。
    async fn market(&self) -> anyhow::Result<Vec<TradingMarket>>;

    // ---- Gateway ----

    /// 获取底层 Gateway（供 Execution 使用）。
    /// 当前返回 MockGateway，未来返回 PolymarketGateway。
    fn gateway_name(&self) -> &str;

    // ---- 状态 ----

    /// 当前状态。
    fn state(&self) -> TradingState;

    /// 设置状态。
    fn set_state(&mut self, state: TradingState);
}

// ============================================================================
// MockTradingProvider
// ============================================================================

/// Mock Trading Provider（V1.07 第二节）。
///
/// 模拟 Trading Provider，不产生真实交易。
/// 所有操作均为 Dry Run。
pub struct MockTradingProvider {
    /// Provider 名称。
    name: String,
    /// 当前状态。
    state: TradingState,
    /// 能力声明。
    capability: Capability,
    /// 凭据管理器。
    pub credential_manager: CredentialManager,
    /// Session 管理器。
    pub session_manager: SessionManager,
    /// 连接管理器。
    pub connection_manager: ConnectionManager,
}

impl MockTradingProvider {
    /// 创建 Mock Provider。
    pub fn new() -> Self {
        let mut cm = ConnectionManager::new();
        cm.http_connecting();
        cm.http_connected();

        Self {
            name: "MockTradingProvider".to_string(),
            state: TradingState::Ready,
            capability: Capability::mock(),
            credential_manager: CredentialManager::new(),
            session_manager: SessionManager::with_defaults(),
            connection_manager: cm,
        }
    }

    /// 带名称的 Mock Provider。
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }
}

impl Default for MockTradingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TradingProvider for MockTradingProvider {
    async fn connect(&mut self) -> anyhow::Result<()> {
        self.state.transition_to(TradingState::Connecting);
        self.connection_manager.http_connecting();
        // 模拟连接成功
        self.connection_manager.http_connected();
        self.state.transition_to(TradingState::Connected);
        // 模拟认证
        self.state.transition_to(TradingState::Authenticated);
        self.state.transition_to(TradingState::Ready);
        info!("MockTradingProvider 已连接（模拟）");
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.connection_manager.http_disconnected();
        self.session_manager.destroy();
        self.state.transition_to(TradingState::Stopped);
        info!("MockTradingProvider 已断开（模拟）");
        Ok(())
    }

    async fn health(&self) -> HealthStatus {
        HealthStatus {
            healthy: true,
            provider: self.name.clone(),
            state: self.state,
            http_ok: self.connection_manager.http_ok(),
            ws_ok: self.connection_manager.ws_ok(),
            session_valid: self.session_manager.is_valid(),
            latency_ms: 1,
            detail: "Mock Provider 始终健康".to_string(),
        }
    }

    async fn heartbeat(&mut self) -> anyhow::Result<()> {
        self.session_manager.check_and_renew();
        info!("MockTradingProvider 心跳正常");
        Ok(())
    }

    fn capability(&self) -> &Capability {
        &self.capability
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn account(&self) -> anyhow::Result<AccountSummary> {
        Ok(AccountSummary::empty())
    }

    async fn market(&self) -> anyhow::Result<Vec<TradingMarket>> {
        Ok(vec![
            TradingMarket {
                market_id: "mock-1".into(),
                question: "Mock 市场 1：BTC 本周会突破 100k 吗？".into(),
                best_bid: Some(0.45),
                best_ask: Some(0.47),
                min_size: 1.0,
                price_precision: 4,
                size_precision: 2,
                tradable: true,
            },
            TradingMarket {
                market_id: "mock-2".into(),
                question: "Mock 市场 2：ETH 会涨到 5000 吗？".into(),
                best_bid: Some(0.30),
                best_ask: Some(0.32),
                min_size: 1.0,
                price_precision: 4,
                size_precision: 2,
                tradable: true,
            },
            TradingMarket {
                market_id: "mock-arb".into(),
                question: "Mock 套利样本：YES+NO=0.96 < 1.0".into(),
                best_bid: Some(0.52),
                best_ask: Some(0.54),
                min_size: 1.0,
                price_precision: 4,
                size_precision: 2,
                tradable: true,
            },
        ])
    }

    fn gateway_name(&self) -> &str {
        "mock"
    }

    fn state(&self) -> TradingState {
        self.state
    }

    fn set_state(&mut self, state: TradingState) {
        self.state.transition_to(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_connect_disconnect() {
        let mut provider = MockTradingProvider::new();
        assert_eq!(provider.state(), TradingState::Ready);

        provider.disconnect().await.unwrap();
        assert_eq!(provider.state(), TradingState::Stopped);

        provider.connect().await.unwrap();
        assert_eq!(provider.state(), TradingState::Ready);
    }

    #[tokio::test]
    async fn mock_provider_health() {
        let provider = MockTradingProvider::new();
        let health = provider.health().await;
        assert!(health.healthy);
        assert!(health.summary_zh().contains("健康"));
    }

    #[tokio::test]
    async fn mock_provider_heartbeat() {
        let mut provider = MockTradingProvider::new();
        assert!(provider.heartbeat().await.is_ok());
    }

    #[tokio::test]
    async fn mock_provider_account() {
        let provider = MockTradingProvider::new();
        let acc = provider.account().await.unwrap();
        assert!(acc.total_balance > 0.0);
        assert_eq!(acc.currency, "USDC");
    }

    #[tokio::test]
    async fn mock_provider_market() {
        let provider = MockTradingProvider::new();
        let markets = provider.market().await.unwrap();
        assert!(!markets.is_empty());
        assert!(markets.iter().any(|m| m.tradable));
    }

    #[test]
    fn mock_provider_capability_no_real_trading() {
        let provider = MockTradingProvider::new();
        let cap = provider.capability();
        assert!(!cap.can_real_trading);
        assert!(!cap.can_order);
    }

    #[test]
    fn mock_provider_gateway_name() {
        let provider = MockTradingProvider::new();
        assert_eq!(provider.gateway_name(), "mock");
    }
}
