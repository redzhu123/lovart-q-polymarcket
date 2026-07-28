use std::collections::{HashMap, HashSet};

use alloy_primitives::keccak256;

use super::error::{DexV2Error, DexV2Result};
use super::types::{
    ArbitrageRoute, PoolEdge, PoolId, RouteId, SwapLeg, TokenId, TokenMeta, V2Pool,
};

#[derive(Debug, Clone, Default)]
pub struct PoolRegistry {
    pools: HashMap<PoolId, V2Pool>,
    tokens: HashMap<TokenId, TokenMeta>,
}

impl PoolRegistry {
    pub fn new(tokens: Vec<TokenMeta>, pools: Vec<V2Pool>) -> DexV2Result<Self> {
        let tokens = tokens
            .into_iter()
            .map(|token| (token.id.clone(), token))
            .collect::<HashMap<_, _>>();
        let mut registered = HashMap::new();
        for pool in pools {
            if pool.token0.chain_id != pool.id.chain_id
                || pool.token1.chain_id != pool.id.chain_id
                || !tokens.contains_key(&pool.token0)
                || !tokens.contains_key(&pool.token1)
            {
                return Err(DexV2Error::Configuration(format!(
                    "pool {} has unknown or cross-chain token",
                    pool.name
                )));
            }
            if registered.insert(pool.id.clone(), pool).is_some() {
                return Err(DexV2Error::Configuration("duplicate pool id".into()));
            }
        }
        Ok(Self {
            pools: registered,
            tokens,
        })
    }

    pub fn pool(&self, id: &PoolId) -> DexV2Result<&V2Pool> {
        self.pools
            .get(id)
            .ok_or_else(|| DexV2Error::PoolState(format!("unknown pool {:?}", id.address)))
    }
    pub fn pools(&self) -> impl Iterator<Item = &V2Pool> {
        self.pools.values()
    }
    pub fn token(&self, id: &TokenId) -> Option<&TokenMeta> {
        self.tokens.get(id)
    }
    pub fn anchors(&self) -> impl Iterator<Item = &TokenMeta> {
        self.tokens.values().filter(|token| token.anchor)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TokenPoolGraph {
    pub edges: HashMap<(TokenId, TokenId), Vec<PoolEdge>>,
    pub outgoing: HashMap<TokenId, Vec<PoolEdge>>,
}

impl TokenPoolGraph {
    pub fn from_registry(registry: &PoolRegistry) -> Self {
        let mut graph = Self::default();
        for pool in registry.pools() {
            graph.insert(PoolEdge {
                pool_id: pool.id.clone(),
                token_in: pool.token0.clone(),
                token_out: pool.token1.clone(),
            });
            graph.insert(PoolEdge {
                pool_id: pool.id.clone(),
                token_in: pool.token1.clone(),
                token_out: pool.token0.clone(),
            });
        }
        for edges in graph.edges.values_mut() {
            edges.sort_by_key(|edge| edge.pool_id.address);
        }
        for edges in graph.outgoing.values_mut() {
            edges.sort_by_key(|edge| (edge.token_out.address, edge.pool_id.address));
        }
        graph
    }

    fn insert(&mut self, edge: PoolEdge) {
        self.edges
            .entry((edge.token_in.clone(), edge.token_out.clone()))
            .or_default()
            .push(edge.clone());
        self.outgoing
            .entry(edge.token_in.clone())
            .or_default()
            .push(edge);
    }

    pub fn outgoing(&self, token: &TokenId) -> &[PoolEdge] {
        self.outgoing.get(token).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[derive(Debug, Clone)]
pub struct RouteGenerationConfig {
    pub enable_two_hop: bool,
    pub enable_three_hop: bool,
    pub enable_four_hop: bool,
    pub max_route_hops: usize,
    pub max_routes_total: usize,
    pub max_routes_per_anchor: usize,
    pub max_edges_per_token_pair: usize,
    pub allowed_anchor_tokens: HashSet<TokenId>,
    pub allowed_intermediate_tokens: HashSet<TokenId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteGenerationStats {
    pub generated_two_hop: usize,
    pub generated_three_hop: usize,
    pub generated_four_hop: usize,
    pub pruned_duplicate: usize,
    pub pruned_pair_edge_limit: usize,
    pub pruned_anchor_limit: usize,
    pub pruned_total_limit: usize,
}

pub trait RouteGenerator: Send + Sync {
    fn generate_two_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>>;

    fn generate_three_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>>;
    fn generate_four_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>>;
}

pub struct BoundedRouteGenerator<'a> {
    registry: &'a PoolRegistry,
}

impl<'a> BoundedRouteGenerator<'a> {
    pub fn new(registry: &'a PoolRegistry) -> Self {
        Self { registry }
    }

    fn anchors<'b>(
        &'b self,
        config: &'b RouteGenerationConfig,
    ) -> impl Iterator<Item = &'b TokenMeta> {
        self.registry.anchors().filter(|token| {
            config.allowed_anchor_tokens.is_empty()
                || config.allowed_anchor_tokens.contains(&token.id)
        })
    }

    fn allowed_intermediate(&self, token: &TokenId, config: &RouteGenerationConfig) -> bool {
        self.registry.token(token).is_some()
            && (config.allowed_intermediate_tokens.is_empty()
                || config.allowed_intermediate_tokens.contains(token))
    }

    fn limited_edges<'b>(
        &self,
        graph: &'b TokenPoolGraph,
        from: &TokenId,
        to: &TokenId,
        config: &RouteGenerationConfig,
    ) -> &'b [PoolEdge] {
        graph
            .edges
            .get(&(from.clone(), to.clone()))
            .map(|edges| &edges[..edges.len().min(config.max_edges_per_token_pair)])
            .unwrap_or(&[])
    }

    fn make_route(&self, anchor: &TokenId, edges: &[PoolEdge]) -> DexV2Result<ArbitrageRoute> {
        let legs = edges
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, edge)| (index as u8, edge).into())
            .collect::<Vec<SwapLeg>>();
        let id = stable_route_id(anchor.chain_id, anchor, &legs);
        ArbitrageRoute::new(id, anchor.chain_id, anchor.clone(), legs)
    }
}

impl RouteGenerator for BoundedRouteGenerator<'_> {
    fn generate_two_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>> {
        if !config.enable_two_hop {
            return Ok(Vec::new());
        }
        let mut routes = Vec::new();
        for anchor in self.anchors(config) {
            for first in graph.outgoing(&anchor.id) {
                if !self
                    .limited_edges(graph, &anchor.id, &first.token_out, config)
                    .contains(first)
                {
                    continue;
                }
                if first.token_out == anchor.id
                    || !self.allowed_intermediate(&first.token_out, config)
                {
                    continue;
                }
                for second in self.limited_edges(graph, &first.token_out, &anchor.id, config) {
                    if first.pool_id != second.pool_id {
                        routes.push(self.make_route(&anchor.id, &[first.clone(), second.clone()])?);
                    }
                }
            }
        }
        Ok(routes)
    }

    fn generate_three_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>> {
        if !config.enable_three_hop || config.max_route_hops < 3 {
            return Ok(Vec::new());
        }
        let mut routes = Vec::new();
        for anchor in self.anchors(config) {
            for first in graph.outgoing(&anchor.id) {
                if !self
                    .limited_edges(graph, &anchor.id, &first.token_out, config)
                    .contains(first)
                {
                    continue;
                }
                let token_b = &first.token_out;
                if token_b == &anchor.id || !self.allowed_intermediate(token_b, config) {
                    continue;
                }
                for second in graph.outgoing(token_b) {
                    if !self
                        .limited_edges(graph, token_b, &second.token_out, config)
                        .contains(second)
                    {
                        continue;
                    }
                    let token_c = &second.token_out;
                    if token_c == &anchor.id
                        || token_c == token_b
                        || second.pool_id == first.pool_id
                        || !self.allowed_intermediate(token_c, config)
                    {
                        continue;
                    }
                    for third in self.limited_edges(graph, token_c, &anchor.id, config) {
                        if third.pool_id == first.pool_id || third.pool_id == second.pool_id {
                            continue;
                        }
                        routes.push(self.make_route(
                            &anchor.id,
                            &[first.clone(), second.clone(), third.clone()],
                        )?);
                    }
                }
            }
        }
        Ok(routes)
    }

    fn generate_four_hop_routes(
        &self,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Vec<ArbitrageRoute>> {
        if !config.enable_four_hop || config.max_route_hops < 4 {
            return Ok(Vec::new());
        }
        let mut routes = Vec::new();
        for anchor in self.anchors(config) {
            for first in graph.outgoing(&anchor.id) {
                if !self
                    .limited_edges(graph, &anchor.id, &first.token_out, config)
                    .contains(first)
                {
                    continue;
                }
                let token_b = &first.token_out;
                if token_b == &anchor.id || !self.allowed_intermediate(token_b, config) {
                    continue;
                }
                for second in graph.outgoing(token_b) {
                    if !self
                        .limited_edges(graph, token_b, &second.token_out, config)
                        .contains(second)
                    {
                        continue;
                    }
                    let token_c = &second.token_out;
                    if token_c == &anchor.id
                        || token_c == token_b
                        || second.pool_id == first.pool_id
                        || !self.allowed_intermediate(token_c, config)
                    {
                        continue;
                    }
                    for third in graph.outgoing(token_c) {
                        if !self
                            .limited_edges(graph, token_c, &third.token_out, config)
                            .contains(third)
                        {
                            continue;
                        }
                        let token_d = &third.token_out;
                        if token_d == &anchor.id
                            || token_d == token_b
                            || token_d == token_c
                            || third.pool_id == first.pool_id
                            || third.pool_id == second.pool_id
                            || !self.allowed_intermediate(token_d, config)
                        {
                            continue;
                        }
                        for fourth in self.limited_edges(graph, token_d, &anchor.id, config) {
                            if fourth.pool_id == first.pool_id
                                || fourth.pool_id == second.pool_id
                                || fourth.pool_id == third.pool_id
                            {
                                continue;
                            }
                            routes.push(self.make_route(
                                &anchor.id,
                                &[first.clone(), second.clone(), third.clone(), fourth.clone()],
                            )?);
                            if routes.len() >= config.max_routes_total {
                                return Ok(routes);
                            }
                        }
                    }
                }
            }
        }
        Ok(routes)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RouteIndex {
    pub routes: HashMap<RouteId, ArbitrageRoute>,
    pub routes_by_pool: HashMap<PoolId, Vec<RouteId>>,
    pub routes_by_token: HashMap<TokenId, Vec<RouteId>>,
    pub routes_by_anchor: HashMap<TokenId, Vec<RouteId>>,
    pub generation_stats: RouteGenerationStats,
}

impl RouteIndex {
    pub fn build(
        registry: &PoolRegistry,
        graph: &TokenPoolGraph,
        config: &RouteGenerationConfig,
    ) -> DexV2Result<Self> {
        if !(2..=4).contains(&config.max_route_hops) {
            return Err(DexV2Error::Configuration(
                "max_route_hops must be between 2 and 4".into(),
            ));
        }
        let generator = BoundedRouteGenerator::new(registry);
        let mut candidates = generator.generate_two_hop_routes(graph, config)?;
        candidates.extend(generator.generate_three_hop_routes(graph, config)?);
        candidates.extend(generator.generate_four_hop_routes(graph, config)?);
        candidates.sort_by_key(|route| (route.anchor_token.address, route.id.clone()));

        let mut index = Self::default();
        index.generation_stats.pruned_pair_edge_limit = graph
            .edges
            .values()
            .map(|edges| edges.len().saturating_sub(config.max_edges_per_token_pair))
            .sum();
        let mut per_anchor = HashMap::<TokenId, usize>::new();
        for route in candidates {
            if index.routes.contains_key(&route.id) {
                index.generation_stats.pruned_duplicate += 1;
                continue;
            }
            if index.routes.len() >= config.max_routes_total {
                index.generation_stats.pruned_total_limit += 1;
                continue;
            }
            let anchor_count = per_anchor.entry(route.anchor_token.clone()).or_default();
            if *anchor_count >= config.max_routes_per_anchor {
                index.generation_stats.pruned_anchor_limit += 1;
                continue;
            }
            *anchor_count += 1;
            match route.legs.len() {
                2 => index.generation_stats.generated_two_hop += 1,
                3 => index.generation_stats.generated_three_hop += 1,
                4 => index.generation_stats.generated_four_hop += 1,
                _ => return Err(DexV2Error::Route("unsupported route length".into())),
            }
            let id = route.id.clone();
            for pool in &route.involved_pools {
                index
                    .routes_by_pool
                    .entry(pool.clone())
                    .or_default()
                    .push(id.clone());
            }
            for token in &route.involved_tokens {
                index
                    .routes_by_token
                    .entry(token.clone())
                    .or_default()
                    .push(id.clone());
            }
            index
                .routes_by_anchor
                .entry(route.anchor_token.clone())
                .or_default()
                .push(id.clone());
            index.routes.insert(id, route);
        }
        if index.generation_stats.pruned_anchor_limit > 0
            || index.generation_stats.pruned_total_limit > 0
        {
            tracing::warn!(
                pruned_anchor_limit = index.generation_stats.pruned_anchor_limit,
                pruned_total_limit = index.generation_stats.pruned_total_limit,
                "DEX route generation reached configured limits"
            );
        }
        Ok(index)
    }

    pub fn affected_routes(&self, pool_id: &PoolId) -> &[RouteId] {
        self.routes_by_pool
            .get(pool_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

pub fn stable_route_id(chain_id: u64, anchor: &TokenId, legs: &[SwapLeg]) -> RouteId {
    let mut bytes = Vec::with_capacity(8 + 20 + legs.len() * 60);
    bytes.extend_from_slice(&chain_id.to_be_bytes());
    bytes.extend_from_slice(anchor.address.as_slice());
    for leg in legs {
        bytes.extend_from_slice(leg.pool_id.address.as_slice());
        bytes.extend_from_slice(leg.token_in.address.as_slice());
        bytes.extend_from_slice(leg.token_out.address.as_slice());
    }
    RouteId(keccak256(bytes))
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Address;

    use super::super::types::Protocol;
    use super::*;

    fn token(last: u8, anchor: bool) -> TokenMeta {
        TokenMeta {
            id: TokenId {
                chain_id: 1,
                address: Address::with_last_byte(last),
            },
            symbol: format!("T{last}"),
            decimals: 18,
            anchor,
        }
    }
    fn pool(last: u8, a: &TokenId, b: &TokenId) -> V2Pool {
        V2Pool {
            id: PoolId {
                chain_id: 1,
                address: Address::with_last_byte(last),
            },
            name: format!("P{last}"),
            protocol: Protocol::UniswapV2Compatible {
                factory: Address::ZERO,
                router: None,
            },
            token0: a.clone(),
            token1: b.clone(),
            fee_numerator: 997,
            fee_denominator: 1000,
        }
    }
    fn config(tokens: &[TokenMeta]) -> RouteGenerationConfig {
        RouteGenerationConfig {
            enable_two_hop: true,
            enable_three_hop: true,
            enable_four_hop: false,
            max_route_hops: 3,
            max_routes_total: 100,
            max_routes_per_anchor: 100,
            max_edges_per_token_pair: 10,
            allowed_anchor_tokens: tokens
                .iter()
                .filter(|t| t.anchor)
                .map(|t| t.id.clone())
                .collect(),
            allowed_intermediate_tokens: tokens.iter().map(|t| t.id.clone()).collect(),
        }
    }

    #[test]
    fn keeps_two_pool_cycles_and_builds_triangles() {
        let a = token(1, true);
        let b = token(2, false);
        let c = token(3, false);
        let tokens = vec![a.clone(), b.clone(), c.clone()];
        let registry = PoolRegistry::new(
            tokens.clone(),
            vec![
                pool(10, &a.id, &b.id),
                pool(11, &a.id, &b.id),
                pool(12, &b.id, &c.id),
                pool(13, &c.id, &a.id),
            ],
        )
        .unwrap();
        let graph = TokenPoolGraph::from_registry(&registry);
        let index = RouteIndex::build(&registry, &graph, &config(&tokens)).unwrap();
        assert_eq!(index.generation_stats.generated_two_hop, 2);
        assert_eq!(index.generation_stats.generated_three_hop, 4);
        assert!(index.routes.values().all(|route| {
            route.involved_pools.iter().collect::<HashSet<_>>().len() == route.hop_count()
        }));
        let triangle = index
            .routes
            .values()
            .find(|route| route.hop_count() == 3)
            .unwrap();
        assert!(
            triangle
                .involved_pools
                .iter()
                .all(|pool| index.affected_routes(pool).contains(&triangle.id))
        );
        let duplicate_id = stable_route_id(1, &triangle.anchor_token, &triangle.legs);
        assert_eq!(duplicate_id, triangle.id);
    }

    #[test]
    fn enforces_route_limits_without_silent_drop() {
        let a = token(1, true);
        let b = token(2, false);
        let c = token(3, false);
        let tokens = vec![a.clone(), b.clone(), c.clone()];
        let registry = PoolRegistry::new(
            tokens.clone(),
            vec![
                pool(10, &a.id, &b.id),
                pool(11, &b.id, &c.id),
                pool(12, &c.id, &a.id),
            ],
        )
        .unwrap();
        let mut limits = config(&tokens);
        limits.max_routes_total = 1;
        let index = RouteIndex::build(
            &registry,
            &TokenPoolGraph::from_registry(&registry),
            &limits,
        )
        .unwrap();
        assert_eq!(index.routes.len(), 1);
        assert!(index.generation_stats.pruned_total_limit > 0);
    }
}
