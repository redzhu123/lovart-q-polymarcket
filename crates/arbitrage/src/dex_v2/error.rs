use thiserror::Error;

pub type DexV2Result<T> = Result<T, DexV2Error>;

#[derive(Debug, Error)]
pub enum DexV2Error {
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("route error: {0}")]
    Route(String),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("pool state error: {0}")]
    PoolState(String),
    #[error("quote error: {0}")]
    Quote(String),
    #[error("optimization error: {0}")]
    Optimization(String),
    #[error("simulation error: {0}")]
    Simulation(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("repository error: {0}")]
    Repository(String),
    #[error("risk rejected opportunity: {0}")]
    Risk(String),
}
