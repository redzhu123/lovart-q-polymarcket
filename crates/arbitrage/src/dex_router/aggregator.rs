use std::str::FromStr;
use std::sync::Arc;

use alloy_primitives::{Address, Bytes, U256};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;

use crate::dex_v2::ChainConnector;

use super::quote::ProtocolQuoteProvider;
use super::types::{ExactLegQuote, LiquidityEdge, ProtocolKind, RouterError, RouterResult};

pub struct ZeroXQuoteProvider {
    id: String,
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    taker: Address,
    slippage_bps: u32,
}

impl ZeroXQuoteProvider {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
        taker: Address,
        slippage_bps: u32,
    ) -> RouterResult<Self> {
        let id = id.into();
        let endpoint = endpoint.into();
        let api_key = api_key.into();
        if id.trim().is_empty()
            || endpoint.trim().is_empty()
            || api_key.trim().is_empty()
            || taker.is_zero()
            || slippage_bps > 10_000
        {
            return Err(RouterError::Configuration("0x 报价配置无效".into()));
        }
        Ok(Self {
            id,
            client: reqwest::Client::new(),
            endpoint,
            api_key,
            taker,
            slippage_bps,
        })
    }
}

#[async_trait]
impl ProtocolQuoteProvider for ZeroXQuoteProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    async fn quote_exact_input(
        &self,
        edge: &LiquidityEdge,
        amount_in: U256,
        block_number: u64,
    ) -> RouterResult<ExactLegQuote> {
        if edge.protocol != ProtocolKind::Aggregator {
            return Err(RouterError::Aggregator(
                "0x Provider 收到了非聚合器边".into(),
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            "0x-api-key",
            HeaderValue::from_str(&self.api_key)
                .map_err(|error| RouterError::Configuration(error.to_string()))?,
        );
        headers.insert("0x-version", HeaderValue::from_static("v2"));
        let response = self
            .client
            .get(&self.endpoint)
            .headers(headers)
            .query(&[
                ("chainId", edge.chain_id.to_string()),
                ("sellToken", format!("{:#x}", edge.token_in)),
                ("buyToken", format!("{:#x}", edge.token_out)),
                ("sellAmount", amount_in.to_string()),
                ("taker", format!("{:#x}", self.taker)),
                ("slippageBps", self.slippage_bps.to_string()),
            ])
            .send()
            .await
            .map_err(|error| RouterError::Aggregator(format!("0x transport: {error}")))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|error| RouterError::Aggregator(format!("0x decode: {error}")))?;
        if !status.is_success() {
            return Err(RouterError::Aggregator(format!("0x HTTP {status}: {body}")));
        }
        let amount_out = parse_decimal_u256(&body, "buyAmount")?;
        let transaction = body
            .get("transaction")
            .ok_or_else(|| RouterError::Aggregator("0x quote missing transaction".into()))?;
        let target = parse_address(transaction, "to")?;
        let calldata = parse_bytes(transaction, "data")?;
        let gas_units = transaction
            .get("gas")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(edge.estimated_gas_units);
        Ok(ExactLegQuote {
            edge_id: edge.id.clone(),
            provider_id: self.id.clone(),
            amount_in,
            amount_out,
            gas_units,
            block_number,
            target: Some(target),
            calldata: Some(calldata),
        })
    }
}

pub struct RpcQuoteVerifier {
    connector: Arc<dyn ChainConnector>,
    from: Address,
}

impl RpcQuoteVerifier {
    pub fn new(connector: Arc<dyn ChainConnector>, from: Address) -> RouterResult<Self> {
        if from.is_zero() {
            return Err(RouterError::Configuration(
                "RPC 复验需要非零 from 地址".into(),
            ));
        }
        Ok(Self { connector, from })
    }

    pub async fn verify(&self, quote: &ExactLegQuote) -> RouterResult<u64> {
        let target = quote
            .target
            .ok_or_else(|| RouterError::Quote("报价没有执行 target".into()))?;
        let calldata = quote
            .calldata
            .clone()
            .ok_or_else(|| RouterError::Quote("报价没有执行 calldata".into()))?;
        self.connector
            .eth_call(target, calldata.clone(), quote.block_number)
            .await
            .map_err(|error| RouterError::Rpc(format!("聚合器 eth_call 失败: {error}")))?;
        self.connector
            .estimate_gas(self.from, target, calldata, quote.block_number)
            .await
            .map_err(|error| RouterError::Rpc(format!("聚合器 estimateGas 失败: {error}")))
    }
}

fn parse_decimal_u256(value: &Value, field: &str) -> RouterResult<U256> {
    let raw = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RouterError::Aggregator(format!("0x missing {field}")))?;
    U256::from_str(raw).map_err(|error| RouterError::Aggregator(error.to_string()))
}

fn parse_address(value: &Value, field: &str) -> RouterResult<Address> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RouterError::Aggregator(format!("0x missing {field}")))?
        .parse()
        .map_err(|error| RouterError::Aggregator(format!("invalid address: {error}")))
}

fn parse_bytes(value: &Value, field: &str) -> RouterResult<Bytes> {
    Bytes::from_str(
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| RouterError::Aggregator(format!("0x missing {field}")))?,
    )
    .map_err(|error| RouterError::Aggregator(format!("invalid calldata: {error}")))
}
