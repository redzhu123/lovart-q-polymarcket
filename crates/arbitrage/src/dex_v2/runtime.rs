use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, Bytes, U256, keccak256};
use futures_util::{StreamExt, TryStreamExt, stream};
use tokio::sync::{mpsc, watch};

use super::adapter::{PoolAdapter, UniswapV2Adapter};
use super::config::DexV2Config;
use super::connector::ChainConnector;
use super::error::{DexV2Error, DexV2Result};
use super::execution::ExecutionRequestBuilder;
use super::gas::{FixedGasPriceOracle, GasEstimator, GasPriceOracle, HopGasEstimator};
use super::graph::{PoolRegistry, RouteIndex, TokenPoolGraph};
use super::metrics::DexV2Metrics;
use super::optimizer::{AmountOptimizer, IntegerSearchOptimizer, has_marginal_edge};
use super::profit::{FixedNativePriceOracle, NativePriceOracle, ProfitEngine, V2ProfitEngine};
use super::quoter::{LocalRouteQuoter, RouteQuoter};
use super::repository::{InMemoryOpportunityRepository, OpportunityRepository};
use super::risk::{DefaultRiskGuard, RiskGuard};
use super::simulator::{LocalShadowSimulator, SimulationEngine};
use super::state::PoolStateCache;
use super::types::{
    ArbitrageOpportunity, ArbitrageQuote, CostEstimate, OpportunityStatus, PoolId, PoolUpdate,
    RouteId, SimulationRequest, StateVersion,
};

pub struct DexV2Engine {
    config: DexV2Config,
    pub registry: Arc<PoolRegistry>,
    pub routes: Arc<RouteIndex>,
    pub state: Arc<PoolStateCache>,
    optimizer: Arc<IntegerSearchOptimizer>,
    profit_engine: Arc<dyn ProfitEngine>,
    oracle: Arc<dyn NativePriceOracle>,
    risk: Arc<dyn RiskGuard>,
    gas_estimator: Arc<dyn GasEstimator>,
    gas_price_oracle: Arc<dyn GasPriceOracle>,
    execution_builder: ExecutionRequestBuilder,
    simulator: Arc<dyn SimulationEngine>,
    repository: Arc<dyn OpportunityRepository>,
    pub metrics: Arc<DexV2Metrics>,
    latest_checks: Mutex<HashMap<(RouteId, u64), u64>>,
    last_synced_block: Mutex<Option<u64>>,
    last_full_resync_block: Mutex<Option<u64>>,
    log_polling_unavailable: AtomicBool,
}

impl DexV2Engine {
    pub fn from_config(config: DexV2Config) -> DexV2Result<Self> {
        config.validate()?;
        let route_config = config.route_generation_config()?;
        let registry = Arc::new(PoolRegistry::new(config.token_meta()?, config.v2_pools()?)?);
        let graph = TokenPoolGraph::from_registry(&registry);
        let routes = Arc::new(RouteIndex::build(&registry, &graph, &route_config)?);
        let adapter: Arc<dyn PoolAdapter> = Arc::new(UniswapV2Adapter::new());
        let quoter: Arc<dyn RouteQuoter> =
            Arc::new(LocalRouteQuoter::new(Arc::clone(&registry), adapter));
        let optimizer = Arc::new(IntegerSearchOptimizer::new(Arc::clone(&registry), quoter));
        let oracle: Arc<dyn NativePriceOracle> =
            Arc::new(FixedNativePriceOracle::new(config.native_price_anchor()?));
        let risk: Arc<dyn RiskGuard> = Arc::new(DefaultRiskGuard::new(
            Arc::clone(&registry),
            route_config.allowed_intermediate_tokens,
            config.min_net_profit()?,
            config.min_three_hop_net_profit()?,
            config.min_gross_profit()?,
            config.max_gas_anchor()?,
            config.min_roi_bps,
            config
                .risk
                .min_three_hop_roi_bps
                .unwrap_or(config.min_roi_bps),
            config.max_state_block_gap,
            config.max_opportunity_age_blocks,
            config.minimum_pool_liquidity()?,
            config.risk.max_leg_price_impact_bps,
            config.risk.max_total_price_impact_bps,
            config.optimizer.max_quote_evaluations,
        ));
        let gas_estimator: Arc<dyn GasEstimator> = Arc::new(HopGasEstimator {
            two_hop_fallback_gas: config.gas.two_hop_fallback_gas,
            three_hop_fallback_gas: config.gas.three_hop_fallback_gas,
            four_hop_fallback_gas: config.gas.four_hop_fallback_gas,
            gas_units_buffer_bps: config.gas.gas_units_buffer_bps,
        });
        let gas_price_oracle: Arc<dyn GasPriceOracle> =
            Arc::new(FixedGasPriceOracle::new(config.max_fee_per_gas()?));
        let execution_builder = ExecutionRequestBuilder {
            default_leg_slippage_bps: config.execution.default_leg_slippage_bps,
            three_hop_leg_slippage_bps: config.execution.three_hop_leg_slippage_bps,
            max_steps: config.execution.max_steps,
        };
        Ok(Self {
            config,
            registry,
            routes,
            state: Arc::new(PoolStateCache::new()),
            optimizer,
            profit_engine: Arc::new(V2ProfitEngine),
            oracle,
            risk,
            gas_estimator,
            gas_price_oracle,
            execution_builder,
            simulator: Arc::new(LocalShadowSimulator),
            repository: Arc::new(InMemoryOpportunityRepository::new()),
            metrics: Arc::new(DexV2Metrics::default()),
            latest_checks: Mutex::new(HashMap::new()),
            last_synced_block: Mutex::new(None),
            last_full_resync_block: Mutex::new(None),
            log_polling_unavailable: AtomicBool::new(false),
        })
    }

    pub fn with_repository(mut self, repository: Arc<dyn OpportunityRepository>) -> Self {
        self.repository = repository;
        self
    }

    pub fn with_simulator(mut self, simulator: Arc<dyn SimulationEngine>) -> Self {
        self.simulator = simulator;
        self
    }

    pub fn with_native_price_oracle(mut self, oracle: Arc<dyn NativePriceOracle>) -> Self {
        self.oracle = oracle;
        self
    }

    pub fn with_gas_price_oracle(mut self, oracle: Arc<dyn GasPriceOracle>) -> Self {
        self.gas_price_oracle = oracle;
        self
    }

    pub async fn cost_snapshot(
        &self,
        anchor: &super::types::TokenId,
        block: u64,
    ) -> DexV2Result<CostDataSnapshot> {
        let gas_price_wei = self.gas_price_oracle.gas_price(block).await?;
        let native_price_anchor = self
            .oracle
            .quote_native_to_token(anchor, U256::from(10u64).pow(U256::from(18)), block)
            .await?;
        Ok(CostDataSnapshot {
            block_number: block,
            gas_price_wei,
            native_price_anchor,
        })
    }

    pub async fn initialize(&self, connector: &dyn ChainConnector) -> DexV2Result<u64> {
        let block = self.stable_sync_block(connector).await?;
        for pool in self.registry.pools() {
            let state = connector.get_reserves(pool, block).await?;
            self.state.apply(PoolUpdate {
                pool_id: pool.id.clone(),
                state,
                log_index: 0,
            })?;
        }
        *self
            .last_synced_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("sync block lock poisoned".into()))? = Some(block);
        *self
            .last_full_resync_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("resync block lock poisoned".into()))? = Some(block);
        tracing::info!(
            chain_id = self.config.chain_id,
            block_number = block,
            pools = self.registry.pools().count(),
            routes = self.routes.routes.len(),
            two_hop_routes = self.routes.generation_stats.generated_two_hop,
            three_hop_routes = self.routes.generation_stats.generated_three_hop,
            four_hop_routes = self.routes.generation_stats.generated_four_hop,
            execution_mode = ?self.config.execution_mode,
            "DEX V2 cyclic engine initialized"
        );
        Ok(block)
    }

    pub async fn sync_once(
        &self,
        connector: &dyn ChainConnector,
    ) -> DexV2Result<Vec<ArbitrageOpportunity>> {
        let latest = self.stable_sync_block(connector).await?;
        let from = self
            .last_synced_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("sync block lock poisoned".into()))?
            .unwrap_or(latest)
            .saturating_add(1);
        if from > latest {
            return Ok(Vec::new());
        }
        let pools = self.registry.pools().cloned().collect::<Vec<_>>();
        let mut reserve_fallback = self.log_polling_unavailable.load(Ordering::Relaxed);
        let updates = if reserve_fallback {
            self.load_all_pool_updates(connector, &pools, latest)
                .await?
        } else {
            match connector.poll_pool_updates(&pools, from, latest).await {
                Ok(updates) => updates,
                Err(error) if is_log_access_restriction(&error) => {
                    self.log_polling_unavailable.store(true, Ordering::Relaxed);
                    reserve_fallback = true;
                    tracing::warn!(
                        error = %error,
                        "RPC 限制 eth_getLogs，已降级为按新区块刷新全部池储备"
                    );
                    self.load_all_pool_updates(connector, &pools, latest)
                        .await?
                }
                Err(error) => return Err(error),
            }
        };
        let opportunities = if reserve_fallback {
            self.process_full_snapshot(updates, latest).await?
        } else {
            let mut opportunities = Vec::new();
            for update in updates {
                opportunities.extend(self.process_pool_update(update).await?);
            }
            opportunities
        };
        let should_resync = self
            .last_full_resync_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("resync block lock poisoned".into()))?
            .map(|block| latest.saturating_sub(block) >= self.config.resync_interval_blocks)
            .unwrap_or(true);
        if should_resync && !reserve_fallback {
            for pool in &pools {
                let state = connector.get_reserves(pool, latest).await?;
                let _ = self.state.apply(PoolUpdate {
                    pool_id: pool.id.clone(),
                    state,
                    log_index: 0,
                })?;
            }
            *self
                .last_full_resync_block
                .lock()
                .map_err(|_| DexV2Error::PoolState("resync block lock poisoned".into()))? =
                Some(latest);
        }
        *self
            .last_synced_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("sync block lock poisoned".into()))? = Some(latest);
        Ok(opportunities)
    }

    async fn load_all_pool_updates(
        &self,
        connector: &dyn ChainConnector,
        pools: &[super::types::V2Pool],
        block: u64,
    ) -> DexV2Result<Vec<PoolUpdate>> {
        let concurrency = self.config.worker_count.clamp(1, pools.len().max(1));
        let updates = stream::iter(pools.iter().cloned().map(|pool| async move {
            Ok::<_, DexV2Error>(PoolUpdate {
                pool_id: pool.id.clone(),
                state: connector.get_reserves(&pool, block).await?,
                log_index: 0,
            })
        }))
        .buffer_unordered(concurrency)
        .try_collect::<Vec<_>>()
        .await?;
        *self
            .last_full_resync_block
            .lock()
            .map_err(|_| DexV2Error::PoolState("resync block lock poisoned".into()))? = Some(block);
        Ok(updates)
    }

    async fn process_full_snapshot(
        &self,
        updates: Vec<PoolUpdate>,
        block: u64,
    ) -> DexV2Result<Vec<ArbitrageOpportunity>> {
        let mut affected = HashMap::<RouteId, PoolId>::new();
        for update in updates {
            if !self.state.apply(update.clone())? {
                continue;
            }
            DexV2Metrics::increment(&self.metrics.pool_updates_total);
            for route_id in self.routes.affected_routes(&update.pool_id) {
                affected
                    .entry(route_id.clone())
                    .or_insert_with(|| update.pool_id.clone());
            }
        }

        let mut opportunities = Vec::new();
        for (route_id, trigger_pool) in affected {
            if let Some(opportunity) = self
                .scan_route(
                    &route_id,
                    &trigger_pool,
                    StateVersion {
                        block_number: block,
                        max_log_index: 0,
                    },
                )
                .await?
            {
                opportunities.push(opportunity);
            }
        }
        Ok(opportunities)
    }

    async fn stable_sync_block(&self, connector: &dyn ChainConnector) -> DexV2Result<u64> {
        let head = connector.head_block(self.config.confirmation_mode).await?;
        Ok(head.saturating_sub(self.config.log_query_delay_blocks))
    }

    pub async fn process_pool_update(
        &self,
        update: PoolUpdate,
    ) -> DexV2Result<Vec<ArbitrageOpportunity>> {
        if !self.state.apply(update.clone())? {
            DexV2Metrics::increment(&self.metrics.route_checks_deduplicated_total);
            return Ok(Vec::new());
        }
        DexV2Metrics::increment(&self.metrics.pool_updates_total);
        let mut opportunities = Vec::new();
        for route_id in self.routes.affected_routes(&update.pool_id) {
            let version = StateVersion {
                block_number: update.state.block_number,
                max_log_index: update.log_index,
            };
            if let Some(opportunity) = self.scan_route(route_id, &update.pool_id, version).await? {
                opportunities.push(opportunity);
            }
        }
        Ok(opportunities)
    }

    async fn scan_route(
        &self,
        route_id: &RouteId,
        trigger_pool: &PoolId,
        trigger_version: StateVersion,
    ) -> DexV2Result<Option<ArbitrageOpportunity>> {
        DexV2Metrics::increment(&self.metrics.route_checks_total);
        {
            let mut checks = self
                .latest_checks
                .lock()
                .map_err(|_| DexV2Error::PoolState("dedupe lock poisoned".into()))?;
            let key = (route_id.clone(), trigger_version.block_number);
            if checks
                .get(&key)
                .is_some_and(|index| *index >= trigger_version.max_log_index)
            {
                DexV2Metrics::increment(&self.metrics.route_checks_deduplicated_total);
                return Ok(None);
            }
            checks.insert(key, trigger_version.max_log_index);
            checks.retain(|(_, block), _| *block >= trigger_version.block_number.saturating_sub(2));
        }
        let route = self
            .routes
            .routes
            .get(route_id)
            .ok_or_else(|| DexV2Error::PoolState("route index is inconsistent".into()))?;
        let snapshot = self.state.snapshot(
            self.config.chain_id,
            trigger_version.block_number,
            &route.involved_pools,
            self.config.max_state_block_gap,
        )?;
        self.risk
            .validate_route(route, &snapshot)
            .map_err(DexV2Error::from)?;
        if !has_marginal_edge(
            &self.registry,
            route,
            &snapshot,
            self.config.optimizer.minimum_theoretical_edge_bps,
        )? {
            return Ok(None);
        }
        DexV2Metrics::increment(&self.metrics.marginal_filter_pass_total);
        let bounds = self.config.amount_bounds()?;
        if !self
            .optimizer
            .has_profitable_seed(route, &snapshot, &bounds)?
        {
            return Ok(None);
        }
        DexV2Metrics::increment(&self.metrics.seed_quote_filter_pass_total);
        let optimized = match self.optimizer.optimize(route, &snapshot, &bounds) {
            Ok(value) => value,
            Err(DexV2Error::Optimization(message)) if message.contains("no profitable input") => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        self.metrics.optimization_quote_evaluations.fetch_add(
            optimized.tested_amounts as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        DexV2Metrics::increment(&self.metrics.theoretical_opportunities_total);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DexV2Error::Execution("system clock before epoch".into()))?
            .as_secs();
        let min_profit = match route.kind {
            super::types::RouteKind::TwoHop => self.config.min_net_profit()?,
            super::types::RouteKind::ThreeHop => self
                .config
                .min_three_hop_net_profit()?
                .max(self.config.min_net_profit()?),
            super::types::RouteKind::FourHop => self
                .config
                .min_three_hop_net_profit()?
                .max(self.config.min_net_profit()?),
        };
        let execution = self.execution_builder.build(
            route,
            &optimized.route_quote,
            min_profit,
            now.saturating_add(self.config.execution.deadline_seconds),
        )?;
        let call = if let Some(executor) = self.config.executor_address()? {
            Some(self.execution_builder.encode(executor, &execution)?)
        } else {
            None
        };
        if self.config.execution_mode == super::types::ExecutionMode::SimulateOnly && call.is_none()
        {
            return Err(DexV2Error::Configuration(
                "simulate_only requires execution.executor_address".into(),
            ));
        }
        let preliminary_gas = self
            .gas_estimator
            .estimate_route(route, &execution, None)
            .await?;
        let max_fee = self
            .gas_price_oracle
            .gas_price(trigger_version.block_number)
            .await?;
        let preliminary_native = U256::from(preliminary_gas.gas_units)
            .checked_mul(max_fee)
            .ok_or_else(|| DexV2Error::Quote("gas cost overflow".into()))?;
        let preliminary_anchor = self
            .oracle
            .quote_native_to_token(
                &route.anchor_token,
                preliminary_native,
                trigger_version.block_number,
            )
            .await?;
        let preliminary_profit = self.profit_engine.calculate(
            optimized.route_quote.amount_in,
            optimized.route_quote.amount_out,
            &CostEstimate {
                gas_units: preliminary_gas.gas_units,
                max_fee_per_gas: max_fee,
                priority_fee_per_gas: U256::ZERO,
                native_token_cost: preliminary_native,
                anchor_token_cost: preliminary_anchor,
                builder_tip_anchor: U256::ZERO,
                risk_buffer_anchor: U256::ZERO,
            },
        )?;
        let simulation_request = SimulationRequest {
            from: Address::ZERO,
            to: call.as_ref().map_or(Address::ZERO, |call| call.to),
            data: call
                .as_ref()
                .map_or_else(Bytes::new, |call| call.data.clone()),
            value: U256::ZERO,
            gas: preliminary_gas.gas_units,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: U256::ZERO,
            block_number: trigger_version.block_number,
            expected_amount_out: optimized.route_quote.amount_out,
            expected_profit: preliminary_profit.net_profit,
            execution: Some(execution.clone()),
            leg_quotes: optimized.route_quote.leg_quotes.clone(),
        };
        DexV2Metrics::increment(&self.metrics.simulation_total);
        let simulation = match self.simulator.simulate(&simulation_request).await {
            Ok(result) if result.success => result,
            Ok(_) | Err(_) => {
                DexV2Metrics::increment(&self.metrics.simulation_failures_total);
                return Ok(None);
            }
        };
        let gas = self
            .gas_estimator
            .estimate_route(route, &execution, Some(&simulation))
            .await?;
        let gas_native = U256::from(gas.gas_units)
            .checked_mul(max_fee)
            .ok_or_else(|| DexV2Error::Quote("gas cost overflow".into()))?;
        let gas_anchor = self
            .oracle
            .quote_native_to_token(
                &route.anchor_token,
                gas_native,
                trigger_version.block_number,
            )
            .await?;
        let gross_profit = optimized
            .route_quote
            .amount_out
            .saturating_sub(optimized.route_quote.amount_in);
        let risk_buffer = gross_profit
            .checked_mul(U256::from(self.config.risk_buffer_bps))
            .ok_or_else(|| DexV2Error::Quote("risk buffer overflow".into()))?
            / U256::from(10_000);
        let profit = self.profit_engine.calculate(
            optimized.route_quote.amount_in,
            optimized.route_quote.amount_out,
            &CostEstimate {
                gas_units: gas.gas_units,
                max_fee_per_gas: max_fee,
                priority_fee_per_gas: U256::ZERO,
                native_token_cost: gas_native,
                anchor_token_cost: gas_anchor,
                builder_tip_anchor: U256::ZERO,
                risk_buffer_anchor: risk_buffer,
            },
        )?;
        let quote = ArbitrageQuote {
            route_id: route.id.clone(),
            block_number: trigger_version.block_number,
            amount_in: optimized.route_quote.amount_in,
            amount_out: optimized.route_quote.amount_out,
            gross_profit: profit.gross_profit,
            estimated_gas_units: gas.gas_units,
            estimated_gas_native: gas_native,
            estimated_gas_anchor: gas_anchor,
            risk_buffer,
            net_profit: profit.net_profit,
            roi_bps: profit.roi_bps,
            leg_quotes: optimized.route_quote.leg_quotes.clone(),
            price_impacts: optimized.route_quote.price_impacts.clone(),
            optimization_method: optimized.method,
            quote_evaluations: optimized.tested_amounts,
            state_min_block: snapshot.min_state_block,
            state_max_block: snapshot.max_state_block,
        };
        if let Err(reason) = self.risk.validate_quote(route, &quote) {
            DexV2Metrics::increment(&self.metrics.opportunities_rejected_total);
            tracing::debug!(route_id = %route.id.0, hop_count = route.hop_count(), rejection_reason = ?reason, "cyclic opportunity rejected");
            return Ok(None);
        }
        let current_version = self.state.version(&route.involved_pools)?;
        if current_version.is_none_or(|version| version > snapshot.state_version) {
            tracing::debug!(route_id = %route.id.0, hop_count = route.hop_count(), "discarding stale route result");
            return Ok(None);
        }
        DexV2Metrics::increment(&self.metrics.profitable_quotes_total);
        let id = opportunity_id(
            self.config.chain_id,
            &route.id,
            trigger_version.block_number,
            quote.amount_in,
            bounds.minimum_search_step,
        );
        let opportunity = ArbitrageOpportunity {
            id,
            chain_id: self.config.chain_id,
            route: route.clone(),
            trigger_pool: trigger_pool.clone(),
            quote,
            simulation_result: simulation,
            status: OpportunityStatus::Simulated,
            created_at: SystemTime::now(),
        };
        self.repository.save_opportunity(&opportunity).await?;
        tracing::info!(
            chain_id = self.config.chain_id, block_number = trigger_version.block_number,
            route_id = %route.id.0, route_kind = ?route.kind, hop_count = route.hop_count(),
            trigger_pool = %trigger_pool.address, amount_in = %opportunity.quote.amount_in,
            amount_out = %opportunity.quote.amount_out, gross_profit = %opportunity.quote.gross_profit,
            net_profit = %opportunity.quote.net_profit, gas_units = opportunity.quote.estimated_gas_units,
            roi_bps = %opportunity.quote.roi_bps, quote_evaluations = opportunity.quote.quote_evaluations,
            execution_mode = ?self.config.execution_mode, "DEX V2 cyclic shadow opportunity"
        );
        Ok(Some(opportunity))
    }

    pub fn start(self: Arc<Self>) -> RuntimeHandle {
        let (sender, receiver) = mpsc::channel(self.config.queue_capacity);
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut joins = Vec::new();
        for _ in 0..self.config.worker_count {
            let engine = Arc::clone(&self);
            let receiver = Arc::clone(&receiver);
            let mut shutdown = shutdown_rx.clone();
            joins.push(tokio::spawn(async move {
                loop {
                    tokio::select! {
                        changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
                        update = async { receiver.lock().await.recv().await } => {
                            match update {
                                Some(update) => { if let Err(error) = engine.process_pool_update(update).await { tracing::warn!(error = %error, "DEX V2 worker rejected update"); } }
                                None => break,
                            }
                        }
                    }
                }
            }));
        }
        RuntimeHandle {
            sender,
            shutdown: shutdown_tx,
            joins,
        }
    }
}

fn is_log_access_restriction(error: &DexV2Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("eth_getlogs")
        && (message.contains("http 403")
            || message.contains("archive requests")
            || message.contains("personal token")
            || message.contains("invalid block range"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostDataSnapshot {
    pub block_number: u64,
    pub gas_price_wei: U256,
    pub native_price_anchor: U256,
}

pub struct RuntimeHandle {
    sender: mpsc::Sender<PoolUpdate>,
    shutdown: watch::Sender<bool>,
    joins: Vec<tokio::task::JoinHandle<()>>,
}

impl RuntimeHandle {
    pub async fn submit(&self, update: PoolUpdate) -> DexV2Result<()> {
        self.sender
            .send(update)
            .await
            .map_err(|_| DexV2Error::Execution("runtime queue closed".into()))
    }
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        for join in self.joins {
            let _ = join.await;
        }
    }
}

fn opportunity_id(
    chain_id: u64,
    route: &RouteId,
    block: u64,
    amount: U256,
    step: U256,
) -> alloy_primitives::B256 {
    let bucket = if step.is_zero() {
        amount
    } else {
        amount / step
    };
    let mut bytes = Vec::with_capacity(80);
    bytes.extend_from_slice(&chain_id.to_be_bytes());
    bytes.extend_from_slice(route.0.as_slice());
    bytes.extend_from_slice(&block.to_be_bytes());
    bytes.extend_from_slice(&bucket.to_be_bytes::<32>());
    keccak256(bytes)
}
