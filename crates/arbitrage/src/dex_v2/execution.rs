use alloy_primitives::{Address, Bytes, U256, keccak256};

use super::error::{DexV2Error, DexV2Result};
use super::types::{
    ArbitrageRoute, EncodedCall, ExecutionRequest, ExecutionStep, RouteKind, RouteQuote,
};

pub struct ExecutionRequestBuilder {
    pub default_leg_slippage_bps: u32,
    pub three_hop_leg_slippage_bps: u32,
    pub max_steps: usize,
}

impl ExecutionRequestBuilder {
    pub fn build(
        &self,
        route: &ArbitrageRoute,
        quote: &RouteQuote,
        min_profit: U256,
        deadline: u64,
    ) -> DexV2Result<ExecutionRequest> {
        if route.hop_count() < 2
            || route.hop_count() > self.max_steps
            || quote.leg_quotes.len() != route.hop_count()
        {
            return Err(DexV2Error::Execution(format!(
                "route {} has invalid execution step count {}",
                route.id.0,
                route.hop_count()
            )));
        }
        let slippage = match route.kind {
            RouteKind::TwoHop => self.default_leg_slippage_bps,
            RouteKind::ThreeHop => self.three_hop_leg_slippage_bps,
            RouteKind::FourHop => self.three_hop_leg_slippage_bps,
        };
        let mut steps = Vec::with_capacity(route.hop_count());
        for (index, (leg, leg_quote)) in route.legs.iter().zip(&quote.leg_quotes).enumerate() {
            if leg.index as usize != index
                || leg.token_in != leg_quote.token_in
                || leg.token_out != leg_quote.token_out
                || leg.pool_id != leg_quote.pool_id
                || (index > 0 && route.legs[index - 1].token_out != leg.token_in)
            {
                return Err(DexV2Error::Execution(format!(
                    "route {} execution legs are disconnected at {index}",
                    route.id.0
                )));
            }
            let min_amount_out = leg_quote
                .amount_out
                .checked_mul(U256::from(10_000u32 - slippage))
                .ok_or_else(|| DexV2Error::Execution("minAmountOut overflow".into()))?
                / U256::from(10_000);
            steps.push(ExecutionStep {
                pair: leg.pool_id.address,
                token_in: leg.token_in.address,
                token_out: leg.token_out.address,
                min_amount_out,
            });
        }
        if steps.first().map(|step| step.token_in) != Some(route.anchor_token.address)
            || steps.last().map(|step| step.token_out) != Some(route.anchor_token.address)
        {
            return Err(DexV2Error::Execution(format!(
                "route {} execution is not closed",
                route.id.0
            )));
        }
        Ok(ExecutionRequest {
            route_id: route.id.clone(),
            anchor_token: route.anchor_token.clone(),
            amount_in: quote.amount_in,
            min_profit,
            deadline,
            steps,
        })
    }

    pub fn encode(
        &self,
        executor: Address,
        request: &ExecutionRequest,
    ) -> DexV2Result<EncodedCall> {
        if !(2..=4).contains(&request.steps.len()) || request.steps.len() > self.max_steps {
            return Err(DexV2Error::Execution(
                "executor only accepts 2 or 3 steps".into(),
            ));
        }
        let selector = &keccak256(
            "executeV2Arbitrage(address,uint256,uint256,uint256,(address,address,address,uint256)[])",
        )[..4];
        let mut data = Vec::with_capacity(4 + 32 * (6 + request.steps.len() * 4));
        data.extend_from_slice(selector);
        push_address(&mut data, request.anchor_token.address);
        push_u256(&mut data, request.amount_in);
        push_u256(&mut data, request.min_profit);
        push_u256(&mut data, U256::from(request.deadline));
        push_u256(&mut data, U256::from(32 * 5));
        push_u256(&mut data, U256::from(request.steps.len()));
        for step in &request.steps {
            push_address(&mut data, step.pair);
            push_address(&mut data, step.token_in);
            push_address(&mut data, step.token_out);
            push_u256(&mut data, step.min_amount_out);
        }
        Ok(EncodedCall {
            to: executor,
            data: Bytes::from(data),
            value: U256::ZERO,
        })
    }
}

fn push_address(output: &mut Vec<u8>, address: Address) {
    output.extend_from_slice(&[0u8; 12]);
    output.extend_from_slice(address.as_slice());
}

fn push_u256(output: &mut Vec<u8>, value: U256) {
    output.extend_from_slice(&value.to_be_bytes::<32>());
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Address;

    use super::super::types::{LegQuote, OptimizationMethod, PoolId, RouteId, SwapLeg, TokenId};
    use super::*;

    #[test]
    fn builds_and_encodes_three_steps_with_leg_slippage() {
        let tokens = [1u8, 2, 3].map(|last| TokenId {
            chain_id: 1,
            address: Address::with_last_byte(last),
        });
        let legs = (0..3)
            .map(|index| SwapLeg {
                index: index as u8,
                pool_id: PoolId {
                    chain_id: 1,
                    address: Address::with_last_byte(10 + index as u8),
                },
                token_in: tokens[index].clone(),
                token_out: tokens[(index + 1) % 3].clone(),
            })
            .collect::<Vec<_>>();
        let route =
            ArbitrageRoute::new(RouteId::default(), 1, tokens[0].clone(), legs.clone()).unwrap();
        let leg_quotes = legs
            .iter()
            .map(|leg| LegQuote {
                leg_index: leg.index,
                pool_id: leg.pool_id.clone(),
                token_in: leg.token_in.clone(),
                token_out: leg.token_out.clone(),
                amount_in: U256::from(1000),
                amount_out: U256::from(1000),
            })
            .collect();
        let quote = RouteQuote {
            route_id: route.id.clone(),
            block_number: 1,
            amount_in: U256::from(1000),
            amount_out: U256::from(1010),
            leg_quotes,
            price_impacts: vec![],
            gross_profit: alloy_primitives::I256::try_from(10i64).unwrap(),
        };
        let builder = ExecutionRequestBuilder {
            default_leg_slippage_bps: 10,
            three_hop_leg_slippage_bps: 15,
            max_steps: 3,
        };
        let request = builder.build(&route, &quote, U256::from(5), 100).unwrap();
        assert_eq!(request.steps.len(), 3);
        assert_eq!(request.steps[0].min_amount_out, U256::from(998));
        let call = builder
            .encode(Address::with_last_byte(99), &request)
            .unwrap();
        assert_eq!(call.data.len(), 4 + 32 * (6 + 12));
        assert_eq!(quote.route_id, RouteId::default());
        let _ = OptimizationMethod::CoarseToFine;
    }
}
