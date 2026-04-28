//! Redfish Chassis and ChassisCollection endpoints
//!
//! Implements /redfish/v1/Chassis and /redfish/v1/Chassis/{ChassisId}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;

/// GET /redfish/v1/Chassis
///
/// Returns the ChassisCollection
pub async fn get_chassis_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis");

    let response = json!({
        "@odata.type": "#ChassisCollection.ChassisCollection",
        "@odata.id": "/redfish/v1/Chassis",
        "Name": "Chassis Collection",
        "Members@odata.count": 1,
        "Members": [
            {
                "@odata.id": "/redfish/v1/Chassis/chassis"
            }
        ]
    });

    Ok(Json(response))
}

/// GET /redfish/v1/Chassis/{chassis_id}
///
/// Returns a specific Chassis resource
pub async fn get_chassis(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}", chassis_id);

    // For now, only support "chassis" as the chassis ID
    if chassis_id != "chassis" {
        warn!("Chassis ID '{}' not found", chassis_id);
        return Err(StatusCode::NOT_FOUND);
    }

    // TODO: Query actual chassis information from DBus
    let response = json!({
        "@odata.type": "#Chassis.v1_23_0.Chassis",
        "@odata.id": "/redfish/v1/Chassis/chassis",
        "Id": "chassis",
        "Name": "Chassis",
        "ChassisType": "RackMount",
        "Manufacturer": "OpenBMC",
        "Model": "Unknown",
        "SerialNumber": "Unknown",
        "PartNumber": "Unknown",
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "IndicatorLED": "Off",
        "PowerState": "On",
        "Links": {
            "ComputerSystems": [
                {
                    "@odata.id": "/redfish/v1/Systems/system"
                }
            ],
            "ManagedBy": [
                {
                    "@odata.id": "/redfish/v1/Managers/bmc"
                }
            ]
        },
        "Power": {
            "@odata.id": "/redfish/v1/Chassis/chassis/Power"
        },
        "Thermal": {
            "@odata.id": "/redfish/v1/Chassis/chassis/Thermal"
        },
        "Sensors": {
            "@odata.id": "/redfish/v1/Chassis/chassis/Sensors"
        },
        "NetworkAdapters": {
            "@odata.id": "/redfish/v1/Chassis/chassis/NetworkAdapters"
        },
        "PCIeDevices": {
            "@odata.id": "/redfish/v1/Chassis/chassis/PCIeDevices"
        }
    });

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_chassis_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_chassis_collection(State(state)).await;
        assert!(result.is_ok());
        
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ChassisCollection.ChassisCollection");
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_chassis() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_chassis(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Chassis.v1_23_0.Chassis");
        assert_eq!(json["Id"], "chassis");
    }

    #[tokio::test]
    async fn test_get_chassis_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        
        let result = get_chassis(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
