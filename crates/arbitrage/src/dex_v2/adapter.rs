use alloy_primitives::{Bytes, U256};

use super::error::{DexV2Error, DexV2Result};
use super::types::{
    ChainLog, EncodedCall, PoolUpdate, SwapQuote, SwapRequest, TokenId, V2Pool, V2PoolState,
};

pub trait PoolAdapter: Send + Sync {
    fn protocol_name(&self) -> &'static str;
    fn tokens(&self, pool: &V2Pool) -> DexV2Result<(TokenId, TokenId)>;
    fn quote_exact_in(
        &self,
        pool: &V2Pool,
        state: &V2PoolState,
        token_in: &TokenId,
        amount_in: U256,
    ) -> DexV2Result<SwapQuote>;
    fn apply_log(&self, pool: &V2Pool, log: &ChainLog) -> DexV2Result<PoolUpdate>;
    fn encode_swap(&self, request: &SwapRequest, amount_out: U256) -> DexV2Result<EncodedCall>;
}

#[derive(Debug, Default)]
pub struct UniswapV2Adapter;

impl UniswapV2Adapter {
    pub fn new() -> Self {
        Self
    }

    pub fn get_amount_out(
        amount_in: U256,
        reserve_in: U256,
        reserve_out: U256,
        fee_numerator: u32,
        fee_denominator: u32,
    ) -> DexV2Result<U256> {
        if amount_in.is_zero() {
            return Err(DexV2Error::Quote("amount_in is zero".into()));
        }
        if reserve_in.is_zero() || reserve_out.is_zero() {
            return Err(DexV2Error::Quote("pool reserve is zero".into()));
        }
        if fee_denominator == 0 || fee_numerator >= fee_denominator {
            return Err(DexV2Error::Quote("invalid pool fee".into()));
        }
        let amount_with_fee = amount_in
            .checked_mul(U256::from(fee_numerator))
            .ok_or_else(|| DexV2Error::Quote("amount_in_with_fee overflow".into()))?;
        let numerator = amount_with_fee
            .checked_mul(reserve_out)
            .ok_or_else(|| DexV2Error::Quote("amount_out numerator overflow".into()))?;
        let denominator = reserve_in
            .checked_mul(U256::from(fee_denominator))
            .and_then(|value| value.checked_add(amount_with_fee))
            .ok_or_else(|| DexV2Error::Quote("amount_out denominator overflow".into()))?;
        Ok(numerator / denominator)
    }
}

impl PoolAdapter for UniswapV2Adapter {
    fn protocol_name(&self) -> &'static str {
        "uniswap_v2_compatible"
    }

    fn tokens(&self, pool: &V2Pool) -> DexV2Result<(TokenId, TokenId)> {
        Ok((pool.token0.clone(), pool.token1.clone()))
    }

    fn quote_exact_in(
        &self,
        pool: &V2Pool,
        state: &V2PoolState,
        token_in: &TokenId,
        amount_in: U256,
    ) -> DexV2Result<SwapQuote> {
        let (reserve_in, reserve_out) = if token_in == &pool.token0 {
            (state.reserve0, state.reserve1)
        } else if token_in == &pool.token1 {
            (state.reserve1, state.reserve0)
        } else {
            return Err(DexV2Error::Quote(format!(
                "token {:?} is not in pool {}",
                token_in.address, pool.name
            )));
        };
        let amount_out = Self::get_amount_out(
            amount_in,
            reserve_in,
            reserve_out,
            pool.fee_numerator,
            pool.fee_denominator,
        )?;
        let fee_amount = amount_in
            .checked_mul(U256::from(pool.fee_denominator - pool.fee_numerator))
            .map(|value| value / U256::from(pool.fee_denominator));
        Ok(SwapQuote {
            amount_in,
            amount_out,
            fee_amount,
        })
    }

    fn apply_log(&self, pool: &V2Pool, log: &ChainLog) -> DexV2Result<PoolUpdate> {
        if log.address != pool.id.address || log.data.len() < 64 {
            return Err(DexV2Error::PoolState(format!(
                "invalid Sync log for pool {}",
                pool.name
            )));
        }
        let reserve0 = U256::from_be_slice(&log.data[..32]);
        let reserve1 = U256::from_be_slice(&log.data[32..64]);
        Ok(PoolUpdate {
            pool_id: pool.id.clone(),
            state: V2PoolState {
                reserve0,
                reserve1,
                block_number: log.block_number,
                block_hash: log.block_hash,
                updated_at: std::time::SystemTime::now(),
            },
            log_index: log.log_index,
        })
    }

    fn encode_swap(&self, request: &SwapRequest, amount_out: U256) -> DexV2Result<EncodedCall> {
        if request.token_in != request.pool.token0 && request.token_in != request.pool.token1 {
            return Err(DexV2Error::Execution("swap token is not in pool".into()));
        }
        if amount_out < request.min_amount_out {
            return Err(DexV2Error::Execution(
                "quoted output is below min_amount_out".into(),
            ));
        }
        let (amount0_out, amount1_out) = if request.token_in == request.pool.token0 {
            (U256::ZERO, amount_out)
        } else {
            (amount_out, U256::ZERO)
        };
        let mut data = Vec::with_capacity(164);
        data.extend_from_slice(&[0x02, 0x2c, 0x0d, 0x9f]);
        data.extend_from_slice(&amount0_out.to_be_bytes::<32>());
        data.extend_from_slice(&amount1_out.to_be_bytes::<32>());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(request.recipient.as_slice());
        data.extend_from_slice(&U256::from(128).to_be_bytes::<32>());
        data.extend_from_slice(&U256::ZERO.to_be_bytes::<32>());
        Ok(EncodedCall {
            to: request.pool.id.address,
            data: Bytes::from(data),
            value: U256::ZERO,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_uniswap_v2_router_vector() {
        let out = UniswapV2Adapter::get_amount_out(
            U256::from(1_000u64),
            U256::from(10_000u64),
            U256::from(20_000u64),
            997,
            1000,
        )
        .unwrap();
        assert_eq!(out, U256::from(1_813u64));
    }

    #[test]
    fn rejects_zero_and_overflow() {
        assert!(
            UniswapV2Adapter::get_amount_out(U256::ZERO, U256::from(1), U256::from(1), 997, 1000)
                .is_err()
        );
        assert!(
            UniswapV2Adapter::get_amount_out(U256::from(1), U256::ZERO, U256::from(1), 997, 1000)
                .is_err()
        );
        assert!(
            UniswapV2Adapter::get_amount_out(U256::MAX, U256::from(1), U256::MAX, 997, 1000)
                .is_err()
        );
    }

    #[test]
    fn supports_custom_fee_and_rounds_down() {
        let out = UniswapV2Adapter::get_amount_out(
            U256::from(1u64),
            U256::from(3u64),
            U256::from(10u64),
            9_975,
            10_000,
        )
        .unwrap();
        assert_eq!(out, U256::from(2u64));
    }
}
