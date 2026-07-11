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

use crate::dbus::{DbusClient, ZBusClient};
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
///
/// Enumerates chassis objects from DBus via `GetManagedObjects` on
/// `xyz.openbmc_project.Inventory.Manager`.  Falls back to a single
/// hard-coded "chassis" member when DBus is unavailable.
pub async fn get_chassis_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis");

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Inventory.Manager",
                "/xyz/openbmc_project/inventory",
            )
            .await
        {
            Ok(objects) => {
                let chassis_iface = "xyz.openbmc_project.Inventory.Item.Chassis";
                let mut chassis_ids: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(chassis_iface))
                    .map(|(path, _)| {
                        path.rsplit('/').next().unwrap_or("chassis").to_string()
                    })
                    .collect();
                // Stable ordering
                chassis_ids.sort();

                if chassis_ids.is_empty() {
                    // No chassis in inventory — fall back to default
                    vec![json!({ "@odata.id": "/redfish/v1/Chassis/chassis" })]
                } else {
                    chassis_ids
                        .iter()
                        .map(|id| {
                            json!({ "@odata.id": format!("/redfish/v1/Chassis/{}", id) })
                        })
                        .collect()
                }
            }
            Err(e) => {
                warn!("Failed to enumerate chassis from DBus: {}", e);
                vec![json!({ "@odata.id": "/redfish/v1/Chassis/chassis" })]
            }
        }
    } else {
        vec![json!({ "@odata.id": "/redfish/v1/Chassis/chassis" })]
    };

    Ok(Json(json!({
        "@odata.type": "#ChassisCollection.ChassisCollection",
        "@odata.id": "/redfish/v1/Chassis",
        "Name": "Chassis Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}
pub async fn get_chassis(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}", chassis_id);

    // Validate chassis_id against DBus inventory when available, else accept "chassis"
    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Inventory.Manager",
                "/xyz/openbmc_project/inventory",
            )
            .await
        {
            Ok(objects) => {
                let chassis_iface = "xyz.openbmc_project.Inventory.Item.Chassis";
                let exists = objects.iter().any(|(path, ifaces)| {
                    ifaces.contains_key(chassis_iface)
                        && path.rsplit('/').next() == Some(chassis_id.as_str())
                });
                if !exists {
                    // Also accept the default "chassis" id as a fallback
                    if chassis_id != "chassis" {
                        warn!("Chassis '{}' not found in DBus inventory", chassis_id);
                        return Err(StatusCode::NOT_FOUND);
                    }
                }
            }
            Err(e) => {
                warn!("DBus GetManagedObjects failed, falling back to local validation: {}", e);
                validate_chassis_id(&chassis_id)?;
            }
        }
    } else {
        validate_chassis_id(&chassis_id)?;
    }

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
/// Exposes PSUs, power controls, and voltage sensors enumerated from DBus.
///
/// On OpenBMC:
///   - PSUs: objects with `xyz.openbmc_project.Inventory.Item.PowerSupply`
///   - Voltage sensors: objects under `/xyz/openbmc_project/sensors/voltage/`
///     with interface `xyz.openbmc_project.Sensor.Value`
pub async fn get_chassis_power(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Power", chassis_id);
    validate_chassis_id(&chassis_id)?;

    let (voltages, power_supplies) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        // Try the sensor service first, then fall back to the inventory manager
        let objects_result = client
            .get_managed_objects(
                "xyz.openbmc_project.Sensor",
                "/xyz/openbmc_project/sensors",
            )
            .await;
        let objects_result = if objects_result.is_err() {
            client
                .get_managed_objects(
                    "xyz.openbmc_project.Inventory.Manager",
                    "/xyz/openbmc_project/inventory",
                )
                .await
        } else {
            objects_result
        };
        match objects_result {
            Ok(objects) => {
                // Enumerate voltage sensors
                let voltage_iface = "xyz.openbmc_project.Sensor.Value";
                let mut volt_sensors: Vec<Value> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        path.contains("/sensors/voltage/") && ifaces.contains_key(voltage_iface)
                    })
                    .enumerate()
                    .map(|(idx, (path, ifaces))| {
                        let props = &ifaces[voltage_iface];
                        let name = path.rsplit('/').next().unwrap_or("Voltage").to_string();
                        let reading = props.get("Value").and_then(|v| v.as_f64());
                        json!({
                            "MemberId": idx.to_string(),
                            "Name": name,
                            "ReadingVolts": reading,
                            "Status": { "State": "Enabled", "Health": "OK" }
                        })
                    })
                    .collect();
                volt_sensors.sort_by_key(|v| v["MemberId"].as_str().unwrap_or("").to_string());

                // Enumerate PSUs
                let psu_iface = "xyz.openbmc_project.Inventory.Item.PowerSupply";
                let mut psus: Vec<Value> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(psu_iface))
                    .enumerate()
                    .map(|(idx, (path, _))| {
                        let name = path.rsplit('/').next().unwrap_or("PSU").to_string();
                        json!({
                            "MemberId": idx.to_string(),
                            "Name": name,
                            "Status": { "State": "Enabled", "Health": "OK" },
                            "PowerSupplyType": "Unknown"
                        })
                    })
                    .collect();
                psus.sort_by_key(|v| v["MemberId"].as_str().unwrap_or("").to_string());

                (volt_sensors, psus)
            }
            Err(e) => {
                warn!("Failed to enumerate power data from DBus: {}", e);
                (vec![], vec![])
            }
        }
    } else {
        (vec![], vec![])
    };

    Ok(Json(json!({
        "@odata.type": "#Power.v1_7_2.Power",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Power", chassis_id),
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
                "RelatedItem": [{ "@odata.id": format!("/redfish/v1/Chassis/{}", chassis_id) }]
            }
        ],
        "Voltages": voltages,
        "PowerSupplies": power_supplies
    })))
}

// ---------------------------------------------------------------------------
// Thermal
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Thermal
///
/// Exposes temperatures and fans enumerated from DBus.
///
/// On OpenBMC:
///   - Temperature sensors: objects under `/xyz/openbmc_project/sensors/temperature/`
///     with `xyz.openbmc_project.Sensor.Value`
///   - Fan inventory: objects with `xyz.openbmc_project.Inventory.Item.Fan`
pub async fn get_chassis_thermal(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Thermal", chassis_id);
    validate_chassis_id(&chassis_id)?;

    let (temperatures, fans) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        // Use the sensor service for temperature readings
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Sensor",
                "/xyz/openbmc_project/sensors",
            )
            .await
        {
            Ok(objects) => {
                let temp_iface = "xyz.openbmc_project.Sensor.Value";

                // Temperature sensors
                let mut temps: Vec<Value> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        path.contains("/sensors/temperature/") && ifaces.contains_key(temp_iface)
                    })
                    .enumerate()
                    .map(|(idx, (path, ifaces))| {
                        let props = &ifaces[temp_iface];
                        let name = path.rsplit('/').next().unwrap_or("Temp").to_string();
                        let reading = props.get("Value").and_then(|v| v.as_f64());
                        let upper_warn = props.get("WarningHigh").and_then(|v| v.as_f64());
                        let upper_crit = props.get("CriticalHigh").and_then(|v| v.as_f64());
                        json!({
                            "MemberId": idx.to_string(),
                            "Name": name,
                            "ReadingCelsius": reading,
                            "UpperThresholdNonCritical": upper_warn,
                            "UpperThresholdCritical": upper_crit,
                            "Status": { "State": "Enabled", "Health": "OK" }
                        })
                    })
                    .collect();
                temps.sort_by_key(|v| v["Name"].as_str().unwrap_or("").to_string());

                // Fan readings
                let fan_iface = "xyz.openbmc_project.Sensor.Value";
                let mut fan_list: Vec<Value> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        path.contains("/sensors/fan") && ifaces.contains_key(fan_iface)
                    })
                    .enumerate()
                    .map(|(idx, (path, ifaces))| {
                        let props = &ifaces[fan_iface];
                        let name = path.rsplit('/').next().unwrap_or("Fan").to_string();
                        let reading = props.get("Value").and_then(|v| v.as_u64());
                        json!({
                            "MemberId": idx.to_string(),
                            "Name": name,
                            "Reading": reading,
                            "ReadingUnits": "RPM",
                            "Status": { "State": "Enabled", "Health": "OK" }
                        })
                    })
                    .collect();
                fan_list.sort_by_key(|v| v["Name"].as_str().unwrap_or("").to_string());

                (temps, fan_list)
            }
            Err(e) => {
                warn!("Failed to enumerate thermal data from DBus: {}", e);
                (vec![], vec![])
            }
        }
    } else {
        (vec![], vec![])
    };

    Ok(Json(json!({
        "@odata.type": "#Thermal.v1_8_0.Thermal",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Thermal", chassis_id),
        "Id": "Thermal",
        "Name": "Thermal",
        "Temperatures": temperatures,
        "Fans": fans
    })))
}

// ---------------------------------------------------------------------------
// Sensors collection (Redfish v1.7+)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Sensors
///
/// Returns the SensorCollection for this chassis.  On OpenBMC every sensor
/// object under `/xyz/openbmc_project/sensors/` with a `Sensor.Value` interface
/// is enumerated here.
pub async fn get_chassis_sensors(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Sensors", chassis_id);
    validate_chassis_id(&chassis_id)?;

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Sensor",
                "/xyz/openbmc_project/sensors",
            )
            .await
        {
            Ok(objects) => {
                let sensor_iface = "xyz.openbmc_project.Sensor.Value";
                let mut sensors: Vec<Value> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(sensor_iface))
                    .map(|(path, _)| {
                        // Build a stable member ID from the last two path segments
                        // e.g. /xyz/.../sensors/temperature/ambient → temperature_ambient
                        let parts: Vec<&str> = path.rsplitn(3, '/').collect();
                        let id = if parts.len() >= 2 {
                            format!("{}_{}", parts[1], parts[0])
                        } else {
                            parts[0].to_string()
                        };
                        json!({
                            "@odata.id": format!(
                                "/redfish/v1/Chassis/{}/Sensors/{}",
                                chassis_id, id
                            )
                        })
                    })
                    .collect();
                sensors.sort_by_key(|v| v["@odata.id"].as_str().unwrap_or("").to_string());
                sensors
            }
            Err(e) => {
                warn!("Failed to enumerate sensors from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#SensorCollection.SensorCollection",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Sensors", chassis_id),
        "Name": "Chassis Sensor Collection",
        "Description": "Collection of all sensors on this chassis",
        "Members@odata.count": members.len(),
        "Members": members
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
