use std::collections::HashMap;
use std::sync::Arc;

use alloy_primitives::{Address, Bytes, U256};
use async_trait::async_trait;

use crate::dex_v2::ChainConnector;

use super::quote::ProtocolQuoteProvider;
use super::types::{ExactLegQuote, LiquidityEdge, ProtocolKind, RouterError, RouterResult};

pub struct UniswapV3QuoterProvider {
    id: String,
    connector: Arc<dyn ChainConnector>,
    quoter: Address,
    fee_tiers: HashMap<String, u32>,
}

impl UniswapV3QuoterProvider {
    pub fn new(
        id: impl Into<String>,
        connector: Arc<dyn ChainConnector>,
        quoter: Address,
        fee_tiers: HashMap<String, u32>,
    ) -> RouterResult<Self> {
        let id = id.into();
        if id.trim().is_empty()
            || quoter.is_zero()
            || fee_tiers.values().any(|fee| *fee == 0 || *fee > 1_000_000)
        {
            return Err(RouterError::Configuration(
                "Uniswap V3 Quoter 配置无效".into(),
            ));
        }
        Ok(Self {
            id,
            connector,
            quoter,
            fee_tiers,
        })
    }
}

#[async_trait]
impl ProtocolQuoteProvider for UniswapV3QuoterProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }

    async fn quote_exact_input(
        &self,
        edge: &LiquidityEdge,
        amount_in: U256,
        block_number: u64,
    ) -> RouterResult<ExactLegQuote> {
        if edge.protocol != ProtocolKind::UniswapV3 {
            return Err(RouterError::Quote("V3 Provider 收到了非 V3 边".into()));
        }
        let fee = self
            .fee_tiers
            .get(&edge.id)
            .copied()
            .ok_or_else(|| RouterError::Configuration(format!("V3 边缺少费率: {}", edge.id)))?;
        // quoteExactInputSingle(address,address,uint24,uint256,uint160)
        let mut data = Vec::with_capacity(164);
        data.extend_from_slice(&[0xf7, 0x72, 0x9d, 0x43]);
        push_address(&mut data, edge.token_in);
        push_address(&mut data, edge.token_out);
        data.extend_from_slice(&U256::from(fee).to_be_bytes::<32>());
        data.extend_from_slice(&amount_in.to_be_bytes::<32>());
        data.extend_from_slice(&[0u8; 32]);
        let result = self
            .connector
            .eth_call(self.quoter, Bytes::from(data), block_number)
            .await
            .map_err(|error| RouterError::Rpc(error.to_string()))?;
        if result.len() < 32 {
            return Err(RouterError::Quote(format!(
                "V3 Quoter 返回过短: {}",
                edge.id
            )));
        }
        let amount_out = U256::from_be_slice(&result[..32]);
        if amount_out.is_zero() {
            return Err(RouterError::Quote("V3 Quoter 返回零输出".into()));
        }
        Ok(ExactLegQuote {
            edge_id: edge.id.clone(),
            provider_id: self.id.clone(),
            amount_in,
            amount_out,
            gas_units: edge.estimated_gas_units,
            block_number,
            target: None,
            calldata: None,
        })
    }
}

fn push_address(output: &mut Vec<u8>, address: Address) {
    output.extend_from_slice(&[0u8; 12]);
    output.extend_from_slice(address.as_slice());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex_v2::MockConnector;

    #[tokio::test]
    async fn quotes_v3_edge_at_requested_block() {
        let connector = Arc::new(MockConnector::new(100));
        connector.set_call_result(Bytes::from(U256::from(1234).to_be_bytes::<32>().to_vec()));
        let mut fees = HashMap::new();
        fees.insert("pool".into(), 3000);
        let provider =
            UniswapV3QuoterProvider::new("v3", connector, Address::from([9u8; 20]), fees).unwrap();
        let edge = LiquidityEdge {
            id: "pool".into(),
            chain_id: 1,
            venue: "uniswap-v3".into(),
            provider_id: "v3".into(),
            protocol: ProtocolKind::UniswapV3,
            token_in: Address::from([1u8; 20]),
            token_out: Address::from([2u8; 20]),
            marginal_rate_after_fee: 1.0,
            estimated_gas_units: 100_000,
        };
        let quote = provider
            .quote_exact_input(&edge, U256::from(100), 99)
            .await
            .unwrap();
        assert_eq!(quote.amount_out, U256::from(1234));
        assert_eq!(quote.block_number, 99);
    }
}
