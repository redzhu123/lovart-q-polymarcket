use std::collections::HashMap;
use std::sync::Mutex;

use alloy_primitives::B256;
use async_trait::async_trait;

use super::error::{DexV2Error, DexV2Result};
use super::types::{ArbitrageOpportunity, OpportunityStatus};

#[async_trait]
pub trait OpportunityRepository: Send + Sync {
    async fn save_opportunity(&self, opportunity: &ArbitrageOpportunity) -> DexV2Result<()>;
    async fn update_status(&self, id: B256, status: OpportunityStatus) -> DexV2Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryOpportunityRepository {
    opportunities: Mutex<HashMap<B256, ArbitrageOpportunity>>,
}

impl InMemoryOpportunityRepository {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, id: &B256) -> Option<ArbitrageOpportunity> {
        self.opportunities.lock().ok()?.get(id).cloned()
    }
    pub fn len(&self) -> usize {
        self.opportunities
            .lock()
            .map(|items| items.len())
            .unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl OpportunityRepository for InMemoryOpportunityRepository {
    async fn save_opportunity(&self, opportunity: &ArbitrageOpportunity) -> DexV2Result<()> {
        self.opportunities
            .lock()
            .map_err(|_| DexV2Error::Repository("repository lock poisoned".into()))?
            .insert(opportunity.id, opportunity.clone());
        Ok(())
    }
    async fn update_status(&self, id: B256, status: OpportunityStatus) -> DexV2Result<()> {
        let mut items = self
            .opportunities
            .lock()
            .map_err(|_| DexV2Error::Repository("repository lock poisoned".into()))?;
        let opportunity = items
            .get_mut(&id)
            .ok_or_else(|| DexV2Error::Repository("unknown opportunity".into()))?;
        if !opportunity.status.can_transition_to(status) {
            return Err(DexV2Error::Repository(format!(
                "invalid opportunity transition {:?} -> {:?}",
                opportunity.status, status
            )));
        }
        opportunity.status = status;
        Ok(())
    }
}
