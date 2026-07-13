//! Redfish TelemetryService endpoints
//!
//! Implements:
//! - GET    /redfish/v1/TelemetryService
//! - GET    /redfish/v1/TelemetryService/MetricDefinitions
//! - GET    /redfish/v1/TelemetryService/MetricReportDefinitions
//! - GET    /redfish/v1/TelemetryService/MetricReports
//! - GET    /redfish/v1/TelemetryService/Triggers
//! - POST   /redfish/v1/TelemetryService/Triggers
//! - GET    /redfish/v1/TelemetryService/Triggers/{trigger_id}
//! - PATCH  /redfish/v1/TelemetryService/Triggers/{trigger_id}
//! - DELETE /redfish/v1/TelemetryService/Triggers/{trigger_id}
//!
//! On OpenBMC, telemetry is managed by the telemetry daemon which exposes
//! readings via xyz.openbmc_project.Telemetry.
//!
//! Reference: DMTF Redfish TelemetryService schema v1.3.2

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info};

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

/// GET /redfish/v1/TelemetryService/Triggers
pub async fn get_triggers_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService/Triggers");

    let triggers = state.telemetry_triggers.read().await;
    let members: Vec<Value> = triggers
        .iter()
        .map(|t| json!({ "@odata.id": format!("/redfish/v1/TelemetryService/Triggers/{}", t["Id"].as_str().unwrap_or("")) }))
        .collect();
    let count = members.len();
    Ok(Json(json!({
        "@odata.type": "#TriggersCollection.TriggersCollection",
        "@odata.id": "/redfish/v1/TelemetryService/Triggers",
        "Name": "Triggers Collection",
        "Members@odata.count": count,
        "Members": members
    })))
}

/// POST /redfish/v1/TelemetryService/Triggers
pub async fn create_trigger(
    State(state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<Value>,
) -> Result<(StatusCode, [(String, String); 1], Json<Value>), StatusCode> {
    debug!("POST /redfish/v1/TelemetryService/Triggers");

    let id = uuid::Uuid::new_v4().to_string();
    let mut trigger = body.clone();
    trigger["Id"] = json!(id);
    trigger["@odata.type"] = json!("#Triggers.v1_3_0.Triggers");
    trigger["@odata.id"] = json!(format!("/redfish/v1/TelemetryService/Triggers/{}", id));
    if trigger.get("Name").is_none() {
        trigger["Name"] = json!(format!("Trigger {}", id));
    }

    let location = format!("/redfish/v1/TelemetryService/Triggers/{}", id);
    info!("Created telemetry trigger {}", id);

    state.telemetry_triggers.write().await.push(trigger.clone());

    Ok((
        StatusCode::CREATED,
        [("Location".to_string(), location)],
        Json(trigger),
    ))
}

/// GET /redfish/v1/TelemetryService/Triggers/{trigger_id}
pub async fn get_trigger(
    State(state): State<Arc<AppState>>,
    Path(trigger_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TelemetryService/Triggers/{}", trigger_id);

    let triggers = state.telemetry_triggers.read().await;
    let trigger = triggers
        .iter()
        .find(|t| t["Id"].as_str() == Some(&trigger_id))
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(trigger))
}

/// PATCH /redfish/v1/TelemetryService/Triggers/{trigger_id}
pub async fn patch_trigger(
    State(state): State<Arc<AppState>>,
    Path(trigger_id): Path<String>,
    JsonBody(patch): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/TelemetryService/Triggers/{}", trigger_id);

    let mut triggers = state.telemetry_triggers.write().await;
    let trigger = triggers
        .iter_mut()
        .find(|t| t["Id"].as_str() == Some(&trigger_id))
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            trigger[k] = v.clone();
        }
    }
    Ok(Json(trigger.clone()))
}

/// DELETE /redfish/v1/TelemetryService/Triggers/{trigger_id}
pub async fn delete_trigger(
    State(state): State<Arc<AppState>>,
    Path(trigger_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/TelemetryService/Triggers/{}", trigger_id);

    let mut triggers = state.telemetry_triggers.write().await;
    let len_before = triggers.len();
    triggers.retain(|t| t["Id"].as_str() != Some(&trigger_id));
    if triggers.len() == len_before {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
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
