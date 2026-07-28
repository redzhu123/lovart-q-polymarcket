use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::SystemTime;

use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use async_trait::async_trait;
use futures_util::{StreamExt, TryStreamExt, stream};
use serde_json::{Value, json};

use super::error::{DexV2Error, DexV2Result};
use super::types::{ConfirmationMode, PoolId, PoolUpdate, V2Pool, V2PoolState};

#[async_trait]
pub trait ChainConnector: Send + Sync {
    async fn head_block(&self, mode: ConfirmationMode) -> DexV2Result<u64>;
    async fn gas_price(&self) -> DexV2Result<U256>;
    async fn v2_pair(
        &self,
        factory: Address,
        token_a: Address,
        token_b: Address,
    ) -> DexV2Result<Address>;
    async fn get_reserves(&self, pool: &V2Pool, block: u64) -> DexV2Result<V2PoolState>;
    async fn poll_pool_updates(
        &self,
        pools: &[V2Pool],
        from_block: u64,
        to_block: u64,
    ) -> DexV2Result<Vec<PoolUpdate>>;
    async fn eth_call(&self, to: Address, data: Bytes, block: u64) -> DexV2Result<Bytes>;
    async fn estimate_gas(
        &self,
        from: Address,
        to: Address,
        data: Bytes,
        block: u64,
    ) -> DexV2Result<u64>;
}

pub struct JsonRpcConnector {
    url: String,
    client: reqwest::Client,
}

impl JsonRpcConnector {
    pub fn new(url: impl Into<String>) -> DexV2Result<Self> {
        let url = url.into();
        if url.trim().is_empty() {
            return Err(DexV2Error::Configuration("empty RPC URL".into()));
        }
        Ok(Self {
            url,
            client: reqwest::Client::new(),
        })
    }

    async fn rpc(&self, method: &str, params: Value) -> DexV2Result<Value> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
            }))
            .send()
            .await
            .map_err(|e| DexV2Error::Rpc(format!("{method} transport: {e}")))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| DexV2Error::Rpc(format!("{method} decode: {e}")))?;
        if !status.is_success() {
            return Err(DexV2Error::Rpc(format!("{method} HTTP {status}: {body}")));
        }
        if let Some(error) = body.get("error") {
            return Err(DexV2Error::Rpc(format!("{method}: {error}")));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| DexV2Error::Rpc(format!("{method}: missing result")))
    }
}

#[async_trait]
impl ChainConnector for JsonRpcConnector {
    async fn head_block(&self, mode: ConfirmationMode) -> DexV2Result<u64> {
        if mode == ConfirmationMode::Latest {
            let value = self.rpc("eth_blockNumber", json!([])).await?;
            return parse_hex_u64(
                value
                    .as_str()
                    .ok_or_else(|| DexV2Error::Rpc("invalid block number".into()))?,
            );
        }
        let tag = match mode {
            ConfirmationMode::Safe => "safe",
            ConfirmationMode::Finalized => "finalized",
            ConfirmationMode::Latest => "latest",
        };
        let block = self
            .rpc("eth_getBlockByNumber", json!([tag, false]))
            .await?;
        parse_hex_u64(
            block
                .get("number")
                .and_then(Value::as_str)
                .ok_or_else(|| DexV2Error::Rpc(format!("{tag} block has no number")))?,
        )
    }

    async fn gas_price(&self) -> DexV2Result<U256> {
        let value = self.rpc("eth_gasPrice", json!([])).await?;
        parse_hex_u256(
            value
                .as_str()
                .ok_or_else(|| DexV2Error::Rpc("invalid gas price".into()))?,
        )
    }

    async fn v2_pair(
        &self,
        factory: Address,
        token_a: Address,
        token_b: Address,
    ) -> DexV2Result<Address> {
        let mut data = Vec::with_capacity(68);
        data.extend_from_slice(&[0xe6, 0xa4, 0x39, 0x05]);
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(token_a.as_slice());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(token_b.as_slice());
        let result = self
            .rpc(
                "eth_call",
                json!([{
                    "to": format!("{factory:#x}"),
                    "data": format!("0x{}", hex_encode(&data)),
                }, "latest"]),
            )
            .await?;
        let bytes = parse_hex_bytes(
            result
                .as_str()
                .ok_or_else(|| DexV2Error::Rpc("invalid getPair result".into()))?,
        )?;
        if bytes.len() < 32 {
            return Err(DexV2Error::Rpc("short getPair result".into()));
        }
        Ok(Address::from_slice(&bytes[bytes.len() - 20..]))
    }

    async fn get_reserves(&self, pool: &V2Pool, block: u64) -> DexV2Result<V2PoolState> {
        let bytes = self
            .eth_call(
                pool.id.address,
                Bytes::from_static(&[0x09, 0x02, 0xf1, 0xac]),
                block,
            )
            .await?;
        if bytes.len() < 64 {
            return Err(DexV2Error::Rpc(format!(
                "getReserves short response for {}",
                pool.name
            )));
        }
        Ok(V2PoolState {
            reserve0: U256::from_be_slice(&bytes[..32]),
            reserve1: U256::from_be_slice(&bytes[32..64]),
            block_number: block,
            block_hash: None,
            updated_at: SystemTime::now(),
        })
    }

    async fn poll_pool_updates(
        &self,
        pools: &[V2Pool],
        from_block: u64,
        to_block: u64,
    ) -> DexV2Result<Vec<PoolUpdate>> {
        if from_block > to_block || pools.is_empty() {
            return Ok(Vec::new());
        }
        let sync_topic = format!("{:#x}", keccak256("Sync(uint112,uint112)"));
        let addresses = pools.iter().map(|pool| pool.id.address).collect::<Vec<_>>();
        // PublicNode 等免费 RPC 会拦截 address 数组，因此按单池查询，并限制并发。
        let results = stream::iter(addresses.into_iter().map(|address| {
            let sync_topic = sync_topic.clone();
            async move {
                self.rpc(
                    "eth_getLogs",
                    json!([{
                        "fromBlock": format!("0x{from_block:x}"),
                        "toBlock": format!("0x{to_block:x}"),
                        "address": format!("{address:#x}"),
                        "topics": [sync_topic],
                    }]),
                )
                .await
            }
        }))
        .buffer_unordered(4)
        .try_collect::<Vec<_>>()
        .await?;
        let by_address = pools
            .iter()
            .map(|pool| (pool.id.address, pool))
            .collect::<HashMap<_, _>>();
        let mut latest = HashMap::<Address, (u64, u64, Option<B256>)>::new();
        for result in results {
            let logs = result
                .as_array()
                .ok_or_else(|| DexV2Error::Rpc("eth_getLogs result is not an array".into()))?;
            for raw in logs {
                let address = parse_address_field(raw, "address")?;
                if raw.get("removed").and_then(Value::as_bool).unwrap_or(false) {
                    latest.insert(address, (to_block, u64::MAX, None));
                    continue;
                }
                let block = parse_hex_u64(field_str(raw, "blockNumber")?)?;
                let index = parse_hex_u64(field_str(raw, "logIndex")?)?;
                let hash = raw
                    .get("blockHash")
                    .and_then(Value::as_str)
                    .map(parse_b256)
                    .transpose()?;
                let version = latest.entry(address).or_insert((0, 0, None));
                if (block, index) > (version.0, version.1) {
                    *version = (block, index, hash);
                }
            }
        }
        let mut updates = Vec::with_capacity(latest.len());
        for (address, (block, index, hash)) in latest {
            let pool = by_address
                .get(&address)
                .ok_or_else(|| DexV2Error::Rpc("log from unregistered pool".into()))?;
            let mut state = self.get_reserves(pool, block).await?;
            state.block_hash = hash;
            updates.push(PoolUpdate {
                pool_id: pool.id.clone(),
                state,
                log_index: index,
            });
        }
        updates.sort_by_key(|update| (update.state.block_number, update.log_index));
        Ok(updates)
    }

    async fn eth_call(&self, to: Address, data: Bytes, block: u64) -> DexV2Result<Bytes> {
        let result = self
            .rpc(
                "eth_call",
                json!([{
            "to": format!("{to:#x}"), "data": format!("0x{}", hex_encode(&data)),
        }, format!("0x{block:x}")]),
            )
            .await?;
        parse_hex_bytes(
            result
                .as_str()
                .ok_or_else(|| DexV2Error::Rpc("invalid eth_call result".into()))?,
        )
    }

    async fn estimate_gas(
        &self,
        from: Address,
        to: Address,
        data: Bytes,
        block: u64,
    ) -> DexV2Result<u64> {
        let result = self
            .rpc(
                "eth_estimateGas",
                json!([{
                    "from": format!("{from:#x}"), "to": format!("{to:#x}"),
                    "data": format!("0x{}", hex_encode(&data)),
                }, format!("0x{block:x}")]),
            )
            .await?;
        parse_hex_u64(
            result
                .as_str()
                .ok_or_else(|| DexV2Error::Rpc("invalid eth_estimateGas result".into()))?,
        )
    }
}

#[derive(Debug, Default)]
pub struct MockConnector {
    latest: Mutex<u64>,
    gas_price: Mutex<U256>,
    states: Mutex<HashMap<PoolId, V2PoolState>>,
    affected: Mutex<HashSet<PoolId>>,
    pairs: Mutex<HashMap<(Address, Address, Address), Address>>,
    call_result: Mutex<Bytes>,
    gas_estimate: Mutex<u64>,
}

impl MockConnector {
    pub fn new(block: u64) -> Self {
        Self {
            latest: Mutex::new(block),
            ..Self::default()
        }
    }
    pub fn set_block(&self, block: u64) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = block;
        }
    }
    pub fn set_gas_price(&self, gas_price: U256) {
        if let Ok(mut value) = self.gas_price.lock() {
            *value = gas_price;
        }
    }
    pub fn set_v2_pair(&self, factory: Address, token_a: Address, token_b: Address, pair: Address) {
        let (token0, token1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        if let Ok(mut pairs) = self.pairs.lock() {
            pairs.insert((factory, token0, token1), pair);
        }
    }
    pub fn set_call_result(&self, result: Bytes) {
        if let Ok(mut value) = self.call_result.lock() {
            *value = result;
        }
    }
    pub fn set_gas_estimate(&self, gas: u64) {
        if let Ok(mut value) = self.gas_estimate.lock() {
            *value = gas;
        }
    }
    pub fn set_state(&self, pool_id: PoolId, state: V2PoolState) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(pool_id.clone(), state);
        }
        if let Ok(mut affected) = self.affected.lock() {
            affected.insert(pool_id);
        }
    }
}

#[async_trait]
impl ChainConnector for MockConnector {
    async fn head_block(&self, _mode: ConfirmationMode) -> DexV2Result<u64> {
        self.latest
            .lock()
            .map(|value| *value)
            .map_err(|_| DexV2Error::Rpc("mock lock poisoned".into()))
    }
    async fn gas_price(&self) -> DexV2Result<U256> {
        self.gas_price
            .lock()
            .map(|value| *value)
            .map_err(|_| DexV2Error::Rpc("mock gas price lock poisoned".into()))
    }
    async fn v2_pair(
        &self,
        factory: Address,
        token_a: Address,
        token_b: Address,
    ) -> DexV2Result<Address> {
        let (token0, token1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        self.pairs
            .lock()
            .map_err(|_| DexV2Error::Rpc("mock pair lock poisoned".into()))
            .map(|pairs| {
                pairs
                    .get(&(factory, token0, token1))
                    .copied()
                    .unwrap_or(Address::ZERO)
            })
    }
    async fn get_reserves(&self, pool: &V2Pool, _block: u64) -> DexV2Result<V2PoolState> {
        self.states
            .lock()
            .map_err(|_| DexV2Error::Rpc("mock lock poisoned".into()))?
            .get(&pool.id)
            .cloned()
            .ok_or_else(|| DexV2Error::Rpc(format!("missing mock state for {}", pool.name)))
    }
    async fn poll_pool_updates(
        &self,
        pools: &[V2Pool],
        _from: u64,
        _to: u64,
    ) -> DexV2Result<Vec<PoolUpdate>> {
        let mut affected = self
            .affected
            .lock()
            .map_err(|_| DexV2Error::Rpc("mock lock poisoned".into()))?;
        let states = self
            .states
            .lock()
            .map_err(|_| DexV2Error::Rpc("mock lock poisoned".into()))?;
        let mut result = Vec::new();
        for pool in pools {
            if affected.remove(&pool.id) {
                if let Some(state) = states.get(&pool.id) {
                    result.push(PoolUpdate {
                        pool_id: pool.id.clone(),
                        state: state.clone(),
                        log_index: 0,
                    });
                }
            }
        }
        Ok(result)
    }
    async fn eth_call(&self, _to: Address, _data: Bytes, _block: u64) -> DexV2Result<Bytes> {
        self.call_result
            .lock()
            .map(|value| value.clone())
            .map_err(|_| DexV2Error::Rpc("mock call result lock poisoned".into()))
    }
    async fn estimate_gas(
        &self,
        _from: Address,
        _to: Address,
        _data: Bytes,
        _block: u64,
    ) -> DexV2Result<u64> {
        self.gas_estimate
            .lock()
            .map(|value| *value)
            .map_err(|_| DexV2Error::Rpc("mock gas estimate lock poisoned".into()))
    }
}

fn field_str<'a>(value: &'a Value, field: &str) -> DexV2Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| DexV2Error::Rpc(format!("missing log field {field}")))
}
fn parse_address_field(value: &Value, field: &str) -> DexV2Result<Address> {
    field_str(value, field)?
        .parse()
        .map_err(|e| DexV2Error::Rpc(format!("invalid address: {e}")))
}
fn parse_b256(value: &str) -> DexV2Result<B256> {
    value
        .parse()
        .map_err(|e| DexV2Error::Rpc(format!("invalid block hash: {e}")))
}
fn parse_hex_u64(value: &str) -> DexV2Result<u64> {
    u64::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|e| DexV2Error::Rpc(format!("invalid hex integer {value}: {e}")))
}
fn parse_hex_u256(value: &str) -> DexV2Result<U256> {
    U256::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|e| DexV2Error::Rpc(format!("invalid hex integer {value}: {e}")))
}
fn parse_hex_bytes(value: &str) -> DexV2Result<Bytes> {
    let hex = value.trim_start_matches("0x");
    if hex.len() % 2 != 0 {
        return Err(DexV2Error::Rpc("odd-length hex bytes".into()));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for index in (0..hex.len()).step_by(2) {
        bytes.push(
            u8::from_str_radix(&hex[index..index + 2], 16)
                .map_err(|e| DexV2Error::Rpc(format!("invalid hex bytes: {e}")))?,
        );
    }
    Ok(Bytes::from(bytes))
}
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
