//! Redfish ComputerSystem and ComputerSystemCollection endpoints
//!
//! Implements /redfish/v1/Systems and /redfish/v1/Systems/{SystemId}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;

/// GET /redfish/v1/Systems
///
/// Returns the ComputerSystemCollection
pub async fn get_systems_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems");

    let response = json!({
        "@odata.type": "#ComputerSystemCollection.ComputerSystemCollection",
        "@odata.id": "/redfish/v1/Systems",
        "Name": "Computer System Collection",
        "Members@odata.count": 1,
        "Members": [
            {
                "@odata.id": "/redfish/v1/Systems/system"
            }
        ]
    });

    Ok(Json(response))
}

/// GET /redfish/v1/Systems/{system_id}
///
/// Returns a specific ComputerSystem resource
pub async fn get_system(
    State(state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Systems/{}", system_id);

    // For now, only support "system" as the system ID
    if system_id != "system" {
        warn!("System ID '{}' not found", system_id);
        return Err(StatusCode::NOT_FOUND);
    }

    // TODO: Query actual system information from DBus
    // For now, return a basic static response
    let response = json!({
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
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "PowerState": "On",
        "BiosVersion": "Unknown",
        "ProcessorSummary": {
            "Count": 0,
            "Model": "Unknown",
            "Status": {
                "State": "Enabled",
                "Health": "OK"
            }
        },
        "MemorySummary": {
            "TotalSystemMemoryGiB": 0,
            "Status": {
                "State": "Enabled",
                "Health": "OK"
            }
        },
        "Boot": {
            "BootSourceOverrideEnabled": "Disabled",
            "BootSourceOverrideMode": "UEFI",
            "BootSourceOverrideTarget": "None",
            "BootSourceOverrideTarget@Redfish.AllowableValues": [
                "None",
                "Pxe",
                "Hdd",
                "Cd",
                "BiosSetup",
                "UefiShell",
                "UefiTarget"
            ]
        },
        "Links": {
            "Chassis": [
                {
                    "@odata.id": "/redfish/v1/Chassis/chassis"
                }
            ],
            "ManagedBy": [
                {
                    "@odata.id": "/redfish/v1/Managers/bmc"
                }
            ]
        },
        "Actions": {
            "#ComputerSystem.Reset": {
                "target": "/redfish/v1/Systems/system/Actions/ComputerSystem.Reset",
                "@Redfish.ActionInfo": "/redfish/v1/Systems/system/ResetActionInfo"
            }
        },
        "Processors": {
            "@odata.id": "/redfish/v1/Systems/system/Processors"
        },
        "Memory": {
            "@odata.id": "/redfish/v1/Systems/system/Memory"
        },
        "Storage": {
            "@odata.id": "/redfish/v1/Systems/system/Storage"
        },
        "EthernetInterfaces": {
            "@odata.id": "/redfish/v1/Systems/system/EthernetInterfaces"
        },
        "NetworkInterfaces": {
            "@odata.id": "/redfish/v1/Systems/system/NetworkInterfaces"
        },
        "LogServices": {
            "@odata.id": "/redfish/v1/Systems/system/LogServices"
        }
    });

    Ok(Json(response))
}

/// POST /redfish/v1/Systems/{system_id}/Actions/ComputerSystem.Reset
///
/// Performs a reset action on the system
pub async fn reset_system(
    State(_state): State<Arc<AppState>>,
    Path(system_id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    debug!("POST /redfish/v1/Systems/{}/Actions/ComputerSystem.Reset", system_id);

    if system_id != "system" {
        return Err(StatusCode::NOT_FOUND);
    }

    // Extract reset type from payload
    let reset_type = payload.get("ResetType")
        .and_then(|v| v.as_str())
        .unwrap_or("On");

    debug!("Reset type requested: {}", reset_type);

    // Validate reset type
    match reset_type {
        "On" | "ForceOff" | "GracefulShutdown" | "GracefulRestart" | 
        "ForceRestart" | "Nmi" | "ForceOn" | "PushPowerButton" => {
            // TODO: Implement actual reset via DBus
            warn!("Reset action not yet implemented, would perform: {}", reset_type);
            Ok(StatusCode::NO_CONTENT)
        }
        _ => {
            warn!("Invalid reset type: {}", reset_type);
            Err(StatusCode::BAD_REQUEST)
        }
    }
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
    }

    #[tokio::test]
    async fn test_get_system_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_system(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
