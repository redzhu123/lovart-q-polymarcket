use std::collections::{HashMap, HashSet};

use alloy_primitives::{Address, keccak256};

use super::types::{LiquidityEdge, MultiProtocolRoute, RouterError, RouterResult};

#[derive(Debug, Clone)]
pub struct CycleSearchConfig {
    pub max_hops: usize,
    pub max_candidates: usize,
    pub max_edges_per_pair: usize,
    pub minimum_theoretical_edge_bps: i64,
}

impl Default for CycleSearchConfig {
    fn default() -> Self {
        Self {
            max_hops: 4,
            max_candidates: 100_000,
            max_edges_per_pair: 8,
            minimum_theoretical_edge_bps: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleSearchStats {
    pub edges_loaded: usize,
    pub candidates_generated: usize,
    pub pruned_edge_limit: usize,
    pub pruned_candidate_limit: usize,
    pub rejected_without_edge: usize,
}

pub struct BoundedCycleFinder {
    config: CycleSearchConfig,
}

impl BoundedCycleFinder {
    pub fn new(config: CycleSearchConfig) -> RouterResult<Self> {
        if !(2..=4).contains(&config.max_hops)
            || config.max_candidates == 0
            || config.max_edges_per_pair == 0
        {
            return Err(RouterError::Configuration("循环搜索限制无效".into()));
        }
        Ok(Self { config })
    }

    pub fn find(
        &self,
        chain_id: u64,
        anchor: Address,
        edges: &[LiquidityEdge],
    ) -> RouterResult<(Vec<MultiProtocolRoute>, CycleSearchStats)> {
        let mut stats = CycleSearchStats::default();
        let mut grouped = HashMap::<(Address, Address), Vec<LiquidityEdge>>::new();
        for edge in edges.iter().filter(|edge| edge.chain_id == chain_id) {
            edge.validate()?;
            stats.edges_loaded += 1;
            grouped
                .entry((edge.token_in, edge.token_out))
                .or_default()
                .push(edge.clone());
        }
        let mut outgoing = HashMap::<Address, Vec<LiquidityEdge>>::new();
        for (_, mut pair_edges) in grouped {
            pair_edges.sort_by(|left, right| {
                right
                    .marginal_rate_after_fee
                    .total_cmp(&left.marginal_rate_after_fee)
                    .then_with(|| left.id.cmp(&right.id))
            });
            stats.pruned_edge_limit += pair_edges
                .len()
                .saturating_sub(self.config.max_edges_per_pair);
            for edge in pair_edges.into_iter().take(self.config.max_edges_per_pair) {
                outgoing.entry(edge.token_in).or_default().push(edge);
            }
        }
        let mut routes = Vec::new();
        let mut path = Vec::new();
        let mut used_tokens = HashSet::from([anchor]);
        let mut used_edges = HashSet::new();
        self.walk(
            chain_id,
            anchor,
            anchor,
            &outgoing,
            &mut path,
            &mut used_tokens,
            &mut used_edges,
            &mut routes,
            &mut stats,
        )?;
        routes.sort_by(|left, right| {
            right
                .theoretical_edge_bps
                .cmp(&left.theoretical_edge_bps)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok((routes, stats))
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        chain_id: u64,
        anchor: Address,
        current: Address,
        outgoing: &HashMap<Address, Vec<LiquidityEdge>>,
        path: &mut Vec<LiquidityEdge>,
        used_tokens: &mut HashSet<Address>,
        used_edges: &mut HashSet<String>,
        routes: &mut Vec<MultiProtocolRoute>,
        stats: &mut CycleSearchStats,
    ) -> RouterResult<()> {
        if path.len() >= self.config.max_hops || routes.len() >= self.config.max_candidates {
            if routes.len() >= self.config.max_candidates {
                stats.pruned_candidate_limit += 1;
            }
            return Ok(());
        }
        for edge in outgoing.get(&current).into_iter().flatten() {
            if used_edges.contains(&edge.id) {
                continue;
            }
            if edge.token_out == anchor {
                if path.is_empty() {
                    continue;
                }
                path.push(edge.clone());
                let edge_bps = theoretical_edge_bps(path);
                if edge_bps >= self.config.minimum_theoretical_edge_bps {
                    let id = route_id(chain_id, anchor, path);
                    let route = MultiProtocolRoute {
                        id,
                        chain_id,
                        anchor_token: anchor,
                        legs: path.clone(),
                        theoretical_edge_bps: edge_bps,
                    };
                    route.validate()?;
                    routes.push(route);
                    stats.candidates_generated += 1;
                } else {
                    stats.rejected_without_edge += 1;
                }
                path.pop();
                continue;
            }
            if used_tokens.contains(&edge.token_out) {
                continue;
            }
            used_tokens.insert(edge.token_out);
            used_edges.insert(edge.id.clone());
            path.push(edge.clone());
            self.walk(
                chain_id,
                anchor,
                edge.token_out,
                outgoing,
                path,
                used_tokens,
                used_edges,
                routes,
                stats,
            )?;
            path.pop();
            used_edges.remove(&edge.id);
            used_tokens.remove(&edge.token_out);
        }
        Ok(())
    }
}

fn theoretical_edge_bps(path: &[LiquidityEdge]) -> i64 {
    let log_rate = path
        .iter()
        .map(|edge| edge.marginal_rate_after_fee.ln())
        .sum::<f64>();
    ((log_rate.exp() - 1.0) * 10_000.0).floor() as i64
}

fn route_id(chain_id: u64, anchor: Address, path: &[LiquidityEdge]) -> alloy_primitives::B256 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&chain_id.to_be_bytes());
    bytes.extend_from_slice(anchor.as_slice());
    for edge in path {
        bytes.extend_from_slice(edge.id.as_bytes());
        bytes.push(0xff);
    }
    keccak256(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex_router::ProtocolKind;

    fn edge(id: &str, from: u8, to: u8, rate: f64) -> LiquidityEdge {
        LiquidityEdge {
            id: id.into(),
            chain_id: 137,
            venue: "test".into(),
            provider_id: "test".into(),
            protocol: ProtocolKind::UniswapV2,
            token_in: Address::from([from; 20]),
            token_out: Address::from([to; 20]),
            marginal_rate_after_fee: rate,
            estimated_gas_units: 80_000,
        }
    }

    #[test]
    fn finds_profitable_four_hop_cycle_and_rejects_repeated_tokens() {
        let anchor = Address::from([1u8; 20]);
        let edges = vec![
            edge("a", 1, 2, 1.01),
            edge("b", 2, 3, 1.01),
            edge("c", 3, 4, 1.01),
            edge("d", 4, 1, 1.01),
            edge("repeat", 3, 2, 2.0),
        ];
        let (routes, stats) = BoundedCycleFinder::new(CycleSearchConfig::default())
            .unwrap()
            .find(137, anchor, &edges)
            .unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].legs.len(), 4);
        assert_eq!(stats.candidates_generated, 1);
    }
}
