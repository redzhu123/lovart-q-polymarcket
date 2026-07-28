use std::collections::HashMap;
use std::sync::Arc;

use alloy_primitives::{Address, Bytes, U256};
use async_trait::async_trait;

use crate::dex_v2::ChainConnector;

use super::{
    ExactLegQuote, LiquidityEdge, ProtocolKind, ProtocolQuoteProvider, RouterError, RouterResult,
};

#[derive(Debug, Clone, Copy)]
pub struct CurvePoolBinding {
    pub pool: Address,
    pub token_in_index: u8,
    pub token_out_index: u8,
}

pub struct CurveGetDyQuoteProvider {
    id: String,
    connector: Arc<dyn ChainConnector>,
    bindings: HashMap<String, CurvePoolBinding>,
}

impl CurveGetDyQuoteProvider {
    pub fn new(id: impl Into<String>, connector: Arc<dyn ChainConnector>) -> RouterResult<Self> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(RouterError::Configuration(
                "Curve Provider ID 不能为空".into(),
            ));
        }
        Ok(Self {
            id,
            connector,
            bindings: HashMap::new(),
        })
    }

    pub fn bind_edge(&mut self, edge_id: impl Into<String>, binding: CurvePoolBinding) {
        self.bindings.insert(edge_id.into(), binding);
    }
}

#[async_trait]
impl ProtocolQuoteProvider for CurveGetDyQuoteProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    async fn quote_exact_input(
        &self,
        edge: &LiquidityEdge,
        amount_in: U256,
        block_number: u64,
    ) -> RouterResult<ExactLegQuote> {
        if edge.protocol != ProtocolKind::StableSwap {
            return Err(RouterError::Quote(
                "Curve Provider 收到了非 StableSwap 边".into(),
            ));
        }
        let binding = self.bindings.get(&edge.id).ok_or_else(|| {
            RouterError::Configuration(format!("Curve 边 {} 未绑定池与币种索引", edge.id))
        })?;
        let mut data = Vec::with_capacity(100);
        data.extend_from_slice(&[0x5e, 0x0d, 0x44, 0x3f]);
        data.extend_from_slice(&U256::from(binding.token_in_index).to_be_bytes::<32>());
        data.extend_from_slice(&U256::from(binding.token_out_index).to_be_bytes::<32>());
        data.extend_from_slice(&amount_in.to_be_bytes::<32>());
        let raw = self
            .connector
            .eth_call(binding.pool, Bytes::from(data.clone()), block_number)
            .await
            .map_err(|error| RouterError::Quote(format!("Curve get_dy 调用失败：{error}")))?;
        if raw.len() < 32 {
            return Err(RouterError::Quote("Curve get_dy 返回值长度不足".into()));
        }
        let amount_out = U256::from_be_slice(&raw[raw.len() - 32..]);
        Ok(ExactLegQuote {
            edge_id: edge.id.clone(),
            provider_id: self.id.clone(),
            amount_in,
            amount_out,
            gas_units: edge.estimated_gas_units,
            block_number,
            target: Some(binding.pool),
            calldata: Some(Bytes::from(data)),
        })
    }
}
