use alloy_primitives::{Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RouterResult<T> = Result<T, RouterError>;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("路由配置错误: {0}")]
    Configuration(String),
    #[error("路由构造错误: {0}")]
    Route(String),
    #[error("协议报价错误: {0}")]
    Quote(String),
    #[error("RPC 错误: {0}")]
    Rpc(String),
    #[error("聚合器错误: {0}")]
    Aggregator(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    UniswapV2,
    UniswapV3,
    StableSwap,
    WeightedPool,
    Aggregator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityEdge {
    pub id: String,
    pub chain_id: u64,
    pub venue: String,
    pub provider_id: String,
    pub protocol: ProtocolKind,
    pub token_in: Address,
    pub token_out: Address,
    /// 仅用于候选筛选；最终盈亏必须使用协议整数报价。
    pub marginal_rate_after_fee: f64,
    pub estimated_gas_units: u64,
}

impl LiquidityEdge {
    pub fn validate(&self) -> RouterResult<()> {
        if self.id.trim().is_empty()
            || self.venue.trim().is_empty()
            || self.provider_id.trim().is_empty()
            || self.chain_id == 0
            || self.token_in == self.token_out
            || !self.marginal_rate_after_fee.is_finite()
            || self.marginal_rate_after_fee <= 0.0
        {
            return Err(RouterError::Configuration(format!(
                "无效流动性边 {}",
                self.id
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProtocolRoute {
    pub id: B256,
    pub chain_id: u64,
    pub anchor_token: Address,
    pub legs: Vec<LiquidityEdge>,
    pub theoretical_edge_bps: i64,
}

impl MultiProtocolRoute {
    pub fn validate(&self) -> RouterResult<()> {
        if !(2..=4).contains(&self.legs.len()) {
            return Err(RouterError::Route("循环路由跳数必须为 2 到 4".into()));
        }
        if self.legs.first().map(|leg| leg.token_in) != Some(self.anchor_token)
            || self.legs.last().map(|leg| leg.token_out) != Some(self.anchor_token)
        {
            return Err(RouterError::Route("循环路由未回到 anchor".into()));
        }
        for (index, leg) in self.legs.iter().enumerate() {
            leg.validate()?;
            if leg.chain_id != self.chain_id
                || (index > 0 && self.legs[index - 1].token_out != leg.token_in)
            {
                return Err(RouterError::Route("路由断开或跨链混接".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ExactLegQuote {
    pub edge_id: String,
    pub provider_id: String,
    pub amount_in: U256,
    pub amount_out: U256,
    pub gas_units: u64,
    pub block_number: u64,
    pub target: Option<Address>,
    pub calldata: Option<Bytes>,
}

#[derive(Debug, Clone)]
pub struct ExactRouteQuote {
    pub route_id: B256,
    pub amount_in: U256,
    pub amount_out: U256,
    pub gross_profit: U256,
    pub total_gas_units: u64,
    pub block_number: u64,
    pub legs: Vec<ExactLegQuote>,
}
