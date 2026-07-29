//! Raydium read-only cyclic arbitrage scanner for Solana.
//!
//! Quotes come from Raydium's Trade API and are chained using each leg's
//! conservative `otherAmountThreshold`. This module never builds, signs, or
//! broadcasts a transaction.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::{StreamExt, stream};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::dex_v2::{DexV2Error, DexV2Result};

fn default_chain_id() -> u64 {
    101
}
fn default_api_url() -> String {
    "https://transaction-v1.raydium.io".into()
}
fn default_rpc_url() -> String {
    "https://api.mainnet-beta.solana.com".into()
}
fn default_slippage_bps() -> u32 {
    10
}
fn default_concurrency() -> usize {
    4
}
fn default_timeout_secs() -> u64 {
    12
}
fn default_poll_interval_ms() -> u64 {
    5_000
}
fn default_quote_retries() -> usize {
    2
}
fn default_true() -> bool {
    true
}
fn default_max_routes() -> usize {
    64
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaydiumConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_chain_id")]
    pub chain_id: u64,
    #[serde(default = "default_api_url")]
    pub api_base_url: String,
    #[serde(default = "default_rpc_url")]
    pub rpc_http_url: String,
    #[serde(default)]
    pub rpc_http_url_env: Option<String>,
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u32,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_quote_retries")]
    pub quote_max_retries: usize,
    pub input_amount: String,
    #[serde(default)]
    pub network_cost_anchor: String,
    #[serde(default)]
    pub min_net_profit_anchor: String,
    #[serde(default)]
    pub min_roi_bps: i64,
    #[serde(default)]
    pub risk_buffer_bps: u32,
    #[serde(default)]
    pub routes: RaydiumRouteConfig,
    #[serde(default)]
    pub tokens: Vec<RaydiumTokenConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaydiumRouteConfig {
    #[serde(default = "default_true")]
    pub enable_two_hop: bool,
    #[serde(default = "default_true")]
    pub enable_three_hop: bool,
    #[serde(default = "default_max_routes")]
    pub max_routes: usize,
}

impl Default for RaydiumRouteConfig {
    fn default() -> Self {
        Self {
            enable_two_hop: true,
            enable_three_hop: true,
            max_routes: default_max_routes(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaydiumTokenConfig {
    pub symbol: String,
    pub mint: String,
    pub decimals: u8,
    #[serde(default)]
    pub anchor: bool,
}

impl RaydiumConfig {
    pub fn load(path: impl AsRef<Path>) -> DexV2Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            DexV2Error::Configuration(format!("read {}: {error}", path.display()))
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| {
            DexV2Error::Configuration(format!("parse {}: {error}", path.display()))
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> DexV2Result<()> {
        if self.chain_id == 0
            || self.api_base_url.trim().is_empty()
            || self.rpc_http_url.trim().is_empty()
        {
            return Err(DexV2Error::Configuration(
                "Raydium chain_id/API/RPC cannot be empty".into(),
            ));
        }
        if self.slippage_bps > 10_000
            || self.risk_buffer_bps > 10_000
            || self.max_concurrency == 0
            || self.routes.max_routes == 0
            || self.request_timeout_secs == 0
            || self.poll_interval_ms == 0
            || self.quote_max_retries > 5
        {
            return Err(DexV2Error::Configuration("invalid Raydium limits".into()));
        }
        if self
            .rpc_http_url_env
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(DexV2Error::Configuration(
                "Raydium rpc_http_url_env cannot be empty".into(),
            ));
        }
        parse_amount("input_amount", &self.input_amount)?;
        parse_amount("network_cost_anchor", &self.network_cost_anchor)?;
        parse_amount("min_net_profit_anchor", &self.min_net_profit_anchor)?;
        if self.tokens.len() < 2 || self.tokens.iter().filter(|token| token.anchor).count() != 1 {
            return Err(DexV2Error::Configuration(
                "Raydium requires at least two tokens and exactly one anchor".into(),
            ));
        }
        let mut symbols = HashSet::new();
        let mut mints = HashSet::new();
        for token in &self.tokens {
            if token.symbol.trim().is_empty()
                || token.mint.trim().is_empty()
                || token.decimals > 18
                || !symbols.insert(token.symbol.to_ascii_uppercase())
                || !mints.insert(&token.mint)
            {
                return Err(DexV2Error::Configuration(
                    "invalid or duplicate Raydium token".into(),
                ));
            }
        }
        Ok(())
    }

    fn rpc_http_urls(&self) -> Vec<String> {
        let mut urls = self
            .rpc_http_url_env
            .as_deref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
            .into_iter()
            .collect::<Vec<_>>();
        if !urls.iter().any(|url| url == &self.rpc_http_url) {
            urls.push(self.rpc_http_url.clone());
        }
        urls
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeQuoteData {
    input_mint: String,
    input_amount: String,
    output_mint: String,
    output_amount: String,
    other_amount_threshold: String,
    #[serde(default)]
    price_impact_pct: f64,
    #[serde(default)]
    route_plan: Vec<RaydiumRouteStep>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaydiumRouteStep {
    pub pool_id: String,
    pub input_mint: String,
    pub output_mint: String,
    pub fee_amount: String,
    pub fee_mint: String,
}

#[derive(Debug, Deserialize)]
struct TradeQuoteEnvelope {
    success: bool,
    #[serde(default)]
    msg: String,
    data: Option<TradeQuoteData>,
}

#[derive(Debug, Clone)]
pub struct RaydiumLegQuote {
    pub input_symbol: String,
    pub output_symbol: String,
    pub input_amount: u128,
    pub expected_output: u128,
    pub conservative_output: u128,
    pub price_impact_pct: f64,
    pub route_plan: Vec<RaydiumRouteStep>,
}

#[derive(Debug, Clone)]
pub struct RaydiumOpportunity {
    pub path: Vec<String>,
    pub amount_in: u128,
    pub amount_out: u128,
    pub gross_profit: i128,
    pub net_profit: i128,
    pub roi_bps: i64,
    pub legs: Vec<RaydiumLegQuote>,
}

#[derive(Debug, Clone)]
pub struct RaydiumScanReport {
    pub slot: u64,
    pub routes_checked: usize,
    pub routes_failed: usize,
    pub pools_observed: usize,
    pub opportunities: Vec<RaydiumOpportunity>,
}

pub struct RaydiumScanner {
    config: RaydiumConfig,
    client: reqwest::Client,
    rpc_http_urls: Vec<String>,
    routes: Vec<Vec<usize>>,
    input_amount: u128,
    network_cost: u128,
    min_net_profit: u128,
    last_report: Mutex<Option<RaydiumScanReport>>,
    last_scan_completed: Mutex<Option<Instant>>,
    scan_gate: tokio::sync::Mutex<()>,
}

impl RaydiumScanner {
    pub fn new(config: RaydiumConfig) -> DexV2Result<Self> {
        config.validate()?;
        let rpc_http_urls = config.rpc_http_urls();
        let routes = generate_routes(&config);
        if routes.is_empty() {
            return Err(DexV2Error::Configuration(
                "Raydium generated no cyclic routes".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .map_err(|error| DexV2Error::Configuration(format!("Raydium HTTP client: {error}")))?;
        Ok(Self {
            input_amount: parse_amount("input_amount", &config.input_amount)?,
            network_cost: parse_amount("network_cost_anchor", &config.network_cost_anchor)?,
            min_net_profit: parse_amount("min_net_profit_anchor", &config.min_net_profit_anchor)?,
            config,
            client,
            rpc_http_urls,
            routes,
            last_report: Mutex::new(None),
            last_scan_completed: Mutex::new(None),
            scan_gate: tokio::sync::Mutex::new(()),
        })
    }

    pub fn chain_id(&self) -> u64 {
        self.config.chain_id
    }
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
    pub fn token(&self, symbol: &str) -> Option<&RaydiumTokenConfig> {
        self.config
            .tokens
            .iter()
            .find(|token| token.symbol == symbol)
    }

    pub fn last_report(&self) -> DexV2Result<Option<RaydiumScanReport>> {
        self.last_report
            .lock()
            .map(|report| report.clone())
            .map_err(|_| DexV2Error::Repository("Raydium report lock poisoned".into()))
    }

    pub async fn scan_once(&self) -> DexV2Result<RaydiumScanReport> {
        let _guard = self.scan_gate.lock().await;
        let result = self.scan_inner().await;
        self.mark_scan_completed()?;
        result
    }

    /// Allows the supervisor to tick quickly while respecting Raydium's own polling interval.
    pub async fn scan_if_due(&self) -> DexV2Result<Option<RaydiumScanReport>> {
        let _guard = self.scan_gate.lock().await;
        let due = self
            .last_scan_completed
            .lock()
            .map_err(|_| DexV2Error::Repository("Raydium scan clock lock poisoned".into()))?
            .is_none_or(|completed| {
                completed.elapsed() >= Duration::from_millis(self.config.poll_interval_ms)
            });
        if !due {
            return Ok(None);
        }
        let result = self.scan_inner().await;
        self.mark_scan_completed()?;
        result.map(Some)
    }

    fn mark_scan_completed(&self) -> DexV2Result<()> {
        *self
            .last_scan_completed
            .lock()
            .map_err(|_| DexV2Error::Repository("Raydium scan clock lock poisoned".into()))? =
            Some(Instant::now());
        Ok(())
    }

    async fn scan_inner(&self) -> DexV2Result<RaydiumScanReport> {
        let slot = match self.head_slot().await {
            Ok(slot) => slot,
            Err(error) => {
                tracing::warn!(error = %error, "Solana slot unavailable; Raydium quote scan continues");
                0
            }
        };
        let results = stream::iter(
            self.routes
                .iter()
                .cloned()
                .map(|route| async move { self.quote_route(&route).await }),
        )
        .buffer_unordered(self.config.max_concurrency)
        .collect::<Vec<_>>()
        .await;

        let mut failed = 0usize;
        let mut succeeded = 0usize;
        let mut pools = HashSet::new();
        let mut opportunities = Vec::new();
        let mut last_error = None;
        for result in results {
            match result {
                Ok((Some(opportunity), route_pools)) => {
                    succeeded += 1;
                    pools.extend(route_pools);
                    opportunities.push(opportunity);
                }
                Ok((None, route_pools)) => {
                    succeeded += 1;
                    pools.extend(route_pools);
                }
                Err(error) => {
                    failed += 1;
                    last_error = Some(error);
                }
            }
        }
        if succeeded == 0 {
            return Err(
                last_error.unwrap_or_else(|| DexV2Error::Rpc("all Raydium routes failed".into()))
            );
        }
        opportunities.sort_by_key(|opportunity| std::cmp::Reverse(opportunity.net_profit));
        let report = RaydiumScanReport {
            slot,
            routes_checked: self.routes.len(),
            routes_failed: failed,
            pools_observed: pools.len(),
            opportunities,
        };
        *self
            .last_report
            .lock()
            .map_err(|_| DexV2Error::Repository("Raydium report lock poisoned".into()))? =
            Some(report.clone());
        Ok(report)
    }

    async fn quote_route(
        &self,
        route: &[usize],
    ) -> DexV2Result<(Option<RaydiumOpportunity>, HashSet<String>)> {
        let mut amount = self.input_amount;
        let mut legs = Vec::with_capacity(route.len() - 1);
        for pair in route.windows(2) {
            let input = &self.config.tokens[pair[0]];
            let output = &self.config.tokens[pair[1]];
            let data = self.quote_leg(&input.mint, &output.mint, amount).await?;
            if data.input_mint != input.mint || data.output_mint != output.mint {
                return Err(DexV2Error::Quote("Raydium quote mint mismatch".into()));
            }
            let input_amount = parse_amount("quote inputAmount", &data.input_amount)?;
            let expected_output = parse_amount("quote outputAmount", &data.output_amount)?;
            let conservative_output =
                parse_amount("quote otherAmountThreshold", &data.other_amount_threshold)?;
            legs.push(RaydiumLegQuote {
                input_symbol: input.symbol.clone(),
                output_symbol: output.symbol.clone(),
                input_amount,
                expected_output,
                conservative_output,
                price_impact_pct: data.price_impact_pct,
                route_plan: data.route_plan,
            });
            amount = conservative_output;
        }

        let input = i128::try_from(self.input_amount)
            .map_err(|_| DexV2Error::Quote("input exceeds i128".into()))?;
        let output =
            i128::try_from(amount).map_err(|_| DexV2Error::Quote("output exceeds i128".into()))?;
        let gross = output - input;
        let risk = self
            .input_amount
            .saturating_mul(u128::from(self.config.risk_buffer_bps))
            / 10_000;
        let explicit_cost = self.network_cost.saturating_add(risk);
        let net = gross
            - i128::try_from(explicit_cost)
                .map_err(|_| DexV2Error::Quote("cost exceeds i128".into()))?;
        let roi = net.saturating_mul(10_000) / input.max(1);
        if net < i128::try_from(self.min_net_profit).unwrap_or(i128::MAX)
            || roi < i128::from(self.config.min_roi_bps)
        {
            let pools = legs
                .iter()
                .flat_map(|leg| leg.route_plan.iter().map(|step| step.pool_id.clone()))
                .collect();
            return Ok((None, pools));
        }
        let pools = legs
            .iter()
            .flat_map(|leg| leg.route_plan.iter().map(|step| step.pool_id.clone()))
            .collect();
        Ok((
            Some(RaydiumOpportunity {
                path: route
                    .iter()
                    .map(|index| self.config.tokens[*index].symbol.clone())
                    .collect(),
                amount_in: self.input_amount,
                amount_out: amount,
                gross_profit: gross,
                net_profit: net,
                roi_bps: i64::try_from(roi).unwrap_or(i64::MAX),
                legs,
            }),
            pools,
        ))
    }

    async fn quote_leg(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: u128,
    ) -> DexV2Result<TradeQuoteData> {
        let url = format!(
            "{}/compute/swap-base-in",
            self.config.api_base_url.trim_end_matches('/')
        );
        let query = [
            ("inputMint", input_mint.to_string()),
            ("outputMint", output_mint.to_string()),
            ("amount", amount.to_string()),
            ("slippageBps", self.config.slippage_bps.to_string()),
            ("txVersion", "V0".to_string()),
        ];
        let mut last_error = None;
        for attempt in 0..=self.config.quote_max_retries {
            let response = match self.client.get(&url).query(&query).send().await {
                Ok(response) => response,
                Err(error) => {
                    last_error = Some(format!("transport: {error}"));
                    if attempt < self.config.quote_max_retries {
                        retry_delay(attempt).await;
                        continue;
                    }
                    break;
                }
            };
            let status = response.status();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown")
                .to_string();
            let body = response.bytes().await.map_err(|error| {
                DexV2Error::Rpc(format!("Raydium quote body read HTTP {status}: {error}"))
            })?;
            if !status.is_success() {
                last_error = Some(format!(
                    "HTTP {status}, content-type={content_type}, body={}",
                    body_preview(&body)
                ));
                if (status.as_u16() == 429 || status.is_server_error())
                    && attempt < self.config.quote_max_retries
                {
                    retry_delay(attempt).await;
                    continue;
                }
                break;
            }
            let envelope: TradeQuoteEnvelope = match serde_json::from_slice(&body) {
                Ok(envelope) => envelope,
                Err(error) => {
                    last_error = Some(format!(
                        "decode HTTP {status}: {error}; content-type={content_type}; body={}",
                        body_preview(&body)
                    ));
                    if attempt < self.config.quote_max_retries {
                        retry_delay(attempt).await;
                        continue;
                    }
                    break;
                }
            };
            if !envelope.success {
                return Err(DexV2Error::Rpc(format!(
                    "Raydium quote rejected: {}",
                    envelope.msg
                )));
            }
            return envelope
                .data
                .ok_or_else(|| DexV2Error::Rpc("Raydium quote missing data".into()));
        }
        Err(DexV2Error::Rpc(format!(
            "Raydium quote failed after {} attempt(s): {}",
            self.config.quote_max_retries + 1,
            last_error.unwrap_or_else(|| "unknown error".into())
        )))
    }

    async fn head_slot(&self) -> DexV2Result<u64> {
        let request = json!({
            "jsonrpc": "2.0", "id": 1, "method": "getSlot", "params": [{"commitment": "confirmed"}]
        });
        let mut failures = Vec::new();
        for url in &self.rpc_http_urls {
            let label = rpc_endpoint_label(url);
            let response = match self.client.post(url).json(&request).send().await {
                Ok(response) => response,
                Err(error) => {
                    failures.push(format!("{label}: {}", rpc_transport_error(&error)));
                    continue;
                }
            };
            let status = response.status();
            let body: Value = match response.json().await {
                Ok(body) => body,
                Err(error) => {
                    failures.push(format!("{label}: decode: {error}"));
                    continue;
                }
            };
            if !status.is_success() || body.get("error").is_some() {
                failures.push(format!("{label}: HTTP {status}: {body}"));
                continue;
            }
            if let Some(slot) = body.get("result").and_then(Value::as_u64) {
                return Ok(slot);
            }
            failures.push(format!("{label}: missing result"));
        }
        Err(DexV2Error::Rpc(format!(
            "Solana getSlot failed on all {} RPC endpoint(s): {}",
            self.rpc_http_urls.len(),
            failures.join("; ")
        )))
    }
}

fn rpc_endpoint_label(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .unwrap_or_else(|| "configured-endpoint".into())
}

fn rpc_transport_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "transport timeout"
    } else if error.is_connect() {
        "connection failed"
    } else if error.is_request() {
        "request failed"
    } else {
        "transport failed"
    }
}

async fn retry_delay(attempt: usize) {
    let delay_ms = 250u64.saturating_mul(1u64 << attempt.min(4));
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

fn body_preview(body: &[u8]) -> String {
    String::from_utf8_lossy(&body[..body.len().min(240)]).replace(['\r', '\n'], " ")
}

fn generate_routes(config: &RaydiumConfig) -> Vec<Vec<usize>> {
    let anchor = config
        .tokens
        .iter()
        .position(|token| token.anchor)
        .unwrap_or(0);
    let others = (0..config.tokens.len())
        .filter(|index| *index != anchor)
        .collect::<Vec<_>>();
    let mut routes = Vec::new();
    if config.routes.enable_two_hop {
        routes.extend(others.iter().map(|token| vec![anchor, *token, anchor]));
    }
    if config.routes.enable_three_hop {
        for left in &others {
            for right in &others {
                if left != right {
                    routes.push(vec![anchor, *left, *right, anchor]);
                }
            }
        }
    }
    routes.truncate(config.routes.max_routes);
    routes
}

fn parse_amount(field: &str, value: &str) -> DexV2Result<u128> {
    if value.trim().is_empty() {
        return Ok(0);
    }
    value
        .parse::<u128>()
        .map_err(|error| DexV2Error::Configuration(format!("invalid {field}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> RaydiumConfig {
        RaydiumConfig {
            enabled: true,
            chain_id: 101,
            api_base_url: default_api_url(),
            rpc_http_url: default_rpc_url(),
            rpc_http_url_env: None,
            slippage_bps: 10,
            max_concurrency: 2,
            request_timeout_secs: 5,
            poll_interval_ms: 5_000,
            quote_max_retries: 2,
            input_amount: "1000000".into(),
            network_cost_anchor: "100".into(),
            min_net_profit_anchor: "1".into(),
            min_roi_bps: 1,
            risk_buffer_bps: 5,
            routes: RaydiumRouteConfig::default(),
            tokens: vec![
                RaydiumTokenConfig {
                    symbol: "USDC".into(),
                    mint: "usdc".into(),
                    decimals: 6,
                    anchor: true,
                },
                RaydiumTokenConfig {
                    symbol: "SOL".into(),
                    mint: "sol".into(),
                    decimals: 9,
                    anchor: false,
                },
                RaydiumTokenConfig {
                    symbol: "RAY".into(),
                    mint: "ray".into(),
                    decimals: 6,
                    anchor: false,
                },
            ],
        }
    }

    #[test]
    fn builds_two_and_three_hop_cycles() {
        let routes = generate_routes(&config());
        assert_eq!(routes.len(), 4);
        assert!(routes.iter().all(|route| route.first() == route.last()));
    }

    #[test]
    fn parses_official_trade_api_shape() {
        let envelope: TradeQuoteEnvelope = serde_json::from_str(r#"{
          "success":true,"data":{"inputMint":"a","inputAmount":"100","outputMint":"b",
          "outputAmount":"101","otherAmountThreshold":"100","priceImpactPct":0.001,
          "routePlan":[{"poolId":"p","inputMint":"a","outputMint":"b","feeAmount":"1","feeMint":"a"}]}}
        "#).unwrap();
        assert_eq!(envelope.data.unwrap().route_plan[0].pool_id, "p");
    }

    #[test]
    fn example_config_is_valid_and_shadow_only() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dex-solana-raydium.toml");
        let config = RaydiumConfig::load(path).unwrap();
        let scanner = RaydiumScanner::new(config).unwrap();
        assert_eq!(scanner.chain_id(), 101);
        assert_eq!(scanner.route_count(), 9);
    }

    #[tokio::test]
    #[ignore = "uses public Solana RPC and Raydium Trade API"]
    async fn live_example_config_scans_all_routes() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dex-solana-raydium.toml");
        let scanner = RaydiumScanner::new(RaydiumConfig::load(path).unwrap()).unwrap();
        let report = scanner.scan_once().await.unwrap();
        assert_eq!(report.routes_checked, scanner.route_count());
        assert!(report.routes_failed < report.routes_checked);
        assert!(report.pools_observed > 0);
    }
}
