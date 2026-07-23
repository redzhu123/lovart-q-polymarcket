//! Mock Provider（V1.02 第二节 / 第十三节测试）。
//!
//! 返回内置的 `UnifiedMarket` 集合，不触网。用于：
//! - 单元测试（验证 Trait 分发、机会识别、Validator、Cache 等）。
//! - `cargo run -- datasource` 离线诊断（无需代理即可演示数据源能力）。
//!
//! 能力声明全开（除 trades），故可演示订单簿/买卖价路径（数据为构造的假数据，
//! 仅用于测试，**绝不混入真实扫描** -- 真实扫描用 gamma/clob）。

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use pm_models::{
    MarketStatus, OrderBook, PriceQuote, ProviderCapability, UnifiedMarket,
};

use crate::datasource::{HealthProbe, MarketDataProvider};
use crate::stats::{FetchResult, FetchStats};

/// Mock Provider。
pub struct MockProvider {
    markets: Vec<UnifiedMarket>,
    capability: ProviderCapability,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self {
            markets: default_markets(),
            capability: mock_capability(),
        }
    }
}

impl MockProvider {
    /// 用自定义市场集构造（测试用）。
    pub fn new(markets: Vec<UnifiedMarket>) -> Self {
        Self {
            markets,
            capability: mock_capability(),
        }
    }

    /// 当前持有的市场集快照。
    pub fn markets(&self) -> &[UnifiedMarket] {
        &self.markets
    }
}

/// Mock 能力：除 trades 外全开（测试/演示用）。
fn mock_capability() -> ProviderCapability {
    ProviderCapability {
        supports_markets: true,
        supports_orderbook: true,
        supports_trades: false,
        supports_bid_ask: true,
        supports_liquidity: true,
        depth_levels: 5,
        supports_depth: true,
    }
}

/// 默认内置市场集：含 1 个套利样本 + 2 个归一化 + 1 个缺价 + 1 个已关闭。
fn default_markets() -> Vec<UnifiedMarket> {
    let now = Utc::now();
    vec![
        unified("mock-arb", "套利样本A", Some(0.40), Some(0.55), MarketStatus::Active, now),
        unified("mock-norm1", "归一化样本B", Some(0.43), Some(0.57), MarketStatus::Active, now),
        unified("mock-norm2", "归一化样本C", Some(0.50), Some(0.50), MarketStatus::Active, now),
        unified("mock-noprice", "缺价样本D", None, None, MarketStatus::Active, now),
        unified("mock-closed", "已关闭样本E", Some(0.30), Some(0.70), MarketStatus::Closed, now),
    ]
}

fn unified(
    id: &str,
    question: &str,
    yes: Option<f64>,
    no: Option<f64>,
    status: MarketStatus,
    now: chrono::DateTime<Utc>,
) -> UnifiedMarket {
    UnifiedMarket {
        market_id: id.into(),
        question: question.into(),
        description: None,
        status,
        yes_price: yes,
        no_price: no,
        volume: 1000.0,
        liquidity: 500.0,
        category: Some("mock".into()),
        outcome_count: if yes.is_some() && no.is_some() { 2 } else { 0 },
        provider: "mock".into(),
        updated_at: now,
    }
}

#[async_trait]
impl MarketDataProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn capability(&self) -> ProviderCapability {
        self.capability
    }

    async fn fetch_markets(&self) -> Result<FetchResult> {
        // 无 HTTP：stats 全零，仅 markets 有值。
        Ok(FetchResult {
            markets: self.markets.clone(),
            stats: FetchStats::default(),
        })
    }

    async fn fetch_orderbooks(&self, market_ids: &[String]) -> Result<Vec<OrderBook>> {
        use pm_models::PriceLevel;

        let now = Utc::now();
        let out: Vec<OrderBook> = market_ids
            .iter()
            .map(|id| {
                // 对已知套利样本给出多档盘口；其余给 None（不伪造未知市场的真实深度）。
                if id == "mock-arb" {
                    let bid_levels = vec![
                        PriceLevel { price: 0.39, size: 50.0, level: 1 },
                        PriceLevel { price: 0.38, size: 100.0, level: 2 },
                        PriceLevel { price: 0.37, size: 200.0, level: 3 },
                        PriceLevel { price: 0.36, size: 150.0, level: 4 },
                        PriceLevel { price: 0.35, size: 100.0, level: 5 },
                    ];
                    let ask_levels = vec![
                        PriceLevel { price: 0.41, size: 60.0, level: 1 },
                        PriceLevel { price: 0.42, size: 80.0, level: 2 },
                        PriceLevel { price: 0.43, size: 120.0, level: 3 },
                    ];
                    let bid_vol: f64 = bid_levels.iter().map(|l| l.size).sum();
                    let ask_vol: f64 = ask_levels.iter().map(|l| l.size).sum();
                    OrderBook {
                        market_id: id.clone(),
                        best_bid: Some(0.39),
                        best_ask: Some(0.41),
                        spread: OrderBook::compute_spread(Some(0.39), Some(0.41)),
                        bid_depth: Some(bid_vol),
                        ask_depth: Some(ask_vol),
                        bid_levels,
                        ask_levels,
                        bid_volume: bid_vol,
                        ask_volume: ask_vol,
                        timestamp: now,
                        provider: "mock".into(),
                    }
                } else {
                    OrderBook::empty(id, "mock")
                }
            })
            .collect();
        Ok(out)
    }

    async fn fetch_prices(&self, market_ids: &[String]) -> Result<Vec<PriceQuote>> {
        let now = Utc::now();
        let out: Vec<PriceQuote> = market_ids
            .iter()
            .map(|id| {
                let (yes, no) = self
                    .markets
                    .iter()
                    .find(|m| m.market_id == *id)
                    .map(|m| (m.yes_price, m.no_price))
                    .unwrap_or((None, None));
                PriceQuote {
                    market_id: id.clone(),
                    yes_price: yes,
                    no_price: no,
                    timestamp: now,
                    provider: "mock".into(),
                }
            })
            .collect();
        Ok(out)
    }

    async fn health_check(&self) -> Result<HealthProbe> {
        Ok(HealthProbe {
            ok: true,
            status: 200,
            market_count: self.markets.len(),
            latency_ms: 0,
            detail: format!("Mock Provider 就绪，内置 {} 个市场", self.markets.len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_markets_contain_arbitrage_and_edge_cases() {
        let p = MockProvider::default();
        let r = p.fetch_markets().await.expect("fetch");
        assert_eq!(r.markets.len(), 5);
        // 套利样本 SUM=0.95 < 0.99
        let arb = r.markets.iter().find(|m| m.market_id == "mock-arb").unwrap();
        let sum = arb.yes_price.unwrap() + arb.no_price.unwrap();
        assert!(sum < 0.99);
        // 含缺价 / 已关闭
        assert!(r.markets.iter().any(|m| m.market_id == "mock-noprice" && m.yes_price.is_none()));
        assert!(r.markets.iter().any(|m| m.market_id == "mock-closed" && m.closed()));
    }

    #[tokio::test]
    async fn orderbook_for_arb_has_bid_ask_and_levels() {
        let p = MockProvider::default();
        let obs = p.fetch_orderbooks(&["mock-arb".into()]).await.expect("ob");
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].best_bid, Some(0.39));
        assert_eq!(obs[0].best_ask, Some(0.41));
        let spread = obs[0].spread.expect("spread");
        assert!((spread - 0.02).abs() < 1e-9);
        // 多档盘口
        assert_eq!(obs[0].bid_levels.len(), 5);
        assert_eq!(obs[0].ask_levels.len(), 3);
        assert!((obs[0].bid_volume - 600.0).abs() < 1e-9); // 50+100+200+150+100
        assert!((obs[0].ask_volume - 260.0).abs() < 1e-9); // 60+80+120
    }

    #[tokio::test]
    async fn orderbook_unknown_market_no_fake_depth() {
        let p = MockProvider::default();
        let obs = p.fetch_orderbooks(&["unknown".into()]).await.expect("ob");
        assert_eq!(obs.len(), 1);
        // 未知市场不伪造深度
        assert!(obs[0].best_bid.is_none());
        assert!(obs[0].best_ask.is_none());
        assert!(obs[0].spread.is_none());
    }

    #[tokio::test]
    async fn health_check_ok() {
        let p = MockProvider::default();
        let h = p.health_check().await.expect("health");
        assert!(h.ok);
        assert_eq!(h.market_count, 5);
    }

    #[test]
    fn capability_supports_real_arbitrage() {
        let p = MockProvider::default();
        let cap = p.capability();
        assert!(cap.supports_orderbook);
        assert!(cap.supports_bid_ask);
        assert!(cap.supports_real_arbitrage());
        assert_eq!(cap.depth_levels, 5);
        assert!(cap.supports_depth);
    }
}
