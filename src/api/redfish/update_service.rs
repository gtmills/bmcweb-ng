//! Redfish UpdateService endpoints
//!
//! Implements the Redfish UpdateService resource family:
//! - GET  /redfish/v1/UpdateService
//! - GET  /redfish/v1/UpdateService/FirmwareInventory
//! - GET  /redfish/v1/UpdateService/FirmwareInventory/{firmware_id}
//! - POST /redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate
//!
//! Reference: DMTF DSP0266, UpdateService schema v1.14.0,
//! SoftwareInventory schema v1.10.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::services::{FirmwareInventory, UpdateProtocol, UpdateRequest, UpdateTarget};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request body for SimpleUpdate action
#[derive(Debug, Deserialize)]
pub struct SimpleUpdateRequest {
    #[serde(rename = "ImageURI")]
    pub image_uri: String,
    #[serde(rename = "TransferProtocol", default = "default_http_protocol")]
    pub transfer_protocol: String,
    #[serde(rename = "Targets", default)]
    pub targets: Vec<String>,
    #[serde(rename = "Username")]
    pub username: Option<String>,
    #[serde(rename = "Password")]
    pub password: Option<String>,
}

fn default_http_protocol() -> String {
    "HTTP".to_string()
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn firmware_to_json(fw: &FirmwareInventory) -> Value {
    json!({
        "@odata.type": "#SoftwareInventory.v1_10_0.SoftwareInventory",
        "@odata.id": format!("/redfish/v1/UpdateService/FirmwareInventory/{}", fw.id),
        "Id": fw.id,
        "Name": fw.name,
        "Version": fw.version,
        "Updateable": fw.updateable,
        "Status": {
            "State": if fw.is_active { "Enabled" } else { "StandbyOffline" },
            "Health": "OK"
        },
        "SoftwareId": format!("{}-{}", fw.target.as_str(), fw.version),
        "LowestSupportedVersion": fw.version,
    })
}

fn parse_transfer_protocol(s: &str) -> Option<UpdateProtocol> {
    match s {
        "HTTP" | "HTTPS" => Some(UpdateProtocol::HTTP),
        "TFTP" => Some(UpdateProtocol::TFTP),
        "SCP" => Some(UpdateProtocol::SCP),
        "LOCAL" => Some(UpdateProtocol::Local),
        _ => None,
    }
}

fn target_uri_to_update_target(uri: &str) -> Option<UpdateTarget> {
    if uri.contains("bmc") || uri.contains("BMC") {
        Some(UpdateTarget::BMC)
    } else if uri.contains("bios") || uri.contains("BIOS") {
        Some(UpdateTarget::BIOS)
    } else if uri.contains("cpld") || uri.contains("CPLD") {
        Some(UpdateTarget::CPLD)
    } else if uri.contains("fpga") || uri.contains("FPGA") {
        Some(UpdateTarget::FPGA)
    } else {
        Some(UpdateTarget::Other)
    }
}

// ---------------------------------------------------------------------------
// UpdateService resource
// ---------------------------------------------------------------------------

/// GET /redfish/v1/UpdateService
pub async fn get_update_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService");

    let update_service = state.update_service.as_ref();
    let active_ops = update_service
        .map(|s| s.active_operation_count())
        .unwrap_or(0);

    let response = json!({
        "@odata.type": "#UpdateService.v1_14_0.UpdateService",
        "@odata.id": "/redfish/v1/UpdateService",
        "Id": "UpdateService",
        "Name": "Update Service",
        "Description": "Redfish Update Service",
        "ServiceEnabled": true,
        "HttpPushUri": "/redfish/v1/UpdateService",
        "HttpPushUriOptions": {
            "HttpPushUriApplyTime": {
                "ApplyTime": "Immediate"
            }
        },
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "FirmwareInventory": {
            "@odata.id": "/redfish/v1/UpdateService/FirmwareInventory"
        },
        "SoftwareInventory": {
            "@odata.id": "/redfish/v1/UpdateService/SoftwareInventory"
        },
        "Actions": {
            "#UpdateService.SimpleUpdate": {
                "target": "/redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate",
                "TransferProtocol@Redfish.AllowableValues": [
                    "HTTP",
                    "HTTPS",
                    "TFTP",
                    "SCP",
                    "LOCAL"
                ]
            }
        }
    });

    Ok(Json(response))
}

/// GET /redfish/v1/UpdateService/FirmwareInventory
pub async fn get_firmware_inventory_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService/FirmwareInventory");

    let update_service = state
        .update_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let firmware = update_service.get_all_firmware();
    let members: Vec<Value> = firmware
        .iter()
        .map(|f| {
            json!({ "@odata.id": format!("/redfish/v1/UpdateService/FirmwareInventory/{}", f.id) })
        })
        .collect();
    let count = members.len();

    let response = json!({
        "@odata.type": "#SoftwareInventoryCollection.SoftwareInventoryCollection",
        "@odata.id": "/redfish/v1/UpdateService/FirmwareInventory",
        "Name": "Firmware Inventory Collection",
        "Members@odata.count": count,
        "Members": members,
    });

    Ok(Json(response))
}

/// GET /redfish/v1/UpdateService/FirmwareInventory/{firmware_id}
pub async fn get_firmware_inventory(
    State(state): State<Arc<AppState>>,
    Path(firmware_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService/FirmwareInventory/{}", firmware_id);

    let update_service = state
        .update_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    match update_service.get_firmware(&firmware_id) {
        Some(fw) => Ok(Json(firmware_to_json(&fw))),
        None => {
            warn!("Firmware '{}' not found in inventory", firmware_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// POST /redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate
///
/// Initiates a firmware update from a remote URI.  Returns 202 Accepted
/// with a `Location` header pointing to the newly created Task.
pub async fn simple_update(
    State(state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<SimpleUpdateRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<Value>), StatusCode> {
    debug!(
        "POST /redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate - URI: {}",
        body.image_uri
    );

    if body.image_uri.is_empty() {
        warn!("Missing ImageURI in SimpleUpdate request");
        return Err(StatusCode::BAD_REQUEST);
    }

    let protocol = parse_transfer_protocol(&body.transfer_protocol).ok_or_else(|| {
        warn!("Unsupported transfer protocol: {}", body.transfer_protocol);
        StatusCode::BAD_REQUEST
    })?;

    // Determine update target from Targets list or default to BMC
    let target = body
        .targets
        .first()
        .and_then(|t| target_uri_to_update_target(t))
        .unwrap_or(UpdateTarget::BMC);

    // Create a task to track this update
    let task_service = state
        .task_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let task = task_service
        .create_task(
            format!("Firmware Update: {}", target.as_str()),
            Some(format!("Updating {} firmware from {}", target.as_str(), body.image_uri)),
        )
        .map_err(|e| {
            warn!("Failed to create task for firmware update: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    // Start the update operation
    let update_service = state
        .update_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let request = UpdateRequest {
        target,
        protocol,
        image_uri: Some(body.image_uri),
        local_path: None,
        username: body.username,
        password: body.password,
        apply_immediately: true,
    };

    update_service.start_update(request).map_err(|e| {
        warn!("Failed to start firmware update: {}", e);
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    info!("Firmware update started, tracking via task '{}'", task.id);

    let location = format!("/redfish/v1/TaskService/Tasks/{}", task.id);
    let response_body = json!({
        "@odata.type": "#Task.v1_8_0.Task",
        "@odata.id": location,
        "Id": task.id,
        "Name": task.name,
        "TaskState": "Running",
        "TaskStatus": "OK"
    });

    Ok((
        StatusCode::ACCEPTED,
        [(
            "Location".to_string(),
            location,
        )],
        Json(response_body),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_update_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_update_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#UpdateService.v1_14_0.UpdateService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_firmware_inventory_empty() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_firmware_inventory_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_firmware_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_firmware_inventory(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_parse_transfer_protocol() {
        assert!(matches!(parse_transfer_protocol("HTTP"), Some(UpdateProtocol::HTTP)));
        assert!(matches!(parse_transfer_protocol("HTTPS"), Some(UpdateProtocol::HTTP)));
        assert!(matches!(parse_transfer_protocol("TFTP"), Some(UpdateProtocol::TFTP)));
        assert!(parse_transfer_protocol("FTP").is_none());
    }
}
