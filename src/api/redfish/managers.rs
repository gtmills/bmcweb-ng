//! Redfish Manager and ManagerCollection endpoints
//!
//! Implements /redfish/v1/Managers and /redfish/v1/Managers/{ManagerId}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;

/// GET /redfish/v1/Managers
///
/// Returns the ManagerCollection
pub async fn get_managers_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers");

    let response = json!({
        "@odata.type": "#ManagerCollection.ManagerCollection",
        "@odata.id": "/redfish/v1/Managers",
        "Name": "Manager Collection",
        "Members@odata.count": 1,
        "Members": [
            {
                "@odata.id": "/redfish/v1/Managers/bmc"
            }
        ]
    });

    Ok(Json(response))
}

/// GET /redfish/v1/Managers/{manager_id}
///
/// Returns a specific Manager resource
pub async fn get_manager(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}", manager_id);

    // For now, only support "bmc" as the manager ID
    if manager_id != "bmc" {
        warn!("Manager ID '{}' not found", manager_id);
        return Err(StatusCode::NOT_FOUND);
    }

    // TODO: Query actual manager information from DBus
    let response = json!({
        "@odata.type": "#Manager.v1_19_0.Manager",
        "@odata.id": "/redfish/v1/Managers/bmc",
        "Id": "bmc",
        "Name": "OpenBMC Manager",
        "Description": "Baseboard Management Controller",
        "ManagerType": "BMC",
        "UUID": state.system_uuid,
        "Model": "OpenBMC",
        "FirmwareVersion": "2.14.0-dev",
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "PowerState": "On",
        "DateTime": chrono::Utc::now().to_rfc3339(),
        "DateTimeLocalOffset": "+00:00",
        "ServiceEntryPointUUID": state.system_uuid,
        "Links": {
            "ManagerForServers": [
                {
                    "@odata.id": "/redfish/v1/Systems/system"
                }
            ],
            "ManagerForChassis": [
                {
                    "@odata.id": "/redfish/v1/Chassis/chassis"
                }
            ],
            "ManagerInChassis": {
                "@odata.id": "/redfish/v1/Chassis/chassis"
            }
        },
        "EthernetInterfaces": {
            "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces"
        },
        "NetworkProtocol": {
            "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol"
        },
        "LogServices": {
            "@odata.id": "/redfish/v1/Managers/bmc/LogServices"
        },
        "SerialConsole": {
            "ServiceEnabled": true,
            "MaxConcurrentSessions": 15,
            "ConnectTypesSupported": [
                "SSH",
                "IPMI"
            ]
        },
        "CommandShell": {
            "ServiceEnabled": true,
            "MaxConcurrentSessions": 4,
            "ConnectTypesSupported": [
                "SSH"
            ]
        },
        "Actions": {
            "#Manager.Reset": {
                "target": "/redfish/v1/Managers/bmc/Actions/Manager.Reset",
                "@Redfish.ActionInfo": "/redfish/v1/Managers/bmc/ResetActionInfo"
            }
        }
    });

    Ok(Json(response))
}

/// POST /redfish/v1/Managers/{manager_id}/Actions/Manager.Reset
///
/// Performs a reset action on the manager
pub async fn reset_manager(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    debug!("POST /redfish/v1/Managers/{}/Actions/Manager.Reset", manager_id);

    if manager_id != "bmc" {
        return Err(StatusCode::NOT_FOUND);
    }

    // Extract reset type from payload
    let reset_type = payload.get("ResetType")
        .and_then(|v| v.as_str())
        .unwrap_or("GracefulRestart");

    debug!("Manager reset type requested: {}", reset_type);

    // Validate reset type
    match reset_type {
        "GracefulRestart" | "ForceRestart" => {
            // TODO: Implement actual BMC reset via DBus
            warn!("Manager reset action not yet implemented, would perform: {}", reset_type);
            Ok(StatusCode::NO_CONTENT)
        }
        _ => {
            warn!("Invalid manager reset type: {}", reset_type);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_managers_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_managers_collection(State(state)).await;
        assert!(result.is_ok());
        
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ManagerCollection.ManagerCollection");
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_manager() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_manager(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Manager.v1_19_0.Manager");
        assert_eq!(json["Id"], "bmc");
    }

    #[tokio::test]
    async fn test_get_manager_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_manager(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
