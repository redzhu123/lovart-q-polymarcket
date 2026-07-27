use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::error::{DexV2Error, DexV2Result};
use super::types::{PoolId, PoolUpdate, StateSnapshot, StateVersion, V2PoolState};

#[derive(Debug, Clone)]
struct VersionedState {
    state: Arc<V2PoolState>,
    log_index: u64,
}

#[derive(Debug, Default)]
pub struct PoolStateCache {
    states: RwLock<HashMap<PoolId, VersionedState>>,
}

impl PoolStateCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 只应用 `(block_number, log_index)` 单调递增的更新。
    /// 被移除的日志由连接器重放处理，不能直接写入缓存。
    pub fn apply(&self, update: PoolUpdate) -> DexV2Result<bool> {
        let mut states = self
            .states
            .write()
            .map_err(|_| DexV2Error::PoolState("state cache lock poisoned".into()))?;
        if let Some(current) = states.get(&update.pool_id) {
            let current_version = (current.state.block_number, current.log_index);
            let next_version = (update.state.block_number, update.log_index);
            if next_version <= current_version {
                return Ok(false);
            }
        }
        states.insert(
            update.pool_id,
            VersionedState {
                state: Arc::new(update.state),
                log_index: update.log_index,
            },
        );
        Ok(true)
    }

    pub fn get(&self, pool_id: &PoolId) -> DexV2Result<Option<Arc<V2PoolState>>> {
        Ok(self
            .states
            .read()
            .map_err(|_| DexV2Error::PoolState("state cache lock poisoned".into()))?
            .get(pool_id)
            .map(|versioned| Arc::clone(&versioned.state)))
    }

    pub fn snapshot(
        &self,
        chain_id: u64,
        target_block: u64,
        pool_ids: &[PoolId],
        max_block_gap: u64,
    ) -> DexV2Result<StateSnapshot> {
        let states = self
            .states
            .read()
            .map_err(|_| DexV2Error::PoolState("state cache lock poisoned".into()))?;
        let mut pools = HashMap::new();
        let mut min_block = target_block;
        let mut max_block = 0;
        let mut max_log_index = 0;
        for pool_id in pool_ids {
            if pool_id.chain_id != chain_id {
                return Err(DexV2Error::PoolState("snapshot mixes chain ids".into()));
            }
            let state = states.get(pool_id).ok_or_else(|| {
                DexV2Error::PoolState(format!("missing state for pool {:?}", pool_id.address))
            })?;
            if state.state.block_number > target_block {
                return Err(DexV2Error::PoolState(
                    "snapshot would use future pool state".into(),
                ));
            }
            min_block = min_block.min(state.state.block_number);
            max_block = max_block.max(state.state.block_number);
            max_log_index = max_log_index.max(state.log_index);
            pools.insert(pool_id.clone(), Arc::clone(&state.state));
        }
        if max_block.saturating_sub(min_block) > max_block_gap {
            return Err(DexV2Error::PoolState(format!(
                "state block gap {} exceeds {}",
                max_block - min_block,
                max_block_gap
            )));
        }
        Ok(StateSnapshot {
            chain_id,
            target_block,
            min_state_block: min_block,
            max_state_block: max_block,
            state_version: StateVersion {
                block_number: max_block,
                max_log_index,
            },
            pools,
        })
    }

    pub fn version(&self, pool_ids: &[PoolId]) -> DexV2Result<Option<StateVersion>> {
        let states = self
            .states
            .read()
            .map_err(|_| DexV2Error::PoolState("state cache lock poisoned".into()))?;
        let mut version: Option<StateVersion> = None;
        for pool_id in pool_ids {
            let Some(state) = states.get(pool_id) else {
                return Ok(None);
            };
            let candidate = StateVersion {
                block_number: state.state.block_number,
                max_log_index: state.log_index,
            };
            version = Some(version.map_or(candidate, |current| current.max(candidate)));
        }
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};
    use std::time::SystemTime;

    fn update(block: u64, index: u64) -> PoolUpdate {
        PoolUpdate {
            pool_id: PoolId {
                chain_id: 1,
                address: Address::with_last_byte(1),
            },
            state: V2PoolState {
                reserve0: U256::from(block),
                reserve1: U256::from(block),
                block_number: block,
                block_hash: None,
                updated_at: SystemTime::now(),
            },
            log_index: index,
        }
    }

    #[test]
    fn rejects_duplicate_and_out_of_order_updates() {
        let cache = PoolStateCache::new();
        assert!(cache.apply(update(10, 1)).unwrap());
        assert!(!cache.apply(update(10, 1)).unwrap());
        assert!(!cache.apply(update(9, 9)).unwrap());
        assert!(cache.apply(update(10, 2)).unwrap());
    }

    #[test]
    fn enforces_configured_snapshot_block_gap() {
        let cache = PoolStateCache::new();
        let first = update(100, 1);
        let first_id = first.pool_id.clone();
        cache.apply(first).unwrap();
        let mut second = update(99, 1);
        second.pool_id.address = Address::with_last_byte(2);
        let second_id = second.pool_id.clone();
        cache.apply(second).unwrap();
        assert!(
            cache
                .snapshot(1, 100, &[first_id.clone(), second_id.clone()], 0)
                .is_err()
        );
        let snapshot = cache.snapshot(1, 100, &[first_id, second_id], 1).unwrap();
        assert_eq!(snapshot.min_state_block, 99);
        assert_eq!(snapshot.max_state_block, 100);
    }
}
