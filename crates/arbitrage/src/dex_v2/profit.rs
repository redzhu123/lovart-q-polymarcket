use std::sync::{Arc, Mutex};

use alloy_primitives::{I256, U256};
use async_trait::async_trait;

use super::connector::ChainConnector;
use super::error::{DexV2Error, DexV2Result};
use super::types::{CostEstimate, ProfitBreakdown, TokenId, V2Pool};

#[async_trait]
pub trait NativePriceOracle: Send + Sync {
    async fn quote_native_to_token(
        &self,
        token: &TokenId,
        native_amount: U256,
        block: u64,
    ) -> DexV2Result<U256>;
}

#[derive(Debug, Clone)]
pub struct FixedNativePriceOracle {
    /// Anchor smallest units per 1e18 native smallest units.
    anchor_per_native: U256,
}

impl FixedNativePriceOracle {
    pub fn new(anchor_per_native: U256) -> Self {
        Self { anchor_per_native }
    }
}

#[async_trait]
impl NativePriceOracle for FixedNativePriceOracle {
    async fn quote_native_to_token(
        &self,
        _token: &TokenId,
        native_amount: U256,
        _block: u64,
    ) -> DexV2Result<U256> {
        native_amount
            .checked_mul(self.anchor_per_native)
            .map(|value| value / U256::from(10u64).pow(U256::from(18)))
            .ok_or_else(|| DexV2Error::Quote("native price conversion overflow".into()))
    }
}

pub struct V2PoolNativePriceOracle {
    connector: Arc<dyn ChainConnector>,
    pool: V2Pool,
    native_token: TokenId,
    anchor_token: TokenId,
    cache: Mutex<Option<(u64, U256)>>,
}

impl V2PoolNativePriceOracle {
    pub fn new(
        connector: Arc<dyn ChainConnector>,
        pool: V2Pool,
        native_token: TokenId,
        anchor_token: TokenId,
    ) -> DexV2Result<Self> {
        let valid_pair = (pool.token0 == native_token && pool.token1 == anchor_token)
            || (pool.token1 == native_token && pool.token0 == anchor_token);
        if !valid_pair {
            return Err(DexV2Error::Configuration(
                "native price pool token order is invalid".into(),
            ));
        }
        Ok(Self {
            connector,
            pool,
            native_token,
            anchor_token,
            cache: Mutex::new(None),
        })
    }

    async fn anchor_per_native(&self, block: u64) -> DexV2Result<U256> {
        if let Some(value) = self
            .cache
            .lock()
            .map_err(|_| DexV2Error::Rpc("native price cache lock poisoned".into()))?
            .filter(|(cached_block, _)| *cached_block == block)
            .map(|(_, value)| value)
        {
            return Ok(value);
        }
        let state = self.connector.get_reserves(&self.pool, block).await?;
        let (native_reserve, anchor_reserve) = if self.pool.token0 == self.native_token {
            (state.reserve0, state.reserve1)
        } else {
            (state.reserve1, state.reserve0)
        };
        if native_reserve.is_zero() || anchor_reserve.is_zero() {
            return Err(DexV2Error::Quote(
                "native price pool has empty reserves".into(),
            ));
        }
        let one_native = U256::from(10u64).pow(U256::from(18));
        let value = one_native
            .checked_mul(anchor_reserve)
            .map(|value| value / native_reserve)
            .ok_or_else(|| DexV2Error::Quote("native price conversion overflow".into()))?;
        *self
            .cache
            .lock()
            .map_err(|_| DexV2Error::Rpc("native price cache lock poisoned".into()))? =
            Some((block, value));
        Ok(value)
    }
}

#[async_trait]
impl NativePriceOracle for V2PoolNativePriceOracle {
    async fn quote_native_to_token(
        &self,
        token: &TokenId,
        native_amount: U256,
        block: u64,
    ) -> DexV2Result<U256> {
        if token != &self.anchor_token {
            return Err(DexV2Error::Quote(
                "native price oracle does not support requested anchor".into(),
            ));
        }
        native_amount
            .checked_mul(self.anchor_per_native(block).await?)
            .map(|value| value / U256::from(10u64).pow(U256::from(18)))
            .ok_or_else(|| DexV2Error::Quote("native price conversion overflow".into()))
    }
}

pub trait ProfitEngine: Send + Sync {
    fn calculate(
        &self,
        amount_in: U256,
        amount_out: U256,
        costs: &CostEstimate,
    ) -> DexV2Result<ProfitBreakdown>;
}

#[derive(Debug, Default)]
pub struct V2ProfitEngine;

impl ProfitEngine for V2ProfitEngine {
    fn calculate(
        &self,
        amount_in: U256,
        amount_out: U256,
        costs: &CostEstimate,
    ) -> DexV2Result<ProfitBreakdown> {
        if amount_in.is_zero() {
            return Err(DexV2Error::Quote("profit amount_in is zero".into()));
        }
        let gross_profit = amount_out.saturating_sub(amount_in);
        let total_cost = costs
            .anchor_token_cost
            .checked_add(costs.builder_tip_anchor)
            .and_then(|value| value.checked_add(costs.risk_buffer_anchor))
            .ok_or_else(|| DexV2Error::Quote("profit cost overflow".into()))?;
        let net_profit = signed_difference(gross_profit, total_cost);
        let roi_magnitude = gross_profit
            .abs_diff(total_cost)
            .checked_mul(U256::from(10_000))
            .ok_or_else(|| DexV2Error::Quote("ROI overflow".into()))?
            / amount_in;
        let roi_bps = if gross_profit >= total_cost {
            positive_i256(roi_magnitude)?
        } else {
            -positive_i256(roi_magnitude)?
        };
        Ok(ProfitBreakdown {
            amount_in,
            amount_out,
            gross_profit,
            gas_anchor: costs.anchor_token_cost,
            tip_anchor: costs.builder_tip_anchor,
            risk_buffer: costs.risk_buffer_anchor,
            net_profit,
            roi_bps,
        })
    }
}

pub fn positive_i256(value: U256) -> DexV2Result<I256> {
    if value.bit_len() > 255 {
        return Err(DexV2Error::Quote("signed amount overflow".into()));
    }
    Ok(I256::from_raw(value))
}

pub fn signed_difference(left: U256, right: U256) -> I256 {
    if left >= right {
        I256::from_raw(left - right)
    } else {
        -I256::from_raw(right - left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    use alloy_primitives::Address;

    use crate::dex_v2::{MockConnector, PoolId, Protocol, V2PoolState};

    #[test]
    fn calculates_positive_and_negative_net_profit() {
        let engine = V2ProfitEngine;
        let mut costs = CostEstimate {
            gas_units: 1,
            max_fee_per_gas: U256::ZERO,
            priority_fee_per_gas: U256::ZERO,
            native_token_cost: U256::ZERO,
            anchor_token_cost: U256::from(5),
            builder_tip_anchor: U256::ZERO,
            risk_buffer_anchor: U256::ZERO,
        };
        assert!(
            engine
                .calculate(U256::from(100), U256::from(120), &costs)
                .unwrap()
                .net_profit
                > I256::ZERO
        );
        costs.anchor_token_cost = U256::from(30);
        assert!(
            engine
                .calculate(U256::from(100), U256::from(120), &costs)
                .unwrap()
                .net_profit
                < I256::ZERO
        );
    }

    #[tokio::test]
    async fn v2_pool_oracle_uses_integer_reserve_ratio() {
        let native = TokenId {
            chain_id: 137,
            address: Address::from([1u8; 20]),
        };
        let anchor = TokenId {
            chain_id: 137,
            address: Address::from([2u8; 20]),
        };
        let pool = V2Pool {
            id: PoolId {
                chain_id: 137,
                address: Address::from([3u8; 20]),
            },
            name: "wpol_usdc".into(),
            protocol: Protocol::UniswapV2Compatible {
                factory: Address::from([4u8; 20]),
                router: None,
            },
            token0: native.clone(),
            token1: anchor.clone(),
            fee_numerator: 997,
            fee_denominator: 1000,
        };
        let connector = Arc::new(MockConnector::new(10));
        connector.set_state(
            pool.id.clone(),
            V2PoolState {
                reserve0: U256::from(1_000_000u64) * U256::from(10u64).pow(U256::from(18)),
                reserve1: U256::from(250_000_000_000u64),
                block_number: 10,
                block_hash: None,
                updated_at: SystemTime::now(),
            },
        );
        let oracle = V2PoolNativePriceOracle::new(connector, pool, native, anchor.clone()).unwrap();
        assert_eq!(
            oracle
                .quote_native_to_token(&anchor, U256::from(10u64).pow(U256::from(18)), 10)
                .await
                .unwrap(),
            U256::from(250_000u64)
        );
    }
}
