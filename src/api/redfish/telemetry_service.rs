//! Redfish TelemetryService endpoints
//!
//! Implements:
//! - GET  /redfish/v1/TelemetryService
//! - GET  /redfish/v1/TelemetryService/MetricDefinitions
//! - GET  /redfish/v1/TelemetryService/MetricReportDefinitions
//! - GET  /redfish/v1/TelemetryService/MetricReports
//!
//! On OpenBMC, telemetry is managed by the telemetry daemon which exposes
//! readings via xyz.openbmc_project.Telemetry.
//!
//! Reference: DMTF Redfish TelemetryService schema v1.3.2

use axum::{extract::State, http::StatusCode, response::Json};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::debug;

use crate::AppState;

/// GET /redfish/v1/TelemetryService
pub async fn get_telemetry_service(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService");

    Ok(Json(json!({
        "@odata.type": "#TelemetryService.v1_3_2.TelemetryService",
        "@odata.id": "/redfish/v1/TelemetryService",
        "Id": "TelemetryService",
        "Name": "Telemetry Service",
        "Description": "The Telemetry Service is used for collecting and reporting metric data",
        "ServiceEnabled": true,
        "Status": { "State": "Enabled", "Health": "OK" },
        "SupportedCollectionFunctions": ["Average", "Maximum", "Minimum", "Summation"],
        "MetricDefinitions": {
            "@odata.id": "/redfish/v1/TelemetryService/MetricDefinitions"
        },
        "MetricReportDefinitions": {
            "@odata.id": "/redfish/v1/TelemetryService/MetricReportDefinitions"
        },
        "MetricReports": {
            "@odata.id": "/redfish/v1/TelemetryService/MetricReports"
        },
        "Triggers": {
            "@odata.id": "/redfish/v1/TelemetryService/Triggers"
        }
    })))
}

/// GET /redfish/v1/TelemetryService/MetricDefinitions
pub async fn get_metric_definitions(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService/MetricDefinitions");

    Ok(Json(json!({
        "@odata.type": "#MetricDefinitionCollection.MetricDefinitionCollection",
        "@odata.id": "/redfish/v1/TelemetryService/MetricDefinitions",
        "Name": "Metric Definition Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

/// GET /redfish/v1/TelemetryService/MetricReportDefinitions
pub async fn get_metric_report_definitions(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService/MetricReportDefinitions");

    Ok(Json(json!({
        "@odata.type": "#MetricReportDefinitionCollection.MetricReportDefinitionCollection",
        "@odata.id": "/redfish/v1/TelemetryService/MetricReportDefinitions",
        "Name": "Metric Report Definition Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

/// GET /redfish/v1/TelemetryService/MetricReports
pub async fn get_metric_reports(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService/MetricReports");

    Ok(Json(json!({
        "@odata.type": "#MetricReportCollection.MetricReportCollection",
        "@odata.id": "/redfish/v1/TelemetryService/MetricReports",
        "Name": "Metric Report Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_telemetry_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_telemetry_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "TelemetryService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_metric_definitions() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_metric_definitions(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }
}
