use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct DexV2Metrics {
    pub pool_updates_total: AtomicU64,
    pub route_checks_total: AtomicU64,
    pub route_checks_deduplicated_total: AtomicU64,
    pub theoretical_opportunities_total: AtomicU64,
    pub marginal_filter_pass_total: AtomicU64,
    pub seed_quote_filter_pass_total: AtomicU64,
    pub optimization_quote_evaluations: AtomicU64,
    pub profitable_quotes_total: AtomicU64,
    pub simulation_total: AtomicU64,
    pub simulation_failures_total: AtomicU64,
    pub opportunities_rejected_total: AtomicU64,
    pub queue_depth: AtomicU64,
    pub rpc_errors_total: AtomicU64,
    pub ws_reconnects_total: AtomicU64,
}

impl DexV2Metrics {
    pub fn increment(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> DexV2MetricsSnapshot {
        DexV2MetricsSnapshot {
            pool_updates_total: self.pool_updates_total.load(Ordering::Relaxed),
            route_checks_total: self.route_checks_total.load(Ordering::Relaxed),
            route_checks_deduplicated_total: self
                .route_checks_deduplicated_total
                .load(Ordering::Relaxed),
            theoretical_opportunities_total: self
                .theoretical_opportunities_total
                .load(Ordering::Relaxed),
            marginal_filter_pass_total: self.marginal_filter_pass_total.load(Ordering::Relaxed),
            seed_quote_filter_pass_total: self.seed_quote_filter_pass_total.load(Ordering::Relaxed),
            optimization_quote_evaluations: self
                .optimization_quote_evaluations
                .load(Ordering::Relaxed),
            profitable_quotes_total: self.profitable_quotes_total.load(Ordering::Relaxed),
            simulation_total: self.simulation_total.load(Ordering::Relaxed),
            simulation_failures_total: self.simulation_failures_total.load(Ordering::Relaxed),
            opportunities_rejected_total: self.opportunities_rejected_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DexV2MetricsSnapshot {
    pub pool_updates_total: u64,
    pub route_checks_total: u64,
    pub route_checks_deduplicated_total: u64,
    pub theoretical_opportunities_total: u64,
    pub marginal_filter_pass_total: u64,
    pub seed_quote_filter_pass_total: u64,
    pub optimization_quote_evaluations: u64,
    pub profitable_quotes_total: u64,
    pub simulation_total: u64,
    pub simulation_failures_total: u64,
    pub opportunities_rejected_total: u64,
}
