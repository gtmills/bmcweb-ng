//! Redfish UpdateService endpoints
//!
//! Implements the Redfish UpdateService resource family:
//! - GET  /redfish/v1/UpdateService
//! - GET  /redfish/v1/UpdateService/FirmwareInventory
//! - GET  /redfish/v1/UpdateService/FirmwareInventory/{firmware_id}
//! - GET  /redfish/v1/UpdateService/SoftwareInventory
//! - GET  /redfish/v1/UpdateService/SoftwareInventory/{software_id}
//! - POST /redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate
//!
//! Reference: DMTF DSP0266, UpdateService schema v1.14.0,
//! SoftwareInventory schema v1.10.0

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_ACTION};
use crate::auth::session::UserSession;
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
    let _active_ops = update_service
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

/// Helper: enumerate software version objects from DBus.
///
/// On OpenBMC, software images are at:
///   /xyz/openbmc_project/software/<id>
///   interface: xyz.openbmc_project.Software.Version
///   properties: Version (string), Purpose (enum)
///
/// Purpose enum:
///   .BMC     → BMC firmware
///   .Host    → Host/BIOS firmware
///   .System  → System firmware
///   .Other   → Other
async fn dbus_firmware_members(conn: &zbus::Connection) -> Vec<Value> {
    use crate::dbus::{DbusClient, ZBusClient};
    let client = ZBusClient::from_connection(conn.clone());
    let sw_iface = "xyz.openbmc_project.Software.Version";
    let act_iface = "xyz.openbmc_project.Software.Activation";

    match client
        .get_managed_objects(
            "xyz.openbmc_project.Software.BMC.Updater",
            "/xyz/openbmc_project/software",
        )
        .await
    {
        Ok(objects) => {
            objects
                .iter()
                .filter(|(path, ifaces)| {
                    ifaces.contains_key(sw_iface)
                        && path.starts_with("/xyz/openbmc_project/software/")
                        && path.as_str() != "/xyz/openbmc_project/software"
                })
                .filter_map(|(path, ifaces)| {
                    let id = path.rsplit('/').next()?;
                    let sw_props = &ifaces[sw_iface];
                    let version = sw_props
                        .get("Version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let purpose_raw = sw_props
                        .get("Purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let name = if purpose_raw.ends_with(".BMC") {
                        "BMC Firmware"
                    } else if purpose_raw.ends_with(".Host") {
                        "Host Firmware"
                    } else if purpose_raw.ends_with(".System") {
                        "System Firmware"
                    } else if purpose_raw.is_empty() {
                        "Firmware"
                    } else {
                        purpose_raw.rsplit('.').next().unwrap_or("Firmware")
                    };
                    let is_active = ifaces
                        .get(act_iface)
                        .and_then(|ap| ap.get("Activation"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.ends_with(".Active"))
                        .unwrap_or(false);
                    Some(json!({
                        "@odata.id": format!("/redfish/v1/UpdateService/FirmwareInventory/{}", id),
                        "Id": id,
                        "Name": name,
                        "Version": version,
                        "Updateable": true,
                        "Status": {
                            "State": if is_active { "Enabled" } else { "StandbyOffline" },
                            "Health": "OK"
                        }
                    }))
                })
                .collect()
        }
        Err(e) => {
            warn!("Failed to enumerate firmware from DBus: {}", e);
            vec![]
        }
    }
}

/// GET /redfish/v1/UpdateService/FirmwareInventory
///
/// Returns firmware inventory from both the in-memory update service and
/// live DBus software objects at /xyz/openbmc_project/software.
pub async fn get_firmware_inventory_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService/FirmwareInventory");

    // Always include in-memory firmware (from upload operations)
    let mut members: Vec<Value> = if let Some(update_service) = state.update_service.as_ref() {
        update_service
            .get_all_firmware()
            .iter()
            .map(|f| json!({ "@odata.id": format!("/redfish/v1/UpdateService/FirmwareInventory/{}", f.id) }))
            .collect()
    } else {
        vec![]
    };

    // Add DBus software version objects (deduplicated by @odata.id)
    if let Some(conn) = state.dbus_connection.as_deref() {
        let dbus_items = dbus_firmware_members(conn).await;
        let existing_ids: std::collections::HashSet<String> = members
            .iter()
            .filter_map(|v| v.get("@odata.id").and_then(|u| u.as_str()).map(|s| s.to_string()))
            .collect();
        for item in dbus_items {
            let oid = item.get("@odata.id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !existing_ids.contains(&oid) {
                members.push(json!({ "@odata.id": oid }));
            }
        }
    }

    let count = members.len();
    Ok(Json(json!({
        "@odata.type": "#SoftwareInventoryCollection.SoftwareInventoryCollection",
        "@odata.id": "/redfish/v1/UpdateService/FirmwareInventory",
        "Name": "Firmware Inventory Collection",
        "Members@odata.count": count,
        "Members": members,
    })))
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
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<SimpleUpdateRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<Value>), StatusCode> {
    debug!(
        "POST /redfish/v1/UpdateService/Actions/UpdateService.SimpleUpdate - URI: {}",
        body.image_uri
    );
    check_privilege(Some(&session), PRIVILEGE_ACTION)?;

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

/// GET /redfish/v1/UpdateService/SoftwareInventory
///
/// SoftwareInventory mirrors FirmwareInventory but covers host-side software
/// (BIOS, ME, etc.) in addition to BMC.  On p10bmc the same DBus software
/// objects are used; items with Purpose `.Host` or `.System` are shown here.
pub async fn get_software_inventory_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService/SoftwareInventory");

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let items = dbus_firmware_members(conn).await;
        items
            .into_iter()
            .map(|item| {
                // Rewrite the @odata.id path from FirmwareInventory to SoftwareInventory
                let id = item["Id"].as_str().unwrap_or("").to_string();
                json!({
                    "@odata.id": format!("/redfish/v1/UpdateService/SoftwareInventory/{}", id)
                })
            })
            .collect()
    } else {
        vec![]
    };

    let count = members.len();
    Ok(Json(json!({
        "@odata.type": "#SoftwareInventoryCollection.SoftwareInventoryCollection",
        "@odata.id": "/redfish/v1/UpdateService/SoftwareInventory",
        "Name": "Software Inventory Collection",
        "Members@odata.count": count,
        "Members": members,
    })))
}

/// GET /redfish/v1/UpdateService/SoftwareInventory/{software_id}
pub async fn get_software_inventory(
    State(state): State<Arc<AppState>>,
    Path(software_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/UpdateService/SoftwareInventory/{}", software_id);

    // Reuse the DBus firmware enumeration; find the entry by id.
    if let Some(conn) = state.dbus_connection.as_deref() {
        let items = dbus_firmware_members(conn).await;
        if let Some(item) = items.into_iter().find(|i| i["Id"].as_str() == Some(&software_id)) {
            let mut entry = item.clone();
            // Repoint @odata.id to SoftwareInventory path
            entry["@odata.id"] = json!(
                format!("/redfish/v1/UpdateService/SoftwareInventory/{}", software_id)
            );
            entry["@odata.type"] = json!("#SoftwareInventory.v1_10_0.SoftwareInventory");
            return Ok(Json(entry));
        }
    }

    Err(StatusCode::NOT_FOUND)
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
