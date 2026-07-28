use std::collections::HashMap;
use std::sync::Arc;

use alloy_primitives::U256;
use async_trait::async_trait;

use super::types::{
    ExactLegQuote, ExactRouteQuote, LiquidityEdge, MultiProtocolRoute, RouterError, RouterResult,
};

#[async_trait]
pub trait ProtocolQuoteProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn quote_exact_input(
        &self,
        edge: &LiquidityEdge,
        amount_in: U256,
        block_number: u64,
    ) -> RouterResult<ExactLegQuote>;
}

#[derive(Default)]
pub struct QuoteProviderRegistry {
    providers: HashMap<String, Arc<dyn ProtocolQuoteProvider>>,
}

impl QuoteProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn ProtocolQuoteProvider>) -> RouterResult<()> {
        let id = provider.provider_id().trim().to_ascii_lowercase();
        if id.is_empty() || self.providers.insert(id.clone(), provider).is_some() {
            return Err(RouterError::Configuration(format!(
                "报价 Provider 重复或为空: {id}"
            )));
        }
        Ok(())
    }

    pub async fn quote_route(
        &self,
        route: &MultiProtocolRoute,
        amount_in: U256,
        block_number: u64,
    ) -> RouterResult<ExactRouteQuote> {
        route.validate()?;
        if amount_in.is_zero() {
            return Err(RouterError::Quote("输入金额不能为零".into()));
        }
        let mut current = amount_in;
        let mut total_gas_units = 0u64;
        let mut legs = Vec::with_capacity(route.legs.len());
        for edge in &route.legs {
            let provider = self
                .providers
                .get(&edge.provider_id.to_ascii_lowercase())
                .ok_or_else(|| {
                    RouterError::Configuration(format!("未注册报价 Provider: {}", edge.provider_id))
                })?;
            let quote = provider
                .quote_exact_input(edge, current, block_number)
                .await?;
            if quote.amount_in != current || quote.amount_out.is_zero() {
                return Err(RouterError::Quote(format!(
                    "Provider {} 返回了不连续报价",
                    edge.provider_id
                )));
            }
            current = quote.amount_out;
            total_gas_units = total_gas_units
                .checked_add(quote.gas_units)
                .ok_or_else(|| RouterError::Quote("Gas 用量溢出".into()))?;
            legs.push(quote);
        }
        Ok(ExactRouteQuote {
            route_id: route.id,
            amount_in,
            amount_out: current,
            gross_profit: current.saturating_sub(amount_in),
            total_gas_units,
            block_number,
            legs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, B256};

    use crate::dex_router::ProtocolKind;

    struct DoubleProvider;

    #[async_trait]
    impl ProtocolQuoteProvider for DoubleProvider {
        fn provider_id(&self) -> &str {
            "double"
        }

        async fn quote_exact_input(
            &self,
            edge: &LiquidityEdge,
            amount_in: U256,
            block_number: u64,
        ) -> RouterResult<ExactLegQuote> {
            Ok(ExactLegQuote {
                edge_id: edge.id.clone(),
                provider_id: "double".into(),
                amount_in,
                amount_out: amount_in * U256::from(2),
                gas_units: 10,
                block_number,
                target: None,
                calldata: None,
            })
        }
    }

    #[tokio::test]
    async fn chains_integer_quotes_across_protocol_providers() {
        let anchor = Address::from([1u8; 20]);
        let other = Address::from([2u8; 20]);
        let leg = |id: &str, from, to| LiquidityEdge {
            id: id.into(),
            chain_id: 1,
            venue: "test".into(),
            provider_id: "double".into(),
            protocol: ProtocolKind::Aggregator,
            token_in: from,
            token_out: to,
            marginal_rate_after_fee: 2.0,
            estimated_gas_units: 10,
        };
        let route = MultiProtocolRoute {
            id: B256::ZERO,
            chain_id: 1,
            anchor_token: anchor,
            legs: vec![leg("a", anchor, other), leg("b", other, anchor)],
            theoretical_edge_bps: 30_000,
        };
        let mut registry = QuoteProviderRegistry::new();
        registry.register(Arc::new(DoubleProvider)).unwrap();
        let quote = registry
            .quote_route(&route, U256::from(10), 100)
            .await
            .unwrap();
        assert_eq!(quote.amount_out, U256::from(40));
        assert_eq!(quote.total_gas_units, 20);
    }
}
