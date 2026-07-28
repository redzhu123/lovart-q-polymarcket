use std::str::FromStr;

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;

use crate::dex_router::{RouterError, RouterResult};

#[derive(Debug, Clone)]
pub struct BridgeQuote {
    pub provider: String,
    pub from_chain_id: u64,
    pub to_chain_id: u64,
    pub from_token: Address,
    pub to_token: Address,
    pub amount_in: U256,
    pub amount_out: U256,
    pub minimum_amount_out: U256,
    pub estimated_seconds: u64,
    pub raw: Value,
}

impl BridgeQuote {
    pub fn amount_cost(&self) -> U256 {
        self.amount_in.saturating_sub(self.amount_out)
    }
}

#[async_trait]
pub trait BridgeQuoteProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    async fn quote(
        &self,
        from_chain_id: u64,
        to_chain_id: u64,
        from_token: Address,
        to_token: Address,
        amount_in: U256,
        from_address: Address,
    ) -> RouterResult<BridgeQuote>;
}

pub struct LiFiQuoteProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
}

impl LiFiQuoteProvider {
    pub fn new(endpoint: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            api_key,
        }
    }
}

#[async_trait]
impl BridgeQuoteProvider for LiFiQuoteProvider {
    fn provider_name(&self) -> &str {
        "lifi"
    }

    async fn quote(
        &self,
        from_chain_id: u64,
        to_chain_id: u64,
        from_token: Address,
        to_token: Address,
        amount_in: U256,
        from_address: Address,
    ) -> RouterResult<BridgeQuote> {
        let mut request = self.client.get(&self.endpoint).query(&[
            ("fromChain", from_chain_id.to_string()),
            ("toChain", to_chain_id.to_string()),
            ("fromToken", format!("{from_token:#x}")),
            ("toToken", format!("{to_token:#x}")),
            ("fromAmount", amount_in.to_string()),
            ("fromAddress", format!("{from_address:#x}")),
        ]);
        if let Some(key) = &self.api_key {
            request = request.header("x-lifi-api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| RouterError::Aggregator(format!("LI.FI transport: {error}")))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|error| RouterError::Aggregator(format!("LI.FI decode: {error}")))?;
        if !status.is_success() {
            return Err(RouterError::Aggregator(format!(
                "LI.FI HTTP {status}: {body}"
            )));
        }
        let estimate = body
            .get("estimate")
            .ok_or_else(|| RouterError::Aggregator("LI.FI missing estimate".into()))?;
        Ok(BridgeQuote {
            provider: "lifi".into(),
            from_chain_id,
            to_chain_id,
            from_token,
            to_token,
            amount_in,
            amount_out: parse_u256(estimate, "toAmount")?,
            minimum_amount_out: parse_u256(estimate, "toAmountMin")?,
            estimated_seconds: estimate
                .get("executionDuration")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            raw: body,
        })
    }
}

pub struct AcrossQuoteProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
}

impl AcrossQuoteProvider {
    pub fn new(endpoint: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            api_key,
        }
    }
}

#[async_trait]
impl BridgeQuoteProvider for AcrossQuoteProvider {
    fn provider_name(&self) -> &str {
        "across"
    }

    async fn quote(
        &self,
        from_chain_id: u64,
        to_chain_id: u64,
        from_token: Address,
        to_token: Address,
        amount_in: U256,
        _from_address: Address,
    ) -> RouterResult<BridgeQuote> {
        let mut headers = HeaderMap::new();
        if let Some(key) = &self.api_key {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(key)
                    .map_err(|error| RouterError::Configuration(error.to_string()))?,
            );
        }
        let response = self
            .client
            .get(&self.endpoint)
            .headers(headers)
            .query(&[
                ("originChainId", from_chain_id.to_string()),
                ("destinationChainId", to_chain_id.to_string()),
                ("token", format!("{from_token:#x}")),
                ("inputToken", format!("{from_token:#x}")),
                ("outputToken", format!("{to_token:#x}")),
                ("amount", amount_in.to_string()),
                ("inputAmount", amount_in.to_string()),
            ])
            .send()
            .await
            .map_err(|error| RouterError::Aggregator(format!("Across transport: {error}")))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|error| RouterError::Aggregator(format!("Across decode: {error}")))?;
        if !status.is_success() {
            return Err(RouterError::Aggregator(format!(
                "Across HTTP {status}: {body}"
            )));
        }
        let amount_out = body
            .get("expectedOutputAmount")
            .or_else(|| body.get("outputAmount"))
            .and_then(Value::as_str)
            .map(parse_u256_str)
            .transpose()?
            .or_else(|| {
                body.pointer("/totalRelayFee/total")
                    .and_then(Value::as_str)
                    .and_then(|fee| parse_u256_str(fee).ok())
                    .map(|fee| amount_in.saturating_sub(fee))
            })
            .ok_or_else(|| RouterError::Aggregator("Across missing output amount".into()))?;
        let minimum = body
            .get("minOutputAmount")
            .and_then(Value::as_str)
            .map(parse_u256_str)
            .transpose()?
            .unwrap_or(amount_out);
        Ok(BridgeQuote {
            provider: "across".into(),
            from_chain_id,
            to_chain_id,
            from_token,
            to_token,
            amount_in,
            amount_out,
            minimum_amount_out: minimum,
            estimated_seconds: body
                .get("estimatedFillTimeSec")
                .or_else(|| body.get("estimatedFillTime"))
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            raw: body,
        })
    }
}

fn parse_u256(value: &Value, field: &str) -> RouterResult<U256> {
    parse_u256_str(
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| RouterError::Aggregator(format!("missing {field}")))?,
    )
}

fn parse_u256_str(value: &str) -> RouterResult<U256> {
    U256::from_str(value).map_err(|error| RouterError::Aggregator(error.to_string()))
}
