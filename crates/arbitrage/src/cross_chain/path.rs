use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use alloy_primitives::{Address, I256, U256, keccak256};
use serde::Deserialize;

use crate::dex_router::{RouterError, RouterResult};
use crate::dex_v2::{V2Pool, V2PoolState};
use crate::multi_chain::MultiChainSupervisor;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuperGraphNode {
    pub chain_id: u64,
    pub asset_id: String,
    pub token: Address,
    pub decimals: u8,
}

#[derive(Debug, Clone)]
pub enum SuperEdgeKind {
    Swap {
        pool: V2Pool,
        state: Arc<V2PoolState>,
        gas_cost_anchor: U256,
    },
    Bridge {
        provider: String,
        fee_bps: u32,
        fixed_cost_anchor: U256,
        estimated_seconds: u64,
    },
}

#[derive(Debug, Clone)]
pub struct SuperGraphEdge {
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub weight: f64,
    pub kind: SuperEdgeKind,
}

#[derive(Debug, Clone)]
pub struct CrossChainOptimalRoute {
    pub id: alloy_primitives::B256,
    pub start: SuperGraphNode,
    pub nodes: Vec<SuperGraphNode>,
    pub edges: Vec<SuperGraphEdge>,
    pub amount_in: U256,
    pub amount_out: U256,
    pub gross_profit: I256,
    pub total_cost_anchor: U256,
    pub net_profit: I256,
    pub roi_bps: i64,
    pub theoretical_profit_bps: i64,
    pub bridge_count: usize,
    pub estimated_bridge_seconds: u64,
    pub execution_model: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrossChainPathConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub anchor_asset: String,
    pub input_amount_anchor: String,
    pub minimum_net_profit_anchor: String,
    #[serde(default = "default_min_roi")]
    pub minimum_roi_bps: u32,
    #[serde(default = "default_min_theoretical")]
    pub minimum_theoretical_profit_bps: u32,
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    #[serde(default = "default_max_bridges")]
    pub max_bridges: usize,
    #[serde(default = "default_risk_buffer")]
    pub risk_buffer_bps: u32,
    #[serde(default)]
    pub tokens: Vec<CrossChainTokenConfig>,
    #[serde(default)]
    pub chain_costs: Vec<ChainCostConfig>,
    #[serde(default)]
    pub bridges: Vec<BridgePathConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrossChainTokenConfig {
    pub chain_id: u64,
    pub asset_id: String,
    pub address: Address,
    pub decimals: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainCostConfig {
    pub chain_id: u64,
    pub swap_gas_anchor: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgePathConfig {
    pub provider: String,
    pub asset_id: String,
    pub chains: Vec<u64>,
    pub fee_bps: u32,
    pub fixed_cost_anchor: String,
    #[serde(default)]
    pub estimated_seconds: u64,
}

fn default_true() -> bool {
    true
}
fn default_min_roi() -> u32 {
    10
}
fn default_min_theoretical() -> u32 {
    5
}
fn default_max_steps() -> usize {
    6
}
fn default_max_bridges() -> usize {
    2
}
fn default_risk_buffer() -> u32 {
    20
}

impl CrossChainPathConfig {
    pub fn load(path: impl AsRef<Path>) -> RouterResult<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            RouterError::Configuration(format!("读取 {} 失败：{error}", path.display()))
        })?;
        let config: Self = toml::from_str(&text).map_err(|error| {
            RouterError::Configuration(format!("解析 {} 失败：{error}", path.display()))
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> RouterResult<()> {
        if self.anchor_asset.trim().is_empty()
            || !(3..=10).contains(&self.max_steps)
            || self.max_bridges < 2
            || self.max_bridges > self.max_steps
            || self.minimum_roi_bps > 10_000
            || self.minimum_theoretical_profit_bps > 10_000
            || self.risk_buffer_bps > 10_000
            || self.tokens.is_empty()
            || self.bridges.is_empty()
        {
            return Err(RouterError::Configuration("跨链最短路径配置无效".into()));
        }
        parse_u256(&self.input_amount_anchor)?;
        parse_u256(&self.minimum_net_profit_anchor)?;
        for cost in &self.chain_costs {
            parse_u256(&cost.swap_gas_anchor)?;
        }
        for bridge in &self.bridges {
            if bridge.provider.trim().is_empty()
                || bridge.asset_id.trim().is_empty()
                || bridge.chains.len() < 2
                || bridge.fee_bps > 10_000
            {
                return Err(RouterError::Configuration("跨链桥边配置无效".into()));
            }
            parse_u256(&bridge.fixed_cost_anchor)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct PathState {
    weight: f64,
    edge_ids: Vec<usize>,
    bridge_count: usize,
}

pub struct CrossChainPathScanner {
    config: CrossChainPathConfig,
}

impl CrossChainPathScanner {
    pub fn new(config: CrossChainPathConfig) -> RouterResult<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn scan(
        &self,
        supervisor: &MultiChainSupervisor,
    ) -> RouterResult<Vec<CrossChainOptimalRoute>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }
        let (nodes, edges) = self.build_graph(supervisor)?;
        let outgoing = build_outgoing(nodes.len(), &edges);
        let mut routes = Vec::new();
        for (start_index, start) in nodes.iter().enumerate() {
            if start.asset_id != self.config.anchor_asset {
                continue;
            }
            if let Some(path) = self.shortest_profitable_cycle(start_index, &edges, &outgoing) {
                if let Some(route) = self.exact_quote(start_index, path, &nodes, &edges)? {
                    routes.push(route);
                }
            }
        }
        routes.sort_by_key(|route| std::cmp::Reverse(route.net_profit));
        Ok(routes)
    }

    fn build_graph(
        &self,
        supervisor: &MultiChainSupervisor,
    ) -> RouterResult<(Vec<SuperGraphNode>, Vec<SuperGraphEdge>)> {
        let mut nodes = Vec::new();
        let mut node_index = HashMap::<(u64, Address), usize>::new();
        for token in &self.config.tokens {
            if node_index.contains_key(&(token.chain_id, token.address)) {
                return Err(RouterError::Configuration(format!(
                    "跨链代币映射重复：{} {:#x}",
                    token.chain_id, token.address
                )));
            }
            let index = nodes.len();
            nodes.push(SuperGraphNode {
                chain_id: token.chain_id,
                asset_id: token.asset_id.clone(),
                token: token.address,
                decimals: token.decimals,
            });
            node_index.insert((token.chain_id, token.address), index);
        }
        let gas_costs = self
            .config
            .chain_costs
            .iter()
            .map(|cost| Ok((cost.chain_id, parse_u256(&cost.swap_gas_anchor)?)))
            .collect::<RouterResult<HashMap<_, _>>>()?;
        let mut edges = Vec::new();
        for chain in supervisor.chains() {
            for pool in chain.engine.registry.pools() {
                let Some(state) = chain
                    .engine
                    .state
                    .get(&pool.id)
                    .map_err(|error| RouterError::Quote(error.to_string()))?
                else {
                    continue;
                };
                let Some(&token0) = node_index.get(&(chain.chain_id, pool.token0.address)) else {
                    continue;
                };
                let Some(&token1) = node_index.get(&(chain.chain_id, pool.token1.address)) else {
                    continue;
                };
                let gas = gas_costs.get(&chain.chain_id).copied().unwrap_or_default();
                add_swap_edge(&mut edges, &nodes, token0, token1, pool, state.clone(), gas)?;
                add_swap_edge(&mut edges, &nodes, token1, token0, pool, state, gas)?;
            }
        }
        for bridge in &self.config.bridges {
            let fixed_cost = parse_u256(&bridge.fixed_cost_anchor)?;
            for &from_chain in &bridge.chains {
                for &to_chain in &bridge.chains {
                    if from_chain == to_chain {
                        continue;
                    }
                    let from = nodes.iter().position(|node| {
                        node.chain_id == from_chain && node.asset_id == bridge.asset_id
                    });
                    let to = nodes.iter().position(|node| {
                        node.chain_id == to_chain && node.asset_id == bridge.asset_id
                    });
                    let (Some(from), Some(to)) = (from, to) else {
                        continue;
                    };
                    let rate = f64::from(10_000 - bridge.fee_bps) / 10_000.0;
                    edges.push(SuperGraphEdge {
                        id: format!(
                            "bridge:{}:{}:{}:{}",
                            bridge.provider, bridge.asset_id, from_chain, to_chain
                        ),
                        from,
                        to,
                        weight: -rate.ln(),
                        kind: SuperEdgeKind::Bridge {
                            provider: bridge.provider.clone(),
                            fee_bps: bridge.fee_bps,
                            fixed_cost_anchor: fixed_cost,
                            estimated_seconds: bridge.estimated_seconds,
                        },
                    });
                }
            }
        }
        Ok((nodes, edges))
    }

    fn shortest_profitable_cycle(
        &self,
        start: usize,
        edges: &[SuperGraphEdge],
        outgoing: &[Vec<usize>],
    ) -> Option<PathState> {
        let mut current = HashMap::<(usize, usize), PathState>::new();
        current.insert(
            (start, 0),
            PathState {
                weight: 0.0,
                edge_ids: Vec::new(),
                bridge_count: 0,
            },
        );
        let threshold =
            -(1.0 + f64::from(self.config.minimum_theoretical_profit_bps) / 10_000.0).ln();
        let mut best: Option<PathState> = None;
        for _ in 0..self.config.max_steps {
            let mut next = HashMap::<(usize, usize), PathState>::new();
            for ((node, _), path) in current {
                for &edge_index in &outgoing[node] {
                    let edge = &edges[edge_index];
                    let bridges = path.bridge_count
                        + usize::from(matches!(&edge.kind, SuperEdgeKind::Bridge { .. }));
                    if bridges > self.config.max_bridges {
                        continue;
                    }
                    let mut candidate = path.clone();
                    candidate.weight += edge.weight;
                    candidate.bridge_count = bridges;
                    candidate.edge_ids.push(edge_index);
                    if edge.to == start
                        && bridges >= 2
                        && candidate.edge_ids.len() >= 3
                        && candidate.weight < threshold
                        && best
                            .as_ref()
                            .is_none_or(|current| candidate.weight < current.weight)
                    {
                        best = Some(candidate.clone());
                    }
                    let key = (edge.to, bridges);
                    if next
                        .get(&key)
                        .is_none_or(|current| candidate.weight < current.weight)
                    {
                        next.insert(key, candidate);
                    }
                }
            }
            current = next;
        }
        best
    }

    fn exact_quote(
        &self,
        start_index: usize,
        path: PathState,
        nodes: &[SuperGraphNode],
        edges: &[SuperGraphEdge],
    ) -> RouterResult<Option<CrossChainOptimalRoute>> {
        let amount_in = parse_u256(&self.config.input_amount_anchor)?;
        let mut amount = amount_in;
        let mut total_cost = U256::ZERO;
        let mut route_edges = Vec::with_capacity(path.edge_ids.len());
        let mut route_nodes = vec![nodes[start_index].clone()];
        let mut bridge_seconds = 0u64;
        let theoretical_weight = path.weight;
        for edge_index in path.edge_ids {
            let edge = edges[edge_index].clone();
            amount = match &edge.kind {
                SuperEdgeKind::Swap {
                    pool,
                    state,
                    gas_cost_anchor,
                } => {
                    total_cost = checked_add(total_cost, *gas_cost_anchor, "Swap Gas 成本")?;
                    quote_v2(pool, state, &nodes[edge.from], amount)?
                }
                SuperEdgeKind::Bridge {
                    fee_bps,
                    fixed_cost_anchor,
                    estimated_seconds,
                    ..
                } => {
                    total_cost = checked_add(total_cost, *fixed_cost_anchor, "桥固定成本")?;
                    bridge_seconds = bridge_seconds.saturating_add(*estimated_seconds);
                    quote_bridge(
                        amount,
                        nodes[edge.from].decimals,
                        nodes[edge.to].decimals,
                        *fee_bps,
                    )?
                }
            };
            route_nodes.push(nodes[edge.to].clone());
            route_edges.push(edge);
        }
        let gross = signed_difference(amount, amount_in);
        let positive_gross = amount.saturating_sub(amount_in);
        let risk_buffer = positive_gross
            .checked_mul(U256::from(self.config.risk_buffer_bps))
            .map(|value| value / U256::from(10_000u64))
            .ok_or_else(|| RouterError::Quote("跨链风险缓冲溢出".into()))?;
        total_cost = checked_add(total_cost, risk_buffer, "风险缓冲")?;
        let net = if amount >= amount_in.saturating_add(total_cost) {
            I256::from_raw(amount - amount_in - total_cost)
        } else {
            -I256::from_raw(amount_in.saturating_add(total_cost).saturating_sub(amount))
        };
        let min_profit = I256::from_raw(parse_u256(&self.config.minimum_net_profit_anchor)?);
        let roi = signed_roi_bps(net, amount_in);
        if net < min_profit || roi < i64::from(self.config.minimum_roi_bps) {
            return Ok(None);
        }
        let theoretical = ((-theoretical_weight).exp() - 1.0) * 10_000.0;
        let id_material = route_edges
            .iter()
            .flat_map(|edge| edge.id.as_bytes())
            .copied()
            .collect::<Vec<_>>();
        Ok(Some(CrossChainOptimalRoute {
            id: keccak256(id_material),
            start: nodes[start_index].clone(),
            nodes: route_nodes,
            bridge_count: route_edges
                .iter()
                .filter(|edge| matches!(&edge.kind, SuperEdgeKind::Bridge { .. }))
                .count(),
            edges: route_edges,
            amount_in,
            amount_out: amount,
            gross_profit: gross,
            total_cost_anchor: total_cost,
            net_profit: net,
            roi_bps: roi,
            theoretical_profit_bps: theoretical.round() as i64,
            estimated_bridge_seconds: bridge_seconds,
            execution_model: "prepositioned_inventory_non_atomic",
        }))
    }
}

fn add_swap_edge(
    edges: &mut Vec<SuperGraphEdge>,
    nodes: &[SuperGraphNode],
    from: usize,
    to: usize,
    pool: &V2Pool,
    state: Arc<V2PoolState>,
    gas_cost_anchor: U256,
) -> RouterResult<()> {
    let (reserve_in, reserve_out) = if pool.token0.address == nodes[from].token {
        (state.reserve0, state.reserve1)
    } else {
        (state.reserve1, state.reserve0)
    };
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return Ok(());
    }
    let raw_rate = u256_to_f64(reserve_out)? / u256_to_f64(reserve_in)?;
    let decimal_rate = 10f64.powi(i32::from(nodes[from].decimals) - i32::from(nodes[to].decimals));
    let fee_rate = f64::from(pool.fee_numerator) / f64::from(pool.fee_denominator);
    let rate = raw_rate * decimal_rate * fee_rate;
    if !rate.is_finite() || rate <= 0.0 {
        return Ok(());
    }
    edges.push(SuperGraphEdge {
        id: format!(
            "swap:{}:{:#x}:{:#x}",
            pool.id.chain_id, pool.id.address, nodes[from].token
        ),
        from,
        to,
        weight: -rate.ln(),
        kind: SuperEdgeKind::Swap {
            pool: pool.clone(),
            state,
            gas_cost_anchor,
        },
    });
    Ok(())
}

fn build_outgoing(node_count: usize, edges: &[SuperGraphEdge]) -> Vec<Vec<usize>> {
    let mut outgoing = vec![Vec::new(); node_count];
    for (index, edge) in edges.iter().enumerate() {
        outgoing[edge.from].push(index);
    }
    outgoing
}

fn quote_v2(
    pool: &V2Pool,
    state: &V2PoolState,
    from: &SuperGraphNode,
    amount_in: U256,
) -> RouterResult<U256> {
    let (reserve_in, reserve_out) = if pool.token0.address == from.token {
        (state.reserve0, state.reserve1)
    } else {
        (state.reserve1, state.reserve0)
    };
    let amount_with_fee = amount_in
        .checked_mul(U256::from(pool.fee_numerator))
        .ok_or_else(|| RouterError::Quote("V2 输入手续费计算溢出".into()))?;
    let numerator = amount_with_fee
        .checked_mul(reserve_out)
        .ok_or_else(|| RouterError::Quote("V2 输出分子溢出".into()))?;
    let denominator = reserve_in
        .checked_mul(U256::from(pool.fee_denominator))
        .and_then(|value| value.checked_add(amount_with_fee))
        .ok_or_else(|| RouterError::Quote("V2 输出分母溢出".into()))?;
    if denominator.is_zero() {
        return Err(RouterError::Quote("V2 输出分母为零".into()));
    }
    Ok(numerator / denominator)
}

fn quote_bridge(
    amount: U256,
    from_decimals: u8,
    to_decimals: u8,
    fee_bps: u32,
) -> RouterResult<U256> {
    let after_fee = amount
        .checked_mul(U256::from(10_000 - fee_bps))
        .map(|value| value / U256::from(10_000u64))
        .ok_or_else(|| RouterError::Quote("桥费计算溢出".into()))?;
    if to_decimals >= from_decimals {
        after_fee
            .checked_mul(pow10(to_decimals - from_decimals))
            .ok_or_else(|| RouterError::Quote("桥接小数位扩展溢出".into()))
    } else {
        Ok(after_fee / pow10(from_decimals - to_decimals))
    }
}

fn pow10(decimals: u8) -> U256 {
    U256::from(10u64).pow(U256::from(decimals))
}

fn parse_u256(value: &str) -> RouterResult<U256> {
    U256::from_str(value)
        .map_err(|error| RouterError::Configuration(format!("整数 {value} 无效：{error}")))
}

fn u256_to_f64(value: U256) -> RouterResult<f64> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|error| RouterError::Quote(format!("候选汇率转换失败：{error}")))
}

fn checked_add(left: U256, right: U256, label: &str) -> RouterResult<U256> {
    left.checked_add(right)
        .ok_or_else(|| RouterError::Quote(format!("{label}累加溢出")))
}

fn signed_difference(left: U256, right: U256) -> I256 {
    if left >= right {
        I256::from_raw(left - right)
    } else {
        -I256::from_raw(right - left)
    }
}

fn signed_roi_bps(net: I256, amount_in: U256) -> i64 {
    if amount_in.is_zero() {
        return 0;
    }
    let magnitude = net.unsigned_abs().saturating_mul(U256::from(10_000u64)) / amount_in;
    let magnitude = i64::try_from(magnitude).unwrap_or(i64::MAX);
    if net.is_negative() {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_quote_converts_decimals_and_fee() {
        let result = quote_bridge(U256::from(1_000_000u64), 6, 18, 20).unwrap();
        assert_eq!(result, U256::from(998_000_000_000_000_000u64));
    }

    #[test]
    fn example_cross_chain_routing_config_is_valid() {
        let config: CrossChainPathConfig =
            toml::from_str(include_str!("../../../../cross-chain-routing.toml")).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.max_steps, 6);
        assert_eq!(config.max_bridges, 2);
        assert_eq!(config.tokens.len(), 6);
    }

    #[test]
    fn bounded_bellman_ford_finds_cross_chain_negative_cycle() {
        use std::time::SystemTime;

        use crate::dex_v2::{PoolId, Protocol, TokenId};

        let scanner = CrossChainPathScanner::new(CrossChainPathConfig {
            enabled: true,
            anchor_asset: "USDC".into(),
            input_amount_anchor: "1000000".into(),
            minimum_net_profit_anchor: "1".into(),
            minimum_roi_bps: 1,
            minimum_theoretical_profit_bps: 1,
            max_steps: 4,
            max_bridges: 2,
            risk_buffer_bps: 0,
            tokens: vec![CrossChainTokenConfig {
                chain_id: 1,
                asset_id: "USDC".into(),
                address: Address::from([1u8; 20]),
                decimals: 6,
            }],
            chain_costs: Vec::new(),
            bridges: vec![BridgePathConfig {
                provider: "test".into(),
                asset_id: "USDC".into(),
                chains: vec![1, 2],
                fee_bps: 1,
                fixed_cost_anchor: "0".into(),
                estimated_seconds: 1,
            }],
        })
        .unwrap();
        let bridge = |id: &str, from, to| SuperGraphEdge {
            id: id.into(),
            from,
            to,
            weight: -(0.9999f64).ln(),
            kind: SuperEdgeKind::Bridge {
                provider: "test".into(),
                fee_bps: 1,
                fixed_cost_anchor: U256::ZERO,
                estimated_seconds: 1,
            },
        };
        let swap = |id: &str, from, to, rate: f64| SuperGraphEdge {
            id: id.into(),
            from,
            to,
            weight: -rate.ln(),
            kind: SuperEdgeKind::Swap {
                pool: V2Pool {
                    id: PoolId {
                        chain_id: 1,
                        address: Address::from([9u8; 20]),
                    },
                    name: "synthetic".into(),
                    protocol: Protocol::UniswapV2Compatible {
                        factory: Address::ZERO,
                        router: None,
                    },
                    token0: TokenId {
                        chain_id: 1,
                        address: Address::from([1u8; 20]),
                    },
                    token1: TokenId {
                        chain_id: 1,
                        address: Address::from([2u8; 20]),
                    },
                    fee_numerator: 997,
                    fee_denominator: 1000,
                },
                state: Arc::new(V2PoolState {
                    reserve0: U256::from(1_000_000u64),
                    reserve1: U256::from(1_000_000u64),
                    block_number: 1,
                    block_hash: None,
                    updated_at: SystemTime::now(),
                }),
                gas_cost_anchor: U256::ZERO,
            },
        };
        let edges = vec![
            swap("buy", 0, 1, 1.02),
            bridge("out", 1, 2),
            swap("sell", 2, 3, 1.02),
            bridge("back", 3, 0),
        ];
        let outgoing = build_outgoing(4, &edges);
        let path = scanner
            .shortest_profitable_cycle(0, &edges, &outgoing)
            .unwrap();
        assert_eq!(path.edge_ids.len(), 4);
    }
}
