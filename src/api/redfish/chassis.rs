//! Redfish Chassis and ChassisCollection endpoints
//!
//! Implements:
//! - GET /redfish/v1/Chassis
//! - GET /redfish/v1/Chassis/{chassis_id}
//! - GET /redfish/v1/Chassis/{chassis_id}/Power
//! - GET /redfish/v1/Chassis/{chassis_id}/Thermal
//! - GET /redfish/v1/Chassis/{chassis_id}/Sensors
//! - GET /redfish/v1/Chassis/{chassis_id}/NetworkAdapters
//!
//! Reference: DMTF Redfish Chassis schema v1.23.0, Power schema v1.7.2,
//! Thermal schema v1.8.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Return `StatusCode::NOT_FOUND` if `chassis_id` is not "chassis".
///
/// In upstream bmcweb the chassis ID is determined by querying DBus
/// `xyz.openbmc_project.Inventory.Manager.GetManagedObjects`.  Until the DBus
/// integration is complete we hard-code the single chassis name "chassis" which
/// is the conventional name on IBM OpenBMC systems.
fn validate_chassis_id(chassis_id: &str) -> Result<(), StatusCode> {
    if chassis_id == "chassis" {
        Ok(())
    } else {
        warn!("Chassis '{}' not found", chassis_id);
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Chassis collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis
pub async fn get_chassis_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis");

    Ok(Json(json!({
        "@odata.type": "#ChassisCollection.ChassisCollection",
        "@odata.id": "/redfish/v1/Chassis",
        "Name": "Chassis Collection",
        "Members@odata.count": 1,
        "Members": [{ "@odata.id": "/redfish/v1/Chassis/chassis" }]
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}
pub async fn get_chassis(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}", chassis_id);
    validate_chassis_id(&chassis_id)?;

    // TODO: Query DBus xyz.openbmc_project.Inventory.Item.Chassis for live data
    Ok(Json(json!({
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
            "Health": "OK",
            "HealthRollup": "OK"
        },
        "IndicatorLED": "Off",
        "PowerState": "On",
        "Links": {
            "ComputerSystems": [{ "@odata.id": "/redfish/v1/Systems/system" }],
            "ManagedBy": [{ "@odata.id": "/redfish/v1/Managers/bmc" }]
        },
        "Power": { "@odata.id": "/redfish/v1/Chassis/chassis/Power" },
        "Thermal": { "@odata.id": "/redfish/v1/Chassis/chassis/Thermal" },
        "Sensors": { "@odata.id": "/redfish/v1/Chassis/chassis/Sensors" },
        "NetworkAdapters": { "@odata.id": "/redfish/v1/Chassis/chassis/NetworkAdapters" },
        "PCIeDevices": { "@odata.id": "/redfish/v1/Chassis/chassis/PCIeDevices" },
        "Assembly": { "@odata.id": "/redfish/v1/Chassis/chassis/Assembly" }
    })))
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Power
///
/// Exposes PSUs, power controls, and voltage sensors.
///
/// Reference: Redfish Power schema v1.7.2.
/// On OpenBMC, power data comes from:
///   - `xyz.openbmc_project.Sensor.Value` on sensor objects
///   - `xyz.openbmc_project.State.Chassis` for chassis power state
///   - `xyz.openbmc_project.Inventory.Item.PowerSupply` for PSU inventory
pub async fn get_chassis_power(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Power", chassis_id);
    validate_chassis_id(&chassis_id)?;

    // TODO: Enumerate power supplies and voltage sensors from DBus
    Ok(Json(json!({
        "@odata.type": "#Power.v1_7_2.Power",
        "@odata.id": "/redfish/v1/Chassis/chassis/Power",
        "Id": "Power",
        "Name": "Power",
        "PowerControl": [
            {
                "MemberId": "0",
                "Name": "Chassis Power Control",
                "PowerConsumedWatts": null,
                "PowerCapacityWatts": null,
                "PowerLimit": {
                    "LimitInWatts": null,
                    "LimitException": "NoAction"
                },
                "Status": { "State": "Enabled", "Health": "OK" },
                "RelatedItem": [{ "@odata.id": "/redfish/v1/Chassis/chassis" }]
            }
        ],
        "Voltages": [],
        "PowerSupplies": []
    })))
}

// ---------------------------------------------------------------------------
// Thermal
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Thermal
///
/// Exposes temperatures and fans.
///
/// On OpenBMC, temperature/fan data comes from sensor objects with
/// `xyz.openbmc_project.Sensor.Value` and fan inventory items with
/// `xyz.openbmc_project.Inventory.Item.Fan`.
pub async fn get_chassis_thermal(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Thermal", chassis_id);
    validate_chassis_id(&chassis_id)?;

    // TODO: Enumerate temperature sensors and fans from DBus
    Ok(Json(json!({
        "@odata.type": "#Thermal.v1_8_0.Thermal",
        "@odata.id": "/redfish/v1/Chassis/chassis/Thermal",
        "Id": "Thermal",
        "Name": "Thermal",
        "Temperatures": [],
        "Fans": []
    })))
}

// ---------------------------------------------------------------------------
// Sensors collection (Redfish v1.7+)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Sensors
///
/// Returns the SensorCollection for this chassis.  On OpenBMC every sensor
/// object under `/xyz/openbmc_project/sensors/` with a Value interface
/// is enumerated here.
pub async fn get_chassis_sensors(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Sensors", chassis_id);
    validate_chassis_id(&chassis_id)?;

    // TODO: Enumerate sensor objects from DBus GetManagedObjects
    Ok(Json(json!({
        "@odata.type": "#SensorCollection.SensorCollection",
        "@odata.id": "/redfish/v1/Chassis/chassis/Sensors",
        "Name": "Chassis Sensor Collection",
        "Description": "Collection of all sensors on this chassis",
        "Members@odata.count": 0,
        "Members": []
    })))
}

// ---------------------------------------------------------------------------
// Network Adapters
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/NetworkAdapters
///
/// Returns network adapter inventory.  On OpenBMC this is backed by
/// `xyz.openbmc_project.Inventory.Item.NetworkInterface`.
pub async fn get_chassis_network_adapters(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/NetworkAdapters", chassis_id);
    validate_chassis_id(&chassis_id)?;

    // TODO: Enumerate network adapter inventory from DBus
    Ok(Json(json!({
        "@odata.type": "#NetworkAdapterCollection.NetworkAdapterCollection",
        "@odata.id": "/redfish/v1/Chassis/chassis/NetworkAdapters",
        "Name": "Network Adapter Collection",
        "Members@odata.count": 0,
        "Members": []
    })))
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
        // Verify sub-resource links are present
        assert!(json["Power"]["@odata.id"].is_string());
        assert!(json["Thermal"]["@odata.id"].is_string());
        assert!(json["Sensors"]["@odata.id"].is_string());
    }

    #[tokio::test]
    async fn test_get_chassis_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_chassis_power() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_power(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Power.v1_7_2.Power");
    }

    #[tokio::test]
    async fn test_get_chassis_thermal() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_thermal(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Thermal.v1_8_0.Thermal");
    }

    #[tokio::test]
    async fn test_get_chassis_sensors() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_sensors(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }
}
