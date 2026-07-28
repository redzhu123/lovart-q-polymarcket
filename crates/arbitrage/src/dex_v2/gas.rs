use std::sync::{Arc, Mutex};

use alloy_primitives::U256;
use async_trait::async_trait;

use super::connector::ChainConnector;
use super::error::{DexV2Error, DexV2Result};
use super::types::{ArbitrageRoute, ExecutionRequest, SimulationResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasEstimateSource {
    HopFallback,
    Simulation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GasEstimate {
    pub gas_units: u64,
    pub source: GasEstimateSource,
}

#[async_trait]
pub trait GasPriceOracle: Send + Sync {
    async fn gas_price(&self, block: u64) -> DexV2Result<U256>;
}

#[derive(Debug, Clone)]
pub struct FixedGasPriceOracle {
    gas_price: U256,
}

impl FixedGasPriceOracle {
    pub fn new(gas_price: U256) -> Self {
        Self { gas_price }
    }
}

#[async_trait]
impl GasPriceOracle for FixedGasPriceOracle {
    async fn gas_price(&self, _block: u64) -> DexV2Result<U256> {
        Ok(self.gas_price)
    }
}

pub struct RpcGasPriceOracle {
    connector: Arc<dyn ChainConnector>,
    buffer_bps: u32,
    cache: Mutex<Option<(u64, U256)>>,
}

impl RpcGasPriceOracle {
    pub fn new(connector: Arc<dyn ChainConnector>, buffer_bps: u32) -> DexV2Result<Self> {
        if buffer_bps > 10_000 {
            return Err(DexV2Error::Configuration(
                "gas price buffer exceeds 10000 bps".into(),
            ));
        }
        Ok(Self {
            connector,
            buffer_bps,
            cache: Mutex::new(None),
        })
    }
}

#[async_trait]
impl GasPriceOracle for RpcGasPriceOracle {
    async fn gas_price(&self, block: u64) -> DexV2Result<U256> {
        if let Some(value) = self
            .cache
            .lock()
            .map_err(|_| DexV2Error::Rpc("gas price cache lock poisoned".into()))?
            .filter(|(cached_block, _)| *cached_block == block)
            .map(|(_, value)| value)
        {
            return Ok(value);
        }
        let raw = self.connector.gas_price().await?;
        let value = raw
            .checked_mul(U256::from(10_000u64 + u64::from(self.buffer_bps)))
            .map(|value| value.div_ceil(U256::from(10_000u64)))
            .ok_or_else(|| DexV2Error::Quote("gas price buffer overflow".into()))?;
        *self
            .cache
            .lock()
            .map_err(|_| DexV2Error::Rpc("gas price cache lock poisoned".into()))? =
            Some((block, value));
        Ok(value)
    }
}

#[async_trait]
pub trait GasEstimator: Send + Sync {
    async fn estimate_route(
        &self,
        route: &ArbitrageRoute,
        execution_request: &ExecutionRequest,
        simulation: Option<&SimulationResult>,
    ) -> DexV2Result<GasEstimate>;
}

pub struct HopGasEstimator {
    pub two_hop_fallback_gas: u64,
    pub three_hop_fallback_gas: u64,
    pub four_hop_fallback_gas: u64,
    pub gas_units_buffer_bps: u32,
}

#[async_trait]
impl GasEstimator for HopGasEstimator {
    async fn estimate_route(
        &self,
        route: &ArbitrageRoute,
        execution_request: &ExecutionRequest,
        simulation: Option<&SimulationResult>,
    ) -> DexV2Result<GasEstimate> {
        if route.hop_count() != execution_request.steps.len() {
            return Err(DexV2Error::Execution("gas request step mismatch".into()));
        }
        let (base, source) = if let Some(result) =
            simulation.filter(|result| result.success && result.gas_used > 0)
        {
            (result.gas_used, GasEstimateSource::Simulation)
        } else {
            match route.hop_count() {
                2 => (self.two_hop_fallback_gas, GasEstimateSource::HopFallback),
                3 => (self.three_hop_fallback_gas, GasEstimateSource::HopFallback),
                4 => (self.four_hop_fallback_gas, GasEstimateSource::HopFallback),
                count => {
                    return Err(DexV2Error::Execution(format!(
                        "unsupported gas hop count {count}"
                    )));
                }
            }
        };
        let gas_units = base
            .checked_mul(10_000u64 + u64::from(self.gas_units_buffer_bps))
            .ok_or_else(|| DexV2Error::Execution("gas buffer overflow".into()))?
            .div_ceil(10_000);
        Ok(GasEstimate { gas_units, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex_v2::MockConnector;

    #[tokio::test]
    async fn rpc_gas_price_applies_configured_buffer() {
        let connector = Arc::new(MockConnector::new(1));
        connector.set_gas_price(U256::from(30_000_000_000u64));
        let oracle = RpcGasPriceOracle::new(connector, 1_000).unwrap();
        assert_eq!(
            oracle.gas_price(1).await.unwrap(),
            U256::from(33_000_000_000u64)
        );
    }
}
