use std::path::Path;
use std::str::FromStr;

use alloy_primitives::{Address, B256, I256, U256, keccak256};
use serde::Deserialize;

use crate::dex_router::{RouterError, RouterResult};

use super::inventory::InventoryLedger;

#[derive(Debug, Clone, Deserialize)]
pub struct CrossChainPaperFileConfig {
    #[serde(default)]
    pub enabled: bool,
    pub minimum_net_profit_anchor: String,
    pub minimum_roi_bps: u32,
    pub max_quote_age_ms: i64,
    pub bridge_risk_buffer_bps: u32,
    pub snapshot: CrossChainSnapshotFileConfig,
    #[serde(default)]
    pub inventory: Vec<InventoryFileConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrossChainSnapshotFileConfig {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub anchor_token_source: String,
    pub anchor_token_destination: String,
    pub asset_token_source: String,
    pub asset_token_destination: String,
    pub source_anchor_in: String,
    pub source_asset_out: String,
    pub destination_asset_in: String,
    pub destination_anchor_out: String,
    pub source_gas_anchor: String,
    pub destination_gas_anchor: String,
    pub rebalance_cost_anchor: String,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InventoryFileConfig {
    pub chain_id: u64,
    pub token: String,
    pub amount: String,
}

impl CrossChainPaperFileConfig {
    pub fn load(path: impl AsRef<Path>) -> RouterResult<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            RouterError::Configuration(format!("读取 {} 失败：{error}", path.display()))
        })?;
        toml::from_str(&text).map_err(|error| {
            RouterError::Configuration(format!("解析 {} 失败：{error}", path.display()))
        })
    }

    pub fn build(
        &self,
    ) -> RouterResult<(
        CrossChainPaperDetector,
        CrossChainMarketSnapshot,
        InventoryLedger,
    )> {
        let detector = CrossChainPaperDetector::new(CrossChainPaperConfig {
            minimum_net_profit_anchor: parse_u256(&self.minimum_net_profit_anchor)?,
            minimum_roi_bps: self.minimum_roi_bps,
            max_quote_age_ms: self.max_quote_age_ms,
            bridge_risk_buffer_bps: self.bridge_risk_buffer_bps,
        })?;
        let snapshot = CrossChainMarketSnapshot {
            source_chain_id: self.snapshot.source_chain_id,
            destination_chain_id: self.snapshot.destination_chain_id,
            anchor_token_source: parse_address(&self.snapshot.anchor_token_source)?,
            anchor_token_destination: parse_address(&self.snapshot.anchor_token_destination)?,
            asset_token_source: parse_address(&self.snapshot.asset_token_source)?,
            asset_token_destination: parse_address(&self.snapshot.asset_token_destination)?,
            source_anchor_in: parse_u256(&self.snapshot.source_anchor_in)?,
            source_asset_out: parse_u256(&self.snapshot.source_asset_out)?,
            destination_asset_in: parse_u256(&self.snapshot.destination_asset_in)?,
            destination_anchor_out: parse_u256(&self.snapshot.destination_anchor_out)?,
            source_gas_anchor: parse_u256(&self.snapshot.source_gas_anchor)?,
            destination_gas_anchor: parse_u256(&self.snapshot.destination_gas_anchor)?,
            rebalance_cost_anchor: parse_u256(&self.snapshot.rebalance_cost_anchor)?,
            observed_at_ms: self.snapshot.observed_at_ms,
        };
        let mut inventory = InventoryLedger::default();
        for item in &self.inventory {
            inventory.set(
                item.chain_id,
                parse_address(&item.token)?,
                parse_u256(&item.amount)?,
            );
        }
        Ok((detector, snapshot, inventory))
    }
}

fn parse_address(value: &str) -> RouterResult<Address> {
    Address::from_str(value)
        .map_err(|error| RouterError::Configuration(format!("地址 {value} 无效：{error}")))
}

fn parse_u256(value: &str) -> RouterResult<U256> {
    U256::from_str(value)
        .map_err(|error| RouterError::Configuration(format!("整数 {value} 无效：{error}")))
}

#[derive(Debug, Clone)]
pub struct CrossChainPaperConfig {
    pub minimum_net_profit_anchor: U256,
    pub minimum_roi_bps: u32,
    pub max_quote_age_ms: i64,
    pub bridge_risk_buffer_bps: u32,
}

impl Default for CrossChainPaperConfig {
    fn default() -> Self {
        Self {
            minimum_net_profit_anchor: U256::from(1_000_000u64),
            minimum_roi_bps: 10,
            max_quote_age_ms: 10_000,
            bridge_risk_buffer_bps: 20,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrossChainMarketSnapshot {
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub anchor_token_source: Address,
    pub anchor_token_destination: Address,
    pub asset_token_source: Address,
    pub asset_token_destination: Address,
    pub source_anchor_in: U256,
    pub source_asset_out: U256,
    pub destination_asset_in: U256,
    pub destination_anchor_out: U256,
    pub source_gas_anchor: U256,
    pub destination_gas_anchor: U256,
    pub rebalance_cost_anchor: U256,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CrossChainPaperOpportunity {
    pub id: B256,
    pub source_chain_id: u64,
    pub destination_chain_id: u64,
    pub gross_profit_anchor: U256,
    pub total_cost_anchor: U256,
    pub net_profit_anchor: I256,
    pub roi_bps: i64,
    pub inventory_model: &'static str,
}

pub struct CrossChainPaperDetector {
    config: CrossChainPaperConfig,
}

impl CrossChainPaperDetector {
    pub fn new(config: CrossChainPaperConfig) -> RouterResult<Self> {
        if config.minimum_roi_bps > 10_000
            || config.bridge_risk_buffer_bps > 10_000
            || config.max_quote_age_ms <= 0
        {
            return Err(RouterError::Configuration("跨链纸面套利配置无效".into()));
        }
        Ok(Self { config })
    }

    pub fn detect(
        &self,
        snapshot: &CrossChainMarketSnapshot,
        inventory: &InventoryLedger,
        now_ms: i64,
    ) -> RouterResult<Option<CrossChainPaperOpportunity>> {
        if snapshot.source_chain_id == snapshot.destination_chain_id
            || snapshot.source_anchor_in.is_zero()
            || snapshot.source_asset_out < snapshot.destination_asset_in
            || now_ms < snapshot.observed_at_ms
            || now_ms - snapshot.observed_at_ms > self.config.max_quote_age_ms
        {
            return Ok(None);
        }
        if inventory.balance(snapshot.source_chain_id, snapshot.anchor_token_source)
            < snapshot.source_anchor_in
            || inventory.balance(
                snapshot.destination_chain_id,
                snapshot.asset_token_destination,
            ) < snapshot.destination_asset_in
        {
            return Ok(None);
        }
        let gross = snapshot
            .destination_anchor_out
            .saturating_sub(snapshot.source_anchor_in);
        let bridge_risk = snapshot
            .rebalance_cost_anchor
            .checked_mul(U256::from(self.config.bridge_risk_buffer_bps))
            .map(|value| value / U256::from(10_000u64))
            .ok_or_else(|| RouterError::Quote("跨链风险缓冲溢出".into()))?;
        let costs = snapshot
            .source_gas_anchor
            .checked_add(snapshot.destination_gas_anchor)
            .and_then(|value| value.checked_add(snapshot.rebalance_cost_anchor))
            .and_then(|value| value.checked_add(bridge_risk))
            .ok_or_else(|| RouterError::Quote("跨链成本溢出".into()))?;
        let net = if gross >= costs {
            I256::from_raw(gross - costs)
        } else {
            -I256::from_raw(costs - gross)
        };
        let roi = if snapshot.source_anchor_in.is_zero() {
            0
        } else {
            let magnitude = net.unsigned_abs() * U256::from(10_000u64) / snapshot.source_anchor_in;
            let magnitude = i64::try_from(magnitude).unwrap_or(i64::MAX);
            if net.is_negative() {
                -magnitude
            } else {
                magnitude
            }
        };
        if net < I256::from_raw(self.config.minimum_net_profit_anchor)
            || roi < i64::from(self.config.minimum_roi_bps)
        {
            return Ok(None);
        }
        let mut id_material = Vec::new();
        id_material.extend_from_slice(&snapshot.source_chain_id.to_be_bytes());
        id_material.extend_from_slice(&snapshot.destination_chain_id.to_be_bytes());
        id_material.extend_from_slice(snapshot.anchor_token_source.as_slice());
        id_material.extend_from_slice(snapshot.anchor_token_destination.as_slice());
        id_material.extend_from_slice(&snapshot.source_anchor_in.to_be_bytes::<32>());
        id_material.extend_from_slice(&snapshot.observed_at_ms.to_be_bytes());
        Ok(Some(CrossChainPaperOpportunity {
            id: keccak256(id_material),
            source_chain_id: snapshot.source_chain_id,
            destination_chain_id: snapshot.destination_chain_id,
            gross_profit_anchor: gross,
            total_cost_anchor: costs,
            net_profit_anchor: net,
            roi_bps: roi,
            inventory_model: "prepositioned_inventory",
        }))
    }

    pub fn apply_paper_execution(
        &self,
        snapshot: &CrossChainMarketSnapshot,
        inventory: &mut InventoryLedger,
    ) -> RouterResult<()> {
        let source_debit = snapshot
            .source_anchor_in
            .checked_add(snapshot.source_gas_anchor)
            .ok_or_else(|| RouterError::Quote("源链纸面扣款溢出".into()))?;
        inventory.apply_delta(
            snapshot.source_chain_id,
            snapshot.anchor_token_source,
            U256::ZERO,
            source_debit,
        )?;
        inventory.apply_delta(
            snapshot.source_chain_id,
            snapshot.asset_token_source,
            snapshot.source_asset_out,
            U256::ZERO,
        )?;
        inventory.apply_delta(
            snapshot.destination_chain_id,
            snapshot.asset_token_destination,
            U256::ZERO,
            snapshot.destination_asset_in,
        )?;
        inventory.apply_delta(
            snapshot.destination_chain_id,
            snapshot.anchor_token_destination,
            snapshot.destination_anchor_out,
            snapshot.destination_gas_anchor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_profitable_prepositioned_inventory_opportunity() {
        let anchor = Address::from([1u8; 20]);
        let source_asset = Address::from([2u8; 20]);
        let destination_asset = Address::from([3u8; 20]);
        let mut inventory = InventoryLedger::default();
        inventory.set(137, anchor, U256::from(10_000_000u64));
        inventory.set(8453, destination_asset, U256::from(1_000u64));
        let snapshot = CrossChainMarketSnapshot {
            source_chain_id: 137,
            destination_chain_id: 8453,
            anchor_token_source: anchor,
            anchor_token_destination: anchor,
            asset_token_source: source_asset,
            asset_token_destination: destination_asset,
            source_anchor_in: U256::from(1_000_000u64),
            source_asset_out: U256::from(1_000u64),
            destination_asset_in: U256::from(1_000u64),
            destination_anchor_out: U256::from(1_100_000u64),
            source_gas_anchor: U256::from(1_000u64),
            destination_gas_anchor: U256::from(1_000u64),
            rebalance_cost_anchor: U256::from(5_000u64),
            observed_at_ms: 1_000,
        };
        let detector = CrossChainPaperDetector::new(CrossChainPaperConfig {
            minimum_net_profit_anchor: U256::from(1),
            minimum_roi_bps: 1,
            ..CrossChainPaperConfig::default()
        })
        .unwrap();
        let opportunity = detector
            .detect(&snapshot, &inventory, 1_001)
            .unwrap()
            .unwrap();
        assert!(opportunity.net_profit_anchor.is_positive());
        assert_eq!(opportunity.inventory_model, "prepositioned_inventory");
    }
}
