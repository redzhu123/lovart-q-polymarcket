use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tokio::task::JoinSet;

use crate::dex_v2::{
    ChainConnector, DexV2Config, DexV2Engine, DexV2Error, DexV2Result, EthCallSimulator,
    ExecutionMode, JsonRpcConnector, RpcGasPriceOracle, V2PoolNativePriceOracle,
    discover_configured_v2_pools,
};

#[derive(Debug, Clone, Deserialize)]
pub struct MultiChainConfig {
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub cross_chain_config_path: Option<PathBuf>,
    #[serde(default)]
    pub chains: Vec<ChainConfigRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfigRef {
    pub name: String,
    pub config_path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_poll_interval() -> u64 {
    500
}

fn default_true() -> bool {
    true
}

impl MultiChainConfig {
    pub fn load(path: impl AsRef<Path>) -> DexV2Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            DexV2Error::Configuration(format!("read {}: {error}", path.display()))
        })?;
        let mut config: Self = toml::from_str(&text).map_err(|error| {
            DexV2Error::Configuration(format!("parse {}: {error}", path.display()))
        })?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        for chain in &mut config.chains {
            if chain.config_path.is_relative() {
                chain.config_path = base.join(&chain.config_path);
            }
        }
        if let Some(cross_chain) = &mut config.cross_chain_config_path {
            if cross_chain.is_relative() {
                *cross_chain = base.join(&*cross_chain);
            }
        }
        if config.poll_interval_ms == 0
            || !config.chains.iter().any(|chain| chain.enabled)
            || config
                .chains
                .iter()
                .filter(|chain| chain.enabled)
                .any(|chain| chain.name.trim().is_empty())
        {
            return Err(DexV2Error::Configuration(
                "多链配置必须包含至少一条启用链".into(),
            ));
        }
        Ok(config)
    }
}

pub struct ChainShadowRuntime {
    pub name: String,
    pub chain_id: u64,
    pub engine: Arc<DexV2Engine>,
    pub connector: Arc<dyn ChainConnector>,
    pub discovered_pools: usize,
}

impl ChainShadowRuntime {
    pub async fn build(name: impl Into<String>, path: impl AsRef<Path>) -> DexV2Result<Self> {
        let name = name.into();
        let mut config = DexV2Config::load(path)?;
        if !config.enabled {
            return Err(DexV2Error::Configuration(format!(
                "链 {name} 的 DEX 配置未启用"
            )));
        }
        let chain_id = config.chain_id;
        let realtime_gas = config.market_data.use_realtime_gas_price;
        let gas_buffer = config.market_data.gas_price_buffer_bps;
        let native_price_pool = config.native_price_pool()?;
        let mode = config.execution_mode;
        let connector = Arc::new(JsonRpcConnector::new(config.rpc_http_url.clone())?);
        let discovery = discover_configured_v2_pools(&mut config, connector.as_ref()).await?;
        let mut engine = DexV2Engine::from_config(config)?;
        if realtime_gas {
            engine = engine.with_gas_price_oracle(Arc::new(RpcGasPriceOracle::new(
                connector.clone(),
                gas_buffer,
            )?));
        }
        if let Some((pool, native, anchor)) = native_price_pool {
            engine = engine.with_native_price_oracle(Arc::new(V2PoolNativePriceOracle::new(
                connector.clone(),
                pool,
                native,
                anchor,
            )?));
        }
        if mode == ExecutionMode::SimulateOnly {
            engine = engine.with_simulator(Arc::new(EthCallSimulator::new(connector.clone())));
        }
        Ok(Self {
            name,
            chain_id,
            engine: Arc::new(engine),
            connector,
            discovered_pools: discovery.pools_discovered,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ChainInitializationReport {
    pub name: String,
    pub chain_id: u64,
    pub block_number: Option<u64>,
    pub pools: usize,
    pub routes: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChainSyncReport {
    pub name: String,
    pub chain_id: u64,
    pub opportunities: usize,
    pub error: Option<String>,
}

pub struct MultiChainSupervisor {
    chains: Vec<Arc<ChainShadowRuntime>>,
}

impl MultiChainSupervisor {
    pub fn new(chains: Vec<Arc<ChainShadowRuntime>>) -> DexV2Result<Self> {
        if chains.is_empty() {
            return Err(DexV2Error::Configuration(
                "多链 Supervisor 没有运行链".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for chain in &chains {
            if !ids.insert(chain.chain_id) || !names.insert(chain.name.to_ascii_lowercase()) {
                return Err(DexV2Error::Configuration(
                    "多链 Supervisor 存在重复链 ID 或名称".into(),
                ));
            }
        }
        Ok(Self { chains })
    }

    pub fn chains(&self) -> &[Arc<ChainShadowRuntime>] {
        &self.chains
    }

    pub async fn initialize_all(&self) -> Vec<ChainInitializationReport> {
        let mut joins = JoinSet::new();
        for chain in &self.chains {
            let chain = Arc::clone(chain);
            joins.spawn(async move {
                let result = chain.engine.initialize(chain.connector.as_ref()).await;
                ChainInitializationReport {
                    name: chain.name.clone(),
                    chain_id: chain.chain_id,
                    block_number: result.as_ref().ok().copied(),
                    pools: chain.engine.registry.pools().count(),
                    routes: chain.engine.routes.routes.len(),
                    error: result.err().map(|error| error.to_string()),
                }
            });
        }
        collect_join_reports(&mut joins).await
    }

    pub async fn sync_all(&self) -> Vec<ChainSyncReport> {
        let mut joins = JoinSet::new();
        for chain in &self.chains {
            let chain = Arc::clone(chain);
            joins.spawn(async move {
                let result = chain.engine.sync_once(chain.connector.as_ref()).await;
                ChainSyncReport {
                    name: chain.name.clone(),
                    chain_id: chain.chain_id,
                    opportunities: result.as_ref().map_or(0, Vec::len),
                    error: result.err().map(|error| error.to_string()),
                }
            });
        }
        collect_join_reports(&mut joins).await
    }
}

async fn collect_join_reports<T: Send + 'static>(joins: &mut JoinSet<T>) -> Vec<T> {
    let mut reports = Vec::new();
    while let Some(result) = joins.join_next().await {
        if let Ok(report) = result {
            reports.push(report);
        }
    }
    reports
}

pub async fn build_supervisor(config: &MultiChainConfig) -> DexV2Result<MultiChainSupervisor> {
    let mut chains = Vec::new();
    for chain in config.chains.iter().filter(|chain| chain.enabled) {
        chains.push(Arc::new(
            ChainShadowRuntime::build(&chain.name, &chain.config_path).await?,
        ));
    }
    MultiChainSupervisor::new(chains)
}
