//! Redfish AggregationService endpoint
//!
//! Implements:
//! - GET /redfish/v1/AggregationService
//!
//! Reference: DMTF Redfish AggregationService schema v1.0.1
//! Upstream: redfish-core/lib/aggregation_service.hpp
//!
//! The AggregationService enables a BMC to act as a Redfish aggregator,
//! proxying requests to satellite BMCs.  In bmcweb-ng this is a stub
//! that advertises the service is present but disabled (no aggregation
//! targets configured).
//!
//! OpenBMC DBus sources: none (feature flag driven in upstream)

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::debug;

use crate::AppState;

/// GET /redfish/v1/AggregationService
///
/// Returns the AggregationService resource.
///
/// The Redfish AggregationService allows a management controller to aggregate
/// multiple satellite BMCs into a single Redfish tree.  For now we advertise
/// the service as disabled with no aggregation sources.
pub async fn get_aggregation_service(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AggregationService");

    Ok(Json(json!({
        "@odata.type": "#AggregationService.v1_0_1.AggregationService",
        "@odata.id": "/redfish/v1/AggregationService",
        "Id": "AggregationService",
        "Name": "Aggregation Service",
        "Description": "Redfish Aggregation Service",
        "ServiceEnabled": false,
        "AggregationSources": {
            "@odata.id": "/redfish/v1/AggregationService/AggregationSources"
        },
        "ConnectionMethods": {
            "@odata.id": "/redfish/v1/AggregationService/ConnectionMethods"
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_aggregation_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_aggregation_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(
            json["@odata.type"],
            "#AggregationService.v1_0_1.AggregationService"
        );
        assert_eq!(json["Id"], "AggregationService");
        assert_eq!(json["ServiceEnabled"], false);
    }
}
