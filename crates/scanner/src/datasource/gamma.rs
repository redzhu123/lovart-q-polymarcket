//! Gamma API Provider（V1.02 第二节）。
//!
//! 把原 `scanner::market::fetch_active_markets` 的分页拉取 + 完整 HTTP 可观测性
//! 整体迁入本 Provider，并把 Gamma 专用的 `Market` 转换为统一的 [`UnifiedMarket`]。
//!
//! 能力：仅 `markets` + `liquidity`；**无**订单簿 / 成交 / 最优买卖价 -> 不支撑真实套利
//!（Gamma 的 outcomePrices 是归一化中间价，YES+NO 恒为 1.0）。
//! 真实套利需接入 CLOB Provider（V1.02 之后）。

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pm_models::{Market, MarketStatus, OrderBook, PriceQuote, ProviderCapability, UnifiedMarket};

use crate::datasource::{HealthProbe, MarketDataProvider};
use crate::stats::{FetchResult, FetchStats, PageStats};

/// Polymarket Gamma 公开 API 基础地址。
pub const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

/// 单次请求拉取的市场数量（Gamma API 单页上限 100）。
const PAGE_LIMIT: usize = 100;

/// 翻页安全上限（API 在 offset≈2100 处会返回 422，到不了 50 页）。
const MAX_PAGES: usize = 50;

/// Gamma API Provider。
pub struct GammaProvider {
    client: reqwest::Client,
    /// 是否打印逐页 HTTP 调试（与 V1.0.1/V1.01 行为一致）。
    debug: bool,
}

impl GammaProvider {
    /// 构造：复用调用方传入的 reqwest::Client（driver 不再自建 client）。
    pub fn new(client: reqwest::Client, debug: bool) -> Self {
        Self { client, debug }
    }

    /// Gamma 能力声明（第六节 / V1.03 盘口深度扩展）。
    pub fn capability_value() -> ProviderCapability {
        ProviderCapability {
            supports_markets: true,
            supports_orderbook: false,
            supports_trades: false,
            supports_bid_ask: false,
            supports_liquidity: true,
            depth_levels: 0,
            supports_depth: false,
        }
    }
}

#[async_trait]
impl MarketDataProvider for GammaProvider {
    fn name(&self) -> &str {
        "gamma"
    }

    fn capability(&self) -> ProviderCapability {
        Self::capability_value()
    }

    /// 拉取所有"未关闭"市场（closed=false）并转换为 `UnifiedMarket`。
    ///
    /// 行为与原 `fetch_active_markets` 完全一致（URL/分页/422 末尾/逐页可观测性/错误绝不静默），
    /// 仅在反序列化后增加 `Market -> UnifiedMarket` 转换。
    async fn fetch_markets(&self) -> Result<FetchResult> {
        let mut all: Vec<UnifiedMarket> = Vec::new();
        let mut offset: usize = 0;
        let mut stats = FetchStats::default();
        let now = Utc::now();
        let provider_name = self.name();

        for page_idx in 0..MAX_PAGES {
            let url = format!(
                "{}/markets?limit={}&offset={}&closed=false&order=volumeNum&ascending=false",
                GAMMA_API_BASE, PAGE_LIMIT, offset
            );
            if page_idx == 0 {
                stats.first_url = Some(url.clone());
            }

            let start = Instant::now();
            let resp = self.client.get(&url).send().await;
            let elapsed = start.elapsed().as_millis();
            stats.request_count += 1;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    stats.failed_count += 1;
                    stats.last_status = 0;
                    stats.last_error = Some(err_msg.clone());
                    stats.pages.push(PageStats {
                        url: url.clone(),
                        status: 0,
                        bytes: 0,
                        elapsed_ms: elapsed,
                        ok: false,
                        error: Some(err_msg.clone()),
                    });
                    if self.debug {
                        print_page_debug(&url, 0, 0, elapsed, Some(&err_msg), page_idx);
                    } else {
                        println!("{}", crate::display::DASH);
                        println!();
                        println!("HTTP 请求失败");
                        println!();
                        println!("地址");
                        println!();
                        println!("{}", url);
                        println!();
                        println!("错误");
                        println!();
                        println!("{}", err_msg);
                        println!();
                    }
                    return Err(anyhow::anyhow!("请求失败: {}", err_msg));
                }
            };

            let status = resp.status().as_u16();

            // 422：offset 超出服务端允许上限，已到可访问数据末尾（视作成功收尾）
            if resp.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
                stats.success_count += 1;
                stats.last_status = status;
                stats.pages.push(PageStats {
                    url: url.clone(),
                    status,
                    bytes: 0,
                    elapsed_ms: elapsed,
                    ok: true,
                    error: None,
                });
                stats.total_ms += elapsed;
                if self.debug {
                    print_page_debug(&url, status, 0, elapsed, None, page_idx);
                }
                break;
            }

            let resp = match resp.error_for_status() {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    stats.failed_count += 1;
                    stats.last_status = status;
                    stats.last_error = Some(err_msg.clone());
                    stats.pages.push(PageStats {
                        url: url.clone(),
                        status,
                        bytes: 0,
                        elapsed_ms: elapsed,
                        ok: false,
                        error: Some(err_msg.clone()),
                    });
                    if self.debug {
                        print_page_debug(&url, status, 0, elapsed, Some(&err_msg), page_idx);
                    } else {
                        println!("{}", crate::display::DASH);
                        println!();
                        println!("HTTP 非 2xx");
                        println!();
                        println!("地址");
                        println!();
                        println!("{}", url);
                        println!();
                        println!("状态");
                        println!();
                        println!("{}", status);
                        println!();
                        println!("错误");
                        println!();
                        println!("{}", err_msg);
                        println!();
                    }
                    return Err(anyhow::anyhow!("服务端返回非 2xx: {}", err_msg));
                }
            };

            // 捕获 Rate-Limit 响应头 -- resp.text() 会消费 resp，须先取。
            let rate_limit = read_rate_limit(resp.headers());

            let body = match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    stats.failed_count += 1;
                    stats.last_status = status;
                    stats.last_error = Some(err_msg.clone());
                    stats.pages.push(PageStats {
                        url: url.clone(),
                        status,
                        bytes: 0,
                        elapsed_ms: elapsed,
                        ok: false,
                        error: Some(err_msg.clone()),
                    });
                    if self.debug {
                        print_page_debug(&url, status, 0, elapsed, Some(&err_msg), page_idx);
                    } else {
                        println!("{}", crate::display::DASH);
                        println!();
                        println!("HTTP 读取响应体失败");
                        println!();
                        println!("{}", err_msg);
                        println!();
                    }
                    return Err(anyhow::anyhow!("读取响应体失败: {}", err_msg));
                }
            };

            let bytes = body.len();
            stats.total_bytes += bytes as u64;
            stats.total_ms += elapsed;
            if stats.rate_limit.is_none() {
                stats.rate_limit = rate_limit;
            }

            // 反序列化单独计时
            let deser_start = Instant::now();
            let batch: Vec<Market> = match serde_json::from_str(&body) {
                Ok(b) => b,
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    stats.failed_count += 1;
                    stats.last_status = status;
                    stats.last_error = Some(format!("JSON 解析失败: {}", err_msg));
                    stats.deserialize_ms += deser_start.elapsed().as_millis();
                    stats.pages.push(PageStats {
                        url: url.clone(),
                        status,
                        bytes,
                        elapsed_ms: elapsed,
                        ok: false,
                        error: Some(format!("JSON 解析失败: {}", err_msg)),
                    });
                    println!("{}", crate::display::DASH);
                    println!();
                    println!("JSON 解析失败");
                    println!();
                    println!("地址");
                    println!();
                    println!("{}", url);
                    println!();
                    println!("状态");
                    println!();
                    println!("{}", status);
                    println!();
                    println!("字节数");
                    println!();
                    println!("{}", bytes);
                    println!();
                    println!("错误");
                    println!();
                    println!("{}", err_msg);
                    println!();
                    println!("响应预览（前 500 字符）");
                    println!();
                    let preview: String = body.chars().take(500).collect();
                    println!("{}", preview);
                    println!();
                    return Err(anyhow::anyhow!("解析 JSON 失败: {}", err_msg));
                }
            };
            stats.deserialize_ms += deser_start.elapsed().as_millis();

            stats.success_count += 1;
            stats.last_status = status;
            stats.pages.push(PageStats {
                url: url.clone(),
                status,
                bytes,
                elapsed_ms: elapsed,
                ok: true,
                error: None,
            });

            if self.debug {
                print_page_debug(&url, status, bytes, elapsed, None, page_idx);
            }

            let n = batch.len();
            all.extend(batch.iter().map(|m| to_unified(m, now, provider_name)));
            if n == 0 {
                break;
            }
            offset += n;
        }

        Ok(FetchResult {
            markets: all,
            stats,
        })
    }

    /// Gamma 不支持订单簿，返回空 Vec（不伪造）。
    async fn fetch_orderbooks(&self, _market_ids: &[String]) -> Result<Vec<OrderBook>> {
        Ok(Vec::new())
    }

    /// Gamma 不支持按需价格查询，返回空 Vec（不伪造）。
    async fn fetch_prices(&self, _market_ids: &[String]) -> Result<Vec<PriceQuote>> {
        Ok(Vec::new())
    }

    /// 健康检查：`limit=1` 探测 + JSON 解析。
    async fn health_check(&self) -> Result<HealthProbe> {
        let url = format!("{}/markets?limit=1&closed=false", GAMMA_API_BASE);
        let start = Instant::now();
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .send()
            .await;
        let latency_ms = start.elapsed().as_millis();
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                return Ok(HealthProbe {
                    ok: false,
                    status: 0,
                    market_count: 0,
                    latency_ms,
                    detail: format!("请求失败: {:#}", e),
                });
            }
        };
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Ok(HealthProbe {
                ok: false,
                status,
                market_count: 0,
                latency_ms,
                detail: format!("HTTP {}", status),
            });
        }
        let body = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(HealthProbe {
                    ok: false,
                    status,
                    market_count: 0,
                    latency_ms,
                    detail: format!("读取响应体失败: {:#}", e),
                });
            }
        };
        match serde_json::from_str::<Vec<Market>>(&body) {
            Ok(v) => Ok(HealthProbe {
                ok: true,
                status,
                market_count: v.len(),
                latency_ms,
                detail: format!("HTTP {}，解析 {} 个市场", status, v.len()),
            }),
            Err(e) => Ok(HealthProbe {
                ok: false,
                status,
                market_count: 0,
                latency_ms,
                detail: format!("JSON 解析失败: {:#}", e),
            }),
        }
    }
}

/// Gamma `Market` -> `UnifiedMarket` 转换（单一事实源）。
///
/// 价格按 outcomePrices 数组**逐侧**提取（get(0)=YES, get(1)=NO），与原
/// `extract_price` 语义一致，保留 YES/NO 缺失的逐侧诊断计数。`outcome_count` 为数组长度。
/// 是否二元由 `UnifiedMarket::yes_no_prices`（要求 outcome_count==2）判定。
fn to_unified(m: &Market, now: DateTime<Utc>, provider: &str) -> UnifiedMarket {
    let values: Vec<serde_json::Value> = m
        .outcome_prices
        .as_ref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let yes = values.first().and_then(to_f64_value);
    let no = values.get(1).and_then(to_f64_value);
    let outcome_count = values.len();

    let status = if m.closed {
        MarketStatus::Closed
    } else if m.active {
        MarketStatus::Active
    } else {
        MarketStatus::Inactive
    };
    let market_id = m
        .condition_id
        .clone()
        .or_else(|| m.id.clone())
        .unwrap_or_else(|| m.question.clone().unwrap_or_default());
    UnifiedMarket {
        market_id,
        question: m.question.clone().unwrap_or_default(),
        description: m.description.clone(),
        status,
        yes_price: yes,
        no_price: no,
        volume: m.volume_num,
        liquidity: m.liquidity_num,
        category: m.category.clone(),
        outcome_count,
        provider: provider.into(),
        updated_at: now,
    }
}

/// 把 JSON 值（字符串或数字）转为 f64（与 pm_models::market::to_f64 同语义）。
fn to_f64_value(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// 打印单页 HTTP 调试日志（debug=true 时调用）。
fn print_page_debug(
    url: &str,
    status: u16,
    bytes: usize,
    elapsed_ms: u128,
    error: Option<&str>,
    page_idx: usize,
) {
    println!("{}", crate::display::DASH);
    println!();
    println!("HTTP 请求 #{}", page_idx + 1);
    println!();
    println!("地址");
    println!();
    println!("{}", url);
    println!();
    println!("HTTP 状态");
    println!();
    println!("{}", status);
    println!();
    println!("响应大小");
    println!();
    println!("{} 字节", bytes);
    println!();
    println!("响应耗时");
    println!();
    println!("{} 毫秒", elapsed_ms);
    println!();
    if let Some(e) = error {
        println!("错误");
        println!();
        println!("{}", e);
        println!();
    }
}

/// 读取 Rate-Limit 响应头（V1.01 第六节）。
fn read_rate_limit(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (name, label) in [
        ("x-ratelimit-limit", "limit"),
        ("x-ratelimit-remaining", "remaining"),
        ("x-ratelimit-reset", "reset"),
    ] {
        if let Some(v) = headers.get(name).and_then(|h| h.to_str().ok()) {
            parts.push(format!("{}={}", label, v));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_capability_no_orderbook_no_bidask() {
        let cap = GammaProvider::capability_value();
        assert!(cap.supports_markets);
        assert!(cap.supports_liquidity);
        assert!(!cap.supports_orderbook);
        assert!(!cap.supports_bid_ask);
        assert!(!cap.supports_trades);
        assert!(!cap.supports_real_arbitrage());
        assert_eq!(cap.depth_levels, 0);
        assert!(!cap.supports_depth);
    }

    #[test]
    fn to_unified_binary_market() {
        let m = Market {
            id: Some("42".into()),
            condition_id: Some("0xcond".into()),
            question: Some("问题A".into()),
            description: Some("描述".into()),
            category: Some("政治".into()),
            active: true,
            closed: false,
            outcome_prices: Some(r#"["0.40","0.55"]"#.into()),
            volume_num: 1000.0,
            liquidity_num: 500.0,
        };
        let u = to_unified(&m, Utc::now(), "gamma");
        assert_eq!(u.market_id, "0xcond");
        assert_eq!(u.question, "问题A");
        assert_eq!(u.status, MarketStatus::Active);
        assert_eq!(u.yes_price, Some(0.40));
        assert_eq!(u.no_price, Some(0.55));
        assert_eq!(u.outcome_count, 2);
        assert_eq!(u.volume, 1000.0);
        assert_eq!(u.liquidity, 500.0);
        assert_eq!(u.category.as_deref(), Some("政治"));
        assert_eq!(u.provider, "gamma");
        assert!(u.active());
        assert!(!u.closed());
    }

    #[test]
    fn to_unified_closed_and_inactive_status() {
        let closed = Market {
            id: None,
            condition_id: None,
            question: Some("C".into()),
            description: None,
            category: None,
            active: true,
            closed: true,
            outcome_prices: Some(r#"["0.3","0.7"]"#.into()),
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        let u = to_unified(&closed, Utc::now(), "gamma");
        assert_eq!(u.status, MarketStatus::Closed);
        assert!(u.closed());
        assert!(u.active()); // Gamma 语义：closed 市场 active=true

        let inactive = Market {
            id: None,
            condition_id: None,
            question: Some("I".into()),
            description: None,
            category: None,
            active: false,
            closed: false,
            outcome_prices: Some(r#"["0.3","0.7"]"#.into()),
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        let u = to_unified(&inactive, Utc::now(), "gamma");
        assert_eq!(u.status, MarketStatus::Inactive);
        assert!(!u.active());
    }

    #[test]
    fn to_unified_market_id_fallbacks() {
        // condition_id 缺失 -> 用 id
        let m = Market {
            id: Some("99".into()),
            condition_id: None,
            question: Some("Q".into()),
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: None,
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        assert_eq!(to_unified(&m, Utc::now(), "gamma").market_id, "99");
        // 都缺失 -> 用 question
        let m = Market {
            id: None,
            condition_id: None,
            question: Some("fallbackQ".into()),
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: None,
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        assert_eq!(to_unified(&m, Utc::now(), "gamma").market_id, "fallbackQ");
    }

    #[test]
    fn to_unified_multi_outcome_is_non_binary() {
        let m = Market {
            id: None,
            condition_id: None,
            question: Some("Multi".into()),
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: Some(r#"["0.1","0.2","0.7"]"#.into()),
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        let u = to_unified(&m, Utc::now(), "gamma");
        assert_eq!(u.outcome_count, 3);
        // 逐侧提取：前两价存在，但 yes_no_prices 因非二元为 None
        assert_eq!(u.yes_price, Some(0.1));
        assert_eq!(u.no_price, Some(0.2));
        assert!(u.yes_no_prices().is_none());
        assert!(!u.has_prices());
    }
}
