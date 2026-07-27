//! Multi-venue gateway registry and order router.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Local};

use crate::{ExchangeGateway, GatewayError, GatewayInfo, GatewayResult, OrderRequest};

/// An order plus its explicit venue route.  Keeping the route outside the
/// legacy `OrderRequest` preserves compatibility with existing Polymarket code.
#[derive(Debug, Clone)]
pub struct RoutedOrderRequest {
    pub venue: String,
    pub order: OrderRequest,
}

impl RoutedOrderRequest {
    pub fn new(venue: impl Into<String>, order: OrderRequest) -> Self {
        Self {
            venue: venue.into(),
            order,
        }
    }
}

/// Thread-safe collection of independently configured exchange gateways.
#[derive(Default)]
pub struct GatewayRouter {
    gateways: RwLock<HashMap<String, Arc<dyn ExchangeGateway>>>,
}

impl GatewayRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        venue: impl Into<String>,
        gateway: Arc<dyn ExchangeGateway>,
    ) -> Result<(), GatewayError> {
        let venue = venue.into().trim().to_ascii_lowercase();
        if venue.is_empty() {
            return Err(GatewayError::validation("venue cannot be empty"));
        }
        let mut gateways = self
            .gateways
            .write()
            .map_err(|_| GatewayError::exchange("gateway router lock poisoned"))?;
        if gateways.contains_key(&venue) {
            return Err(GatewayError::validation(format!(
                "venue already registered: {venue}"
            )));
        }
        gateways.insert(venue, gateway);
        Ok(())
    }

    pub fn remove(&self, venue: &str) -> Option<Arc<dyn ExchangeGateway>> {
        self.gateways
            .write()
            .ok()?
            .remove(&venue.to_ascii_lowercase())
    }

    pub fn get(&self, venue: &str) -> Result<Arc<dyn ExchangeGateway>, GatewayError> {
        self.gateways
            .read()
            .map_err(|_| GatewayError::exchange("gateway router lock poisoned"))?
            .get(&venue.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| GatewayError::validation(format!("unknown venue: {venue}")))
    }

    pub fn venues(&self) -> Vec<String> {
        let mut venues = self
            .gateways
            .read()
            .map(|g| g.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        venues.sort();
        venues
    }

    pub async fn submit_order(
        &self,
        request: &RoutedOrderRequest,
        now: DateTime<Local>,
    ) -> GatewayResult {
        match self.get(&request.venue) {
            Ok(gateway) => gateway.submit_order(&request.order, now).await,
            Err(error) => {
                GatewayResult::rejected(&request.order.client_order_id, &error.to_string(), 0)
            }
        }
    }

    pub async fn health_all(&self) -> Vec<(String, GatewayInfo)> {
        let gateways = self
            .gateways
            .read()
            .map(|g| {
                g.iter()
                    .map(|(v, gw)| (v.clone(), Arc::clone(gw)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut result = Vec::with_capacity(gateways.len());
        for (venue, gateway) in gateways {
            result.push((venue, gateway.health().await));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GatewayConfig, MockGateway};

    #[test]
    fn registers_multiple_venues_without_changing_legacy_factory() {
        let router = GatewayRouter::new();
        router
            .register(
                "cex-a",
                Arc::new(MockGateway::new(GatewayConfig::default())),
            )
            .unwrap();
        router
            .register(
                "cex-b",
                Arc::new(MockGateway::new(GatewayConfig::default())),
            )
            .unwrap();
        assert_eq!(router.venues(), vec!["cex-a", "cex-b"]);
        assert!(router.get("CEX-A").is_ok());
        assert!(
            router
                .register("cex-a", Arc::new(MockGateway::default()))
                .is_err()
        );
    }
}
