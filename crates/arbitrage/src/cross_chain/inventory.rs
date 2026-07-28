use std::collections::HashMap;

use alloy_primitives::{Address, U256};

use crate::dex_router::{RouterError, RouterResult};

#[derive(Debug, Clone, Default)]
pub struct InventoryLedger {
    balances: HashMap<(u64, Address), U256>,
}

impl InventoryLedger {
    pub fn set(&mut self, chain_id: u64, token: Address, amount: U256) {
        self.balances.insert((chain_id, token), amount);
    }

    pub fn balance(&self, chain_id: u64, token: Address) -> U256 {
        self.balances
            .get(&(chain_id, token))
            .copied()
            .unwrap_or_default()
    }

    pub fn apply_delta(
        &mut self,
        chain_id: u64,
        token: Address,
        credit: U256,
        debit: U256,
    ) -> RouterResult<()> {
        let current = self.balance(chain_id, token);
        let next = current
            .checked_add(credit)
            .and_then(|value| value.checked_sub(debit))
            .ok_or_else(|| RouterError::Quote("跨链库存不足或溢出".into()))?;
        self.set(chain_id, token, next);
        Ok(())
    }
}
