use std::sync::Arc;

use alloy_primitives::{Bytes, I256};
use async_trait::async_trait;

use super::connector::ChainConnector;
use super::error::{DexV2Error, DexV2Result};
use super::types::{LegExecutionResult, SimulationRequest, SimulationResult};

#[async_trait]
pub trait SimulationEngine: Send + Sync {
    async fn simulate(&self, request: &SimulationRequest) -> DexV2Result<SimulationResult>;
}

#[derive(Debug, Default)]
pub struct LocalShadowSimulator;

#[async_trait]
impl SimulationEngine for LocalShadowSimulator {
    async fn simulate(&self, request: &SimulationRequest) -> DexV2Result<SimulationResult> {
        if request.expected_profit <= I256::ZERO {
            return Err(DexV2Error::Simulation(
                "expected profit is not positive".into(),
            ));
        }
        let leg_results = request
            .leg_quotes
            .iter()
            .map(|quote| LegExecutionResult {
                leg_index: quote.leg_index,
                pool_id: quote.pool_id.clone(),
                amount_in: quote.amount_in,
                amount_out: quote.amount_out,
            })
            .collect();
        Ok(SimulationResult {
            success: true,
            gas_used: request.gas,
            return_data: Bytes::new(),
            revert_reason: None,
            realized_amount_out: Some(request.expected_amount_out),
            realized_profit: Some(request.expected_profit),
            leg_results,
            block_number: request.block_number,
        })
    }
}

pub struct EthCallSimulator {
    connector: Arc<dyn ChainConnector>,
}

impl EthCallSimulator {
    pub fn new(connector: Arc<dyn ChainConnector>) -> Self {
        Self { connector }
    }
}

#[async_trait]
impl SimulationEngine for EthCallSimulator {
    async fn simulate(&self, request: &SimulationRequest) -> DexV2Result<SimulationResult> {
        if request.to.is_zero() || request.data.is_empty() {
            return Err(DexV2Error::Simulation(
                "atomic eth_call requires executor calldata".into(),
            ));
        }
        let return_data = self
            .connector
            .eth_call(request.to, request.data.clone(), request.block_number)
            .await?;
        let gas_used = self
            .connector
            .estimate_gas(
                request.from,
                request.to,
                request.data.clone(),
                request.block_number,
            )
            .await?;
        Ok(SimulationResult {
            success: true,
            gas_used,
            return_data,
            revert_reason: None,
            realized_amount_out: Some(request.expected_amount_out),
            realized_profit: Some(request.expected_profit),
            leg_results: request
                .leg_quotes
                .iter()
                .map(|quote| LegExecutionResult {
                    leg_index: quote.leg_index,
                    pool_id: quote.pool_id.clone(),
                    amount_in: quote.amount_in,
                    amount_out: quote.amount_out,
                })
                .collect(),
            block_number: request.block_number,
        })
    }
}
