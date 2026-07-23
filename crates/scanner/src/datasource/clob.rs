//! CLOB API Provider（V1.03 第一节）。
//!
//! 对接 Polymarket CLOB API，获取真实订单簿数据（多档盘口）。
//! 与 Gamma 不同，CLOB 不提供市场列表（使用 Gamma 获取市场后再用 CLOB 查订单簿）。
//!
//! 能力：订单簿 ✅ / 成交记录 ✅ / 最优买卖价 ✅ / 流动性 ✅ / 盘口深度 ✅（10 档）。
//! 不支持：市场列表 ❌（需 Gamma 配合使用）。
//!
//! 模拟研究专用 -- 不连接钱包 / 不签名 / 不下单 / 不真实交易。

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use pm_models::{OrderBook, PriceLevel, PriceQuote, ProviderCapability};

use crate::datasource::{HealthProbe, MarketDataProvider};
use crate::stats::{FetchResult, FetchStats};

/// Polymarket CLOB API 基础地址。
pub const CLOB_API_BASE: &str = "https://clob.polymarket.com";

/// CLOB API Provider。
///
/// 负责从 CLOB API 获取真实订单簿。需要配合 GammaProvider 使用：
/// Gamma 提供市场列表，CLOB 提供每个市场的订单簿深度。
pub struct ClobProvider {
    client: reqwest::Client,
    /// 是否输出 HTTP 调试信息。
    debug: bool,
}

impl ClobProvider {
    /// 用外部传入的 `reqwest::Client` 构造（复用连接池与代理配置）。
    pub fn new(client: reqwest::Client, debug: bool) -> Self {
        Self { client, debug }
    }

    /// CLOB 能力声明（V1.03 第二节 / 第八节）。
    pub fn capability_value() -> ProviderCapability {
        ProviderCapability {
            supports_markets: false,
            supports_orderbook: true,
            supports_trades: true,
            supports_bid_ask: true,
            supports_liquidity: true,
            depth_levels: 10,
            supports_depth: true,
        }
    }

    /// 解析 CLOB API 返回的订单簿 JSON 为 `OrderBook`。
    ///
    /// CLOB `/orderbook` 返回格式：
    /// ```json
    /// {"bids": [["0.45", "100.5"], ["0.44", "200.0"]], "asks": [["0.47", "80.0"], ...]}
    /// ```
    /// bids/asks 按价格从优到劣排列（bids 降序，asks 升序）。
    fn parse_orderbook(raw: &str, market_id: &str) -> Result<OrderBook> {
        let parsed: serde_json::Value = serde_json::from_str(raw)?;

        let bids_raw = parsed["bids"].as_array();
        let asks_raw = parsed["asks"].as_array();

        let parse_levels = |arr: Option<&Vec<serde_json::Value>>| -> Vec<PriceLevel> {
            arr.into_iter()
                .flatten()
                .filter_map(|entry| {
                    let pair = entry.as_array()?;
                    let price = pair.first()?.as_str().and_then(|s| s.parse::<f64>().ok())?;
                    let size = pair.get(1)?.as_str().and_then(|s| s.parse::<f64>().ok())?;
                    Some((price, size))
                })
                .enumerate()
                .map(|(i, (price, size))| PriceLevel {
                    price,
                    size,
                    level: i + 1,
                })
                .collect()
        };

        let bid_levels: Vec<PriceLevel> = parse_levels(bids_raw);
        let ask_levels: Vec<PriceLevel> = parse_levels(asks_raw);

        let best_bid = bid_levels.first().map(|l| l.price);
        let best_ask = ask_levels.first().map(|l| l.price);
        let spread = OrderBook::compute_spread(best_bid, best_ask);

        let bid_volume: f64 = bid_levels.iter().map(|l| l.size).sum();
        let ask_volume: f64 = ask_levels.iter().map(|l| l.size).sum();

        Ok(OrderBook {
            market_id: market_id.to_string(),
            best_bid,
            best_ask,
            spread,
            bid_depth: Some(bid_volume),
            ask_depth: Some(ask_volume),
            bid_levels,
            ask_levels,
            bid_volume,
            ask_volume,
            timestamp: Utc::now(),
            provider: "clob".into(),
        })
    }
}

#[async_trait]
impl MarketDataProvider for ClobProvider {
    fn name(&self) -> &str {
        "clob"
    }

    fn capability(&self) -> ProviderCapability {
        Self::capability_value()
    }

    /// CLOB 不提供市场列表，返回空。
    async fn fetch_markets(&self) -> Result<FetchResult> {
        tracing::debug!("CLOB Provider 不支持市场列表，返回空");
        Ok(FetchResult {
            markets: Vec::new(),
            stats: FetchStats::default(),
        })
    }

    /// 从 CLOB API 拉取订单簿（多档盘口）。
    ///
    /// 对每个 `market_id`（对应 CLOB 的 `token_id`）调用 `/orderbook`。
    /// 如果 API 返回错误或解析失败，该 market 返回空订单簿（不阻断其他）。
    async fn fetch_orderbooks(&self, market_ids: &[String]) -> Result<Vec<OrderBook>> {
        let mut results: Vec<OrderBook> = Vec::with_capacity(market_ids.len());

        for market_id in market_ids {
            let url = format!("{}/orderbook?token_id={}", CLOB_API_BASE, market_id);
            let start = Instant::now();

            match self
                .client
                .get(&url)
                .timeout(Duration::from_secs(15))
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed = start.elapsed().as_millis();
                    match resp.error_for_status() {
                        Ok(r) => match r.text().await {
                            Ok(body) => {
                                match Self::parse_orderbook(&body, market_id) {
                                    Ok(ob) => {
                                        if self.debug {
                                            tracing::debug!(
                                                market_id = %market_id,
                                                elapsed_ms = elapsed,
                                                bid_levels = ob.bid_levels.len(),
                                                ask_levels = ob.ask_levels.len(),
                                                "CLOB 订单簿拉取成功"
                                            );
                                        }
                                        results.push(ob);
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            market_id = %market_id,
                                            error = %e,
                                            "CLOB 订单簿 JSON 解析失败"
                                        );
                                        // 返回空订单簿（不伪造）
                                        results.push(OrderBook::empty(market_id, "clob"));
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    market_id = %market_id,
                                    error = %e,
                                    "CLOB 读取响应体失败"
                                );
                                results.push(OrderBook::empty(market_id, "clob"));
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                market_id = %market_id,
                                status = %e.status().map(|s| s.as_u16()).unwrap_or(0),
                                "CLOB HTTP 请求失败"
                            );
                            results.push(OrderBook::empty(market_id, "clob"));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        market_id = %market_id,
                        error = %e,
                        "CLOB 网络请求超时或连接失败"
                    );
                    results.push(OrderBook::empty(market_id, "clob"));
                }
            }
        }

        Ok(results)
    }

    /// CLOB 暂不支持按需价格查询，返回空。
    async fn fetch_prices(&self, _market_ids: &[String]) -> Result<Vec<PriceQuote>> {
        tracing::debug!("CLOB Provider 不支持按需价格查询，返回空");
        Ok(Vec::new())
    }

    /// 健康检查：用 `limit=1` 的市场查询订单簿。
    async fn health_check(&self) -> Result<HealthProbe> {
        // CLOB 没有 "limit=1" 的市场列表端点，用常见的 token_id 探测
        let test_url = format!("{}/orderbook?token_id=test", CLOB_API_BASE);
        let start = Instant::now();
        let resp = self
            .client
            .get(&test_url)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let latency_ms = start.elapsed().as_millis();

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                Ok(HealthProbe {
                    ok: status == 200,
                    status,
                    market_count: 0,
                    latency_ms,
                    detail: format!("CLOB API HTTP {}（探测地址: {}）", status, test_url),
                })
            }
            Err(e) => Ok(HealthProbe {
                ok: false,
                status: 0,
                market_count: 0,
                latency_ms,
                detail: format!("CLOB API 连接失败: {:#}", e),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clob_capability_has_orderbook_and_depth() {
        let cap = ClobProvider::capability_value();
        assert!(!cap.supports_markets);
        assert!(cap.supports_orderbook);
        assert!(cap.supports_bid_ask);
        assert!(cap.supports_trades);
        assert!(cap.supports_liquidity);
        assert!(cap.supports_real_arbitrage());
        assert!(cap.supports_depth);
        assert_eq!(cap.depth_levels, 10);
    }

    #[test]
    fn parse_orderbook_from_realistic_json() {
        let json = r#"{
            "bids": [["0.45", "100.5"], ["0.44", "200.0"], ["0.43", "50.0"]],
            "asks": [["0.47", "80.0"], ["0.48", "120.0"]]
        }"#;
        let ob = ClobProvider::parse_orderbook(json, "0xtest").expect("解析成功");

        assert_eq!(ob.market_id, "0xtest");
        assert_eq!(ob.provider, "clob");
        assert_eq!(ob.best_bid, Some(0.45));
        assert_eq!(ob.best_ask, Some(0.47));
        assert!((ob.spread.unwrap() - 0.02).abs() < 1e-9);

        // 多档盘口
        assert_eq!(ob.bid_levels.len(), 3);
        assert_eq!(ob.bid_levels[0].price, 0.45);
        assert_eq!(ob.bid_levels[0].size, 100.5);
        assert_eq!(ob.bid_levels[0].level, 1);
        assert_eq!(ob.bid_levels[1].price, 0.44);
        assert_eq!(ob.bid_levels[1].level, 2);

        assert_eq!(ob.ask_levels.len(), 2);
        assert_eq!(ob.ask_levels[0].price, 0.47);
        assert_eq!(ob.ask_levels[0].level, 1);

        // 累计量
        assert!((ob.bid_volume - 350.5).abs() < 1e-9); // 100.5 + 200.0 + 50.0
        assert!((ob.ask_volume - 200.0).abs() < 1e-9); // 80.0 + 120.0
        assert_eq!(ob.bid_depth, Some(350.5));
        assert_eq!(ob.ask_depth, Some(200.0));
    }

    #[test]
    fn parse_orderbook_empty_bids_asks() {
        let json = r#"{"bids": [], "asks": []}"#;
        let ob = ClobProvider::parse_orderbook(json, "empty_market").expect("解析成功");
        assert_eq!(ob.best_bid, None);
        assert_eq!(ob.best_ask, None);
        assert_eq!(ob.spread, None);
        assert!(ob.bid_levels.is_empty());
        assert!(ob.ask_levels.is_empty());
        assert!((ob.bid_volume - 0.0).abs() < 1e-9);
    }

    #[test]
    fn parse_orderbook_missing_fields() {
        // bids 缺失 -> 空
        let json = r#"{"asks": [["0.47", "80.0"]]}"#;
        let ob = ClobProvider::parse_orderbook(json, "partial").expect("解析成功");
        assert_eq!(ob.best_bid, None);
        assert_eq!(ob.best_ask, Some(0.47));
        assert!(ob.bid_levels.is_empty());
        assert_eq!(ob.ask_levels.len(), 1);
    }

    #[test]
    fn orderbook_empty_constructor() {
        let ob = OrderBook::empty("test_market", "clob");
        assert_eq!(ob.market_id, "test_market");
        assert_eq!(ob.provider, "clob");
        assert!(ob.best_bid.is_none());
        assert!(ob.best_ask.is_none());
        assert!(ob.bid_levels.is_empty());
        assert!(ob.ask_levels.is_empty());
    }

    #[test]
    fn parse_orderbook_invalid_json_returns_error() {
        let result = ClobProvider::parse_orderbook("not json", "bad");
        assert!(result.is_err());
    }
}
