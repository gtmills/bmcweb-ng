//! Redfish ComputerSystem and ComputerSystemCollection endpoints
//!
//! Implements:
//! - GET  /redfish/v1/Systems
//! - GET  /redfish/v1/Systems/{system_id}
//! - POST /redfish/v1/Systems/{system_id}/Actions/ComputerSystem.Reset
//! - GET  /redfish/v1/Systems/{system_id}/Processors
//! - GET  /redfish/v1/Systems/{system_id}/Memory
//! - GET  /redfish/v1/Systems/{system_id}/LogServices
//! - GET  /redfish/v1/Systems/{system_id}/Storage
//! - GET  /redfish/v1/Systems/{system_id}/EthernetInterfaces
//!
//! Reference: DMTF Redfish ComputerSystem schema v1.20.0
//!
//! OpenBMC DBus sources:
//!   - Power state:   xyz.openbmc_project.State.Host / CurrentHostState
//!   - Boot settings: xyz.openbmc_project.Control.Boot.*
//!   - Processor inventory: xyz.openbmc_project.Inventory.Item.Cpu
//!   - Memory inventory:    xyz.openbmc_project.Inventory.Item.Dimm

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_system_id(system_id: &str) -> Result<(), StatusCode> {
    if system_id == "system" {
        Ok(())
    } else {
        warn!("System '{}' not found", system_id);
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Systems collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Systems
pub async fn get_systems_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems");

    Ok(Json(json!({
        "@odata.type": "#ComputerSystemCollection.ComputerSystemCollection",
        "@odata.id": "/redfish/v1/Systems",
        "Name": "Computer System Collection",
        "Members@odata.count": 1,
        "Members": [{ "@odata.id": "/redfish/v1/Systems/system" }]
    })))
}

/// GET /redfish/v1/Systems/{system_id}
pub async fn get_system(
    State(state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}", system_id);
    validate_system_id(&system_id)?;

    // TODO: Query live power/boot state from DBus:
    //   - xyz.openbmc_project.State.Host / CurrentHostState
    //   - xyz.openbmc_project.Control.Boot.Source / BootSource
    Ok(Json(json!({
        "@odata.type": "#ComputerSystem.v1_20_0.ComputerSystem",
        "@odata.id": "/redfish/v1/Systems/system",
        "Id": "system",
        "Name": "System",
        "Description": "Computer System",
        "SystemType": "Physical",
        "Manufacturer": "OpenBMC",
        "Model": "Unknown",
        "SerialNumber": "Unknown",
        "PartNumber": "Unknown",
        "UUID": state.system_uuid,
        "Status": { "State": "Enabled", "Health": "OK", "HealthRollup": "OK" },
        "PowerState": "On",
        "BiosVersion": "Unknown",
        "ProcessorSummary": {
            "Count": 0,
            "Model": "Unknown",
            "Status": { "State": "Enabled", "Health": "OK" }
        },
        "MemorySummary": {
            "TotalSystemMemoryGiB": 0,
            "Status": { "State": "Enabled", "Health": "OK" }
        },
        "Boot": {
            "BootSourceOverrideEnabled": "Disabled",
            "BootSourceOverrideMode": "UEFI",
            "BootSourceOverrideTarget": "None",
            "BootSourceOverrideTarget@Redfish.AllowableValues": [
                "None", "Pxe", "Hdd", "Cd", "BiosSetup", "UefiShell", "UefiTarget"
            ]
        },
        "Links": {
            "Chassis": [{ "@odata.id": "/redfish/v1/Chassis/chassis" }],
            "ManagedBy": [{ "@odata.id": "/redfish/v1/Managers/bmc" }]
        },
        "Actions": {
            "#ComputerSystem.Reset": {
                "target": "/redfish/v1/Systems/system/Actions/ComputerSystem.Reset",
                "@Redfish.ActionInfo": "/redfish/v1/Systems/system/ResetActionInfo",
                "ResetType@Redfish.AllowableValues": [
                    "On", "ForceOff", "GracefulShutdown", "GracefulRestart",
                    "ForceRestart", "Nmi", "ForceOn", "PushPowerButton"
                ]
            }
        },
        "Processors": { "@odata.id": "/redfish/v1/Systems/system/Processors" },
        "Memory": { "@odata.id": "/redfish/v1/Systems/system/Memory" },
        "Storage": { "@odata.id": "/redfish/v1/Systems/system/Storage" },
        "EthernetInterfaces": { "@odata.id": "/redfish/v1/Systems/system/EthernetInterfaces" },
        "NetworkInterfaces": { "@odata.id": "/redfish/v1/Systems/system/NetworkInterfaces" },
        "LogServices": { "@odata.id": "/redfish/v1/Systems/system/LogServices" }
    })))
}

/// POST /redfish/v1/Systems/{system_id}/Actions/ComputerSystem.Reset
///
/// Performs a power/reset action.
///
/// On OpenBMC this maps to:
///   - xyz.openbmc_project.State.Host.RequestedHostTransition property
///   - xyz.openbmc_project.State.Chassis.RequestedPowerTransition property
pub async fn reset_system(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
    JsonBody(payload): JsonBody<Value>,
) -> Result<StatusCode, StatusCode> {
    debug!(
        "POST /redfish/v1/Systems/{}/Actions/ComputerSystem.Reset",
        system_id
    );
    validate_system_id(&system_id)?;

    let reset_type = payload
        .get("ResetType")
        .and_then(|v| v.as_str())
        .unwrap_or("On");

    match reset_type {
        "On" | "ForceOff" | "GracefulShutdown" | "GracefulRestart"
        | "ForceRestart" | "Nmi" | "ForceOn" | "PushPowerButton" => {
            // TODO: Write to xyz.openbmc_project.State.Host.RequestedHostTransition
            warn!(
                "System reset '{}' requested — DBus implementation pending",
                reset_type
            );
            Ok(StatusCode::NO_CONTENT)
        }
        _ => {
            warn!("Invalid ResetType: {}", reset_type);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-resources
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Systems/{system_id}/Processors
///
/// On OpenBMC, processors are enumerated via:
///   xyz.openbmc_project.Inventory.Item.Cpu on the inventory bus.
pub async fn get_processors_collection(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}/Processors", system_id);
    validate_system_id(&system_id)?;

    // TODO: Enumerate processors from DBus inventory
    Ok(Json(json!({
        "@odata.type": "#ProcessorCollection.ProcessorCollection",
        "@odata.id": "/redfish/v1/Systems/system/Processors",
        "Name": "Processor Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

/// GET /redfish/v1/Systems/{system_id}/Memory
///
/// On OpenBMC, DIMMs are enumerated via:
///   xyz.openbmc_project.Inventory.Item.Dimm on the inventory bus.
pub async fn get_memory_collection(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}/Memory", system_id);
    validate_system_id(&system_id)?;

    // TODO: Enumerate DIMMs from DBus inventory
    Ok(Json(json!({
        "@odata.type": "#MemoryCollection.MemoryCollection",
        "@odata.id": "/redfish/v1/Systems/system/Memory",
        "Name": "Memory Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

/// GET /redfish/v1/Systems/{system_id}/LogServices
///
/// On OpenBMC, log services include the system event log (SEL) and host
/// logger, backed by xyz.openbmc_project.Logging and
/// xyz.openbmc_project.Dump.Manager.
pub async fn get_system_log_services(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}/LogServices", system_id);
    validate_system_id(&system_id)?;

    Ok(Json(json!({
        "@odata.type": "#LogServiceCollection.LogServiceCollection",
        "@odata.id": "/redfish/v1/Systems/system/LogServices",
        "Name": "System Log Services Collection",
        "Description": "Collection of LogServices for this Computer System",
        "Members@odata.count": 1,
        "Members": [
            { "@odata.id": "/redfish/v1/Systems/system/LogServices/EventLog" }
        ]
    })))
}

/// GET /redfish/v1/Systems/{system_id}/Storage
pub async fn get_storage_collection(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}/Storage", system_id);
    validate_system_id(&system_id)?;

    // TODO: Enumerate storage controllers from DBus
    Ok(Json(json!({
        "@odata.type": "#StorageCollection.StorageCollection",
        "@odata.id": "/redfish/v1/Systems/system/Storage",
        "Name": "Storage Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

/// GET /redfish/v1/Systems/{system_id}/EthernetInterfaces
pub async fn get_ethernet_interfaces_collection(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}/EthernetInterfaces", system_id);
    validate_system_id(&system_id)?;

    // TODO: Enumerate host NIC interfaces from DBus
    Ok(Json(json!({
        "@odata.type": "#EthernetInterfaceCollection.EthernetInterfaceCollection",
        "@odata.id": "/redfish/v1/Systems/system/EthernetInterfaces",
        "Name": "Ethernet Interface Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_systems_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_systems_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ComputerSystemCollection.ComputerSystemCollection");
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_system() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_system(State(state), Path("system".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ComputerSystem.v1_20_0.ComputerSystem");
        assert_eq!(json["Id"], "system");
        assert!(json["Processors"]["@odata.id"].is_string());
        assert!(json["Memory"]["@odata.id"].is_string());
        assert!(json["LogServices"]["@odata.id"].is_string());
    }

    #[tokio::test]
    async fn test_get_system_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_system(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_processors_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_processors_collection(State(state), Path("system".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_memory_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_memory_collection(State(state), Path("system".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_system_log_services() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_system_log_services(State(state), Path("system".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 1);
    }
}
