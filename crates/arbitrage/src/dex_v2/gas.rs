use async_trait::async_trait;

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
