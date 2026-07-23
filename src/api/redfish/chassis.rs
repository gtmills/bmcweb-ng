//! Redfish Chassis and ChassisCollection endpoints
//!
//! Implements:
//! - GET   /redfish/v1/Chassis
//! - GET   /redfish/v1/Chassis/{chassis_id}
//! - PATCH /redfish/v1/Chassis/{chassis_id}
//! - GET   /redfish/v1/Chassis/{chassis_id}/Power
//! - GET   /redfish/v1/Chassis/{chassis_id}/Thermal
//! - GET   /redfish/v1/Chassis/{chassis_id}/Sensors
//! - GET   /redfish/v1/Chassis/{chassis_id}/NetworkAdapters
//!
//! Reference: DMTF Redfish Chassis schema v1.23.0, Power schema v1.7.2,
//! Thermal schema v1.8.0

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_PATCH};
use crate::auth::session::UserSession;
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

    // Read live chassis data from DBus inventory
    let (chassis_name, chassis_model, chassis_serial, chassis_part, chassis_led) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());
            let inv_path = format!("/xyz/openbmc_project/inventory/system/chassis/{}", chassis_id);
            let asset_iface = "xyz.openbmc_project.Inventory.Decorator.Asset";
            let led_iface = "xyz.openbmc_project.Led.Physical";

            let name = client
                .get_property(&inv_path, "xyz.openbmc_project.Inventory.Item", "PrettyName")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| chassis_id.clone());
            let model = client
                .get_property(&inv_path, asset_iface, "Model")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let serial = client
                .get_property(&inv_path, asset_iface, "SerialNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let part = client
                .get_property(&inv_path, asset_iface, "PartNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());

            // IndicatorLED from LED physical object
            // /xyz/openbmc_project/led/physical/front_id (or similar)
            let led_state_raw = client
                .get_property(
                    "/xyz/openbmc_project/led/physical/front_id",
                    led_iface,
                    "State",
                )
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            // IndicatorLED: check enclosure_identify_blink first (upstream led.hpp)
            // then fall back to enclosure_identify solid-on state.
            let blink_asserted = client
                .get_property(
                    "/xyz/openbmc_project/led/groups/enclosure_identify_blink",
                    "xyz.openbmc_project.Led.Group",
                    "Asserted",
                )
                .await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
            let solid_asserted = client
                .get_property(
                    "/xyz/openbmc_project/led/groups/enclosure_identify",
                    "xyz.openbmc_project.Led.Group",
                    "Asserted",
                )
                .await.ok().and_then(|v| v.as_bool()).unwrap_or(false);
            let led = if blink_asserted {
                "Blinking"
            } else if solid_asserted {
                "Lit"
            } else if led_state_raw.ends_with(".On") {
                "Lit"
            } else {
                "Off"
            };
            (name, model, serial, part, led.to_string())
        } else {
            (
                chassis_id.clone(),
                "Unknown".to_string(),
                "Unknown".to_string(),
                "Unknown".to_string(),
                "Off".to_string(),
            )
        };

    Ok(Json(json!({
        "@odata.type": "#Chassis.v1_23_0.Chassis",
        "@odata.id": format!("/redfish/v1/Chassis/{}", chassis_id),
        "Id": chassis_id,
        "Name": chassis_name,
        "ChassisType": "RackMount",
        "Manufacturer": "OpenBMC",
        "Model": chassis_model,
        "SerialNumber": chassis_serial,
        "PartNumber": chassis_part,
        "Status": {
            "State": "Enabled",
            "Health": "OK",
            "HealthRollup": "OK"
        },
        "IndicatorLED": chassis_led,
        "PowerState": "On",
        "Links": {
            "ComputerSystems": [{ "@odata.id": "/redfish/v1/Systems/system" }],
            "ManagedBy": [{ "@odata.id": "/redfish/v1/Managers/bmc" }]
        },
        "Power": { "@odata.id": format!("/redfish/v1/Chassis/{}/Power", chassis_id) },
        "Thermal": { "@odata.id": format!("/redfish/v1/Chassis/{}/Thermal", chassis_id) },
        "Sensors": { "@odata.id": format!("/redfish/v1/Chassis/{}/Sensors", chassis_id) },
        "NetworkAdapters": { "@odata.id": format!("/redfish/v1/Chassis/{}/NetworkAdapters", chassis_id) },
        "PCIeDevices": { "@odata.id": format!("/redfish/v1/Chassis/{}/PCIeDevices", chassis_id) },
        "Assembly": { "@odata.id": format!("/redfish/v1/Chassis/{}/Assembly", chassis_id) }
    })))
}

/// PATCH /redfish/v1/Chassis/{chassis_id}
///
/// Supports setting the IndicatorLED state.
///
/// OpenBMC DBus:
///   /xyz/openbmc_project/led/groups/front_id
///   interface: xyz.openbmc_project.Led.Group
///   property: Asserted (bool) — true = LED on
pub async fn patch_chassis(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(chassis_id): Path<String>,
    JsonBody(body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/Chassis/{}", chassis_id);
    check_privilege(Some(&session), PRIVILEGE_PATCH)?;
    validate_chassis_id(&chassis_id)?;

    if let Some(led_state) = body.get("IndicatorLED").and_then(|v| v.as_str()) {
        let asserted = led_state == "Blinking" || led_state == "Lit";
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());
            if let Err(e) = client
                .set_property(
                    "/xyz/openbmc_project/led/groups/front_id",
                    "xyz.openbmc_project.Led.Group",
                    "Asserted",
                    serde_json::json!(asserted),
                )
                .await
            {
                warn!("Failed to set LED state via DBus: {}", e);
            } else {
                use tracing::info;
                info!("IndicatorLED set to '{}' via DBus (Asserted={})", led_state, asserted);
            }
        }
    }

    get_chassis(State(state), Path(chassis_id)).await
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

    // Read total power consumption from DBus (chassis power sensor)
    // On OpenBMC: /xyz/openbmc_project/sensors/power/total_power
    //   interface: xyz.openbmc_project.Sensor.Value
    let total_power: Option<f64> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        client
            .get_property(
                "/xyz/openbmc_project/sensors/power/total_power",
                "xyz.openbmc_project.Sensor.Value",
                "Value",
            )
            .await
            .ok()
            .and_then(|v| v.as_f64())
    } else {
        None
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
                "PowerConsumedWatts": total_power,
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
fn sensor_path_to_id(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 2 {
        format!("{}_{}", parts[1], parts[0])
    } else {
        parts[0].to_string()
    }
}

async fn fetch_chassis_sensors(
    client: &dyn DbusClient,
    chassis_id: &str,
) -> Vec<Value> {
    let association_path = format!(
        "/xyz/openbmc_project/inventory/system/{}/all_sensors",
        chassis_id
    );
    match client
        .get_associated(
            &association_path,
            "/xyz/openbmc_project/sensors",
            0,
            &["xyz.openbmc_project.Sensor.Value"],
        )
        .await
    {
        Ok(objects) => {
            let mut sensors: Vec<Value> = objects
                .keys()
                .map(|path| {
                    let id = sensor_path_to_id(path);
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
            warn!("Failed to enumerate sensors via association for chassis '{}': {}", chassis_id, e);
            vec![]
        }
    }
}

pub async fn get_chassis_sensors(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Sensors", chassis_id);
    validate_chassis_id(&chassis_id)?;

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        fetch_chassis_sensors(&client, &chassis_id).await
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

/// GET /redfish/v1/Chassis/{chassis_id}/Sensors/{sensor_id}
///
/// Returns a single chassis sensor resource. Currently includes frequency
/// sensors in addition to the existing OpenBMC sensor namespaces.
pub async fn get_chassis_sensor(
    State(state): State<Arc<AppState>>,
    Path((chassis_id, sensor_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Sensors/{}", chassis_id, sensor_id);
    validate_chassis_id(&chassis_id)?;

    let conn = state.dbus_connection.as_deref().ok_or(StatusCode::NOT_FOUND)?;
    let client = ZBusClient::from_connection(conn.clone());
    let objects = client
        .get_managed_objects(
            "xyz.openbmc_project.Sensor",
            "/xyz/openbmc_project/sensors",
        )
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let Some((path, ifaces)) = objects.iter().find(|(path, ifaces)| {
        ifaces.contains_key("xyz.openbmc_project.Sensor.Value")
            && sensor_path_to_id(path) == sensor_id
    }) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let props = ifaces
        .get("xyz.openbmc_project.Sensor.Value")
        .ok_or(StatusCode::NOT_FOUND)?;
    let reading = props
        .get("Value")
        .and_then(|v| v.as_f64())
        .ok_or(StatusCode::NOT_FOUND)?;

    let (reading_type, reading_units) = if path.contains("/sensors/temperature/") {
        ("Temperature", "Cel")
    } else if path.contains("/sensors/voltage/") {
        ("Voltage", "V")
    } else if path.contains("/sensors/fan_tach/") {
        ("Rotational", "RPM")
    } else if path.contains("/sensors/power/") {
        ("Power", "W")
    } else if path.contains("/sensors/current/") {
        ("Current", "A")
    } else if path.contains("/sensors/frequency/") {
        ("Frequency", "Hz")
    } else {
        ("Other", "")
    };

    Ok(Json(json!({
        "@odata.type": "#Sensor.v1_9_0.Sensor",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Sensors/{}", chassis_id, sensor_id),
        "Id": sensor_id,
        "Name": sensor_id,
        "Reading": reading,
        "ReadingType": reading_type,
        "ReadingUnits": reading_units,
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

// ---------------------------------------------------------------------------
// Network Adapters
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/NetworkAdapters
///
/// Enumerates network adapters from DBus inventory.  Upstream bmcweb
/// (see `redfish-core/lib/network_adapter.hpp`) queries
/// `xyz.openbmc_project.Inventory.Item.NetworkAdapter` objects via
/// `GetManagedObjects` on `xyz.openbmc_project.Inventory.Manager`.
///
/// Each adapter object found under the chassis path is returned as a
/// collection member.  Falls back to an empty collection when DBus is
/// unavailable or no adapters are present.
pub async fn get_chassis_network_adapters(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/NetworkAdapters", chassis_id);
    validate_chassis_id(&chassis_id)?;

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
                let adapter_iface = "xyz.openbmc_project.Inventory.Item.NetworkAdapter";
                let chassis_prefix = format!(
                    "/xyz/openbmc_project/inventory/system/{}/",
                    chassis_id
                );
                let mut adapter_ids: Vec<String> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        ifaces.contains_key(adapter_iface)
                            && path.starts_with(&chassis_prefix)
                    })
                    .map(|(path, _)| {
                        path.rsplit('/').next().unwrap_or("adapter").to_string()
                    })
                    .collect();
                adapter_ids.sort();

                adapter_ids
                    .iter()
                    .map(|id| {
                        json!({
                            "@odata.id": format!(
                                "/redfish/v1/Chassis/{}/NetworkAdapters/{}",
                                chassis_id, id
                            )
                        })
                    })
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate NetworkAdapters from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#NetworkAdapterCollection.NetworkAdapterCollection",
        "@odata.id": format!("/redfish/v1/Chassis/{}/NetworkAdapters", chassis_id),
        "Name": "Network Adapter Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

// ---------------------------------------------------------------------------
// NetworkAdapter instance (TODO 4)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/NetworkAdapters/{adapter_id}
///
/// Returns a single NetworkAdapter resource.  Reads asset information
/// (Manufacturer, Model, PartNumber) from the DBus inventory decorator.
///
/// Reference: DMTF Redfish NetworkAdapter schema v1.11.0
/// Upstream: redfish-core/lib/network_adapter.hpp
pub async fn get_chassis_network_adapter(
    State(state): State<Arc<AppState>>,
    Path((chassis_id, adapter_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/NetworkAdapters/{}",
        chassis_id, adapter_id
    );
    validate_chassis_id(&chassis_id)?;

    let (manufacturer, model, part_number, serial_number) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());
            // DBus path convention: .../inventory/system/<chassis_id>/<adapter_id>
            let inv_path = format!(
                "/xyz/openbmc_project/inventory/system/{}/{}",
                chassis_id, adapter_id
            );
            let asset_iface = "xyz.openbmc_project.Inventory.Decorator.Asset";

            let manufacturer = client
                .get_property(&inv_path, asset_iface, "Manufacturer")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let model = client
                .get_property(&inv_path, asset_iface, "Model")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let part_number = client
                .get_property(&inv_path, asset_iface, "PartNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let serial_number = client
                .get_property(&inv_path, asset_iface, "SerialNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());

            (manufacturer, model, part_number, serial_number)
        } else {
            (
                "Unknown".to_string(),
                "Unknown".to_string(),
                "Unknown".to_string(),
                "Unknown".to_string(),
            )
        };

    Ok(Json(json!({
        "@odata.type": "#NetworkAdapter.v1_11_0.NetworkAdapter",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/NetworkAdapters/{}",
            chassis_id, adapter_id
        ),
        "Id": adapter_id,
        "Name": adapter_id,
        "Description": "Network Adapter",
        "Status": { "State": "Enabled", "Health": "OK" },
        "Manufacturer": manufacturer,
        "Model": model,
        "PartNumber": part_number,
        "SerialNumber": serial_number,
        "NetworkPorts": {
            "@odata.id": format!(
                "/redfish/v1/Chassis/{}/NetworkAdapters/{}/NetworkPorts",
                chassis_id, adapter_id
            )
        },
        "NetworkDeviceFunctions": {
            "@odata.id": format!(
                "/redfish/v1/Chassis/{}/NetworkAdapters/{}/NetworkDeviceFunctions",
                chassis_id, adapter_id
            )
        }
    })))
}

// ---------------------------------------------------------------------------
// Chassis Drives (TODO 8)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Drives
///
/// Returns the collection of drives associated with this chassis.
///
/// Reference: DMTF Redfish DriveCollection schema
/// Upstream: redfish-core/lib/storage_chassis.hpp
///
/// On OpenBMC, drives are inventory objects under the chassis path with
/// interface xyz.openbmc_project.Inventory.Item.Drive.
pub async fn get_chassis_drives(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Drives", chassis_id);
    validate_chassis_id(&chassis_id)?;

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
                let drive_iface = "xyz.openbmc_project.Inventory.Item.Drive";
                let chassis_prefix = format!(
                    "/xyz/openbmc_project/inventory/system/{}",
                    chassis_id
                );
                let mut drives: Vec<String> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        ifaces.contains_key(drive_iface)
                            && path.starts_with(&chassis_prefix)
                    })
                    .filter_map(|(path, _)| {
                        path.rsplit('/').next().map(|s| s.to_string())
                    })
                    .collect();
                drives.sort();

                drives
                    .iter()
                    .map(|id| {
                        json!({
                            "@odata.id": format!(
                                "/redfish/v1/Chassis/{}/Drives/{}",
                                chassis_id, id
                            )
                        })
                    })
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate Drives from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#DriveCollection.DriveCollection",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Drives", chassis_id),
        "Name": "Drive Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}/Drives/{drive_id}
///
/// Returns a single Drive resource from chassis inventory.
///
/// Reference: DMTF Redfish Drive schema v1.18.0
/// Upstream: redfish-core/lib/storage_chassis.hpp
pub async fn get_chassis_drive(
    State(state): State<Arc<AppState>>,
    Path((chassis_id, drive_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/Drives/{}", chassis_id, drive_id);
    validate_chassis_id(&chassis_id)?;

    let (model, serial, part_number, capacity_bytes, media_type, protocol) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());
            let inv_path = format!(
                "/xyz/openbmc_project/inventory/system/{}/{}",
                chassis_id, drive_id
            );
            let asset_iface = "xyz.openbmc_project.Inventory.Decorator.Asset";
            let drive_iface = "xyz.openbmc_project.Inventory.Item.Drive";

            // Verify the drive exists in DBus
            let exists = client
                .get_all_properties(&inv_path, drive_iface)
                .await
                .is_ok();
            if !exists {
                return Err(StatusCode::NOT_FOUND);
            }

            let model = client.get_property(&inv_path, asset_iface, "Model")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let serial = client.get_property(&inv_path, asset_iface, "SerialNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());
            let part_number = client.get_property(&inv_path, asset_iface, "PartNumber")
                .await.ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unknown".to_string());

            let props = client.get_all_properties(&inv_path, drive_iface).await
                .unwrap_or_default();
            let capacity = props.get("Capacity")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let drive_type_raw = props.get("DriveType")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            let protocol_raw = props.get("Protocol")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            // Map DriveType to Redfish MediaType
            let media = if drive_type_raw.ends_with(".SSD") {
                "SSD"
            } else if drive_type_raw.ends_with(".HDD") {
                "HDD"
            } else {
                "SSD"
            };
            // Map Protocol
            let proto = if protocol_raw.ends_with(".NVMe") {
                "NVMe"
            } else if protocol_raw.ends_with(".SATA") {
                "SATA"
            } else if protocol_raw.ends_with(".SAS") {
                "SAS"
            } else {
                "NVMe"
            };

            (model, serial, part_number, capacity, media.to_string(), proto.to_string())
        } else {
            return Err(StatusCode::NOT_FOUND);
        };

    Ok(Json(json!({
        "@odata.type": "#Drive.v1_18_0.Drive",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Drives/{}", chassis_id, drive_id),
        "Id": drive_id,
        "Name": drive_id,
        "Description": "Drive",
        "Status": { "State": "Enabled", "Health": "OK" },
        "Model": model,
        "SerialNumber": serial,
        "PartNumber": part_number,
        "CapacityBytes": capacity_bytes,
        "MediaType": media_type,
        "Protocol": protocol
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

    #[tokio::test]
    async fn test_fetch_chassis_sensors_via_association() {
        use crate::dbus::MockDbusClient;
        use std::collections::HashMap;

        let mock = MockDbusClient::new();

        let mut tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        for name in &["ambient", "cpu0"] {
            let mut svc = HashMap::new();
            svc.insert(
                "xyz.openbmc_project.Sensor.Manager".to_string(),
                vec!["xyz.openbmc_project.Sensor.Value".to_string()],
            );
            tree.insert(
                format!("/xyz/openbmc_project/sensors/temperature/{}", name),
                svc,
            );
        }
        mock.set_mock_subtree(
            "/xyz/openbmc_project/inventory/system/chassis/all_sensors",
            tree,
        );

        let members = fetch_chassis_sensors(&mock, "chassis").await;

        assert_eq!(members.len(), 2);
        let ids: Vec<&str> = members
            .iter()
            .map(|v| v["@odata.id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"/redfish/v1/Chassis/chassis/Sensors/temperature_ambient"));
        assert!(ids.contains(&"/redfish/v1/Chassis/chassis/Sensors/temperature_cpu0"));
    }

    #[tokio::test]
    async fn test_get_chassis_network_adapters_no_dbus() {
        // Without a DBus connection, the collection should return empty members
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_network_adapters(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(
            json["@odata.type"],
            "#NetworkAdapterCollection.NetworkAdapterCollection"
        );
        assert_eq!(json["Members@odata.count"], 0);
        assert_eq!(
            json["@odata.id"].as_str().unwrap(),
            "/redfish/v1/Chassis/chassis/NetworkAdapters"
        );
    }

    #[tokio::test]
    async fn test_get_chassis_network_adapters_not_found() {
        // Invalid chassis ID should return 404
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result =
            get_chassis_network_adapters(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/Assembly
///
/// Returns the Assembly resource for a chassis.
/// On OpenBMC, assembly/FRU data lives at xyz.openbmc_project.Inventory.Decorator.Asset
/// on the chassis inventory object.
pub async fn get_chassis_assembly(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    use crate::dbus::{DbusClient, ZBusClient};

    // Validate chassis id
    let chassis_path = format!("/xyz/openbmc_project/inventory/system/chassis/{}", chassis_id);

    let assemblies: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        // Each sub-component under the chassis path that has Inventory.Item is an assembly member
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Inventory.Manager",
                "/xyz/openbmc_project/inventory",
            )
            .await
        {
            Ok(objects) => {
                let asset_iface = "xyz.openbmc_project.Inventory.Decorator.Asset";
                let item_iface  = "xyz.openbmc_project.Inventory.Item";
                let mut idx = 0u32;
                objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        path.starts_with(&chassis_path)
                            && ifaces.contains_key(item_iface)
                    })
                    .map(|(path, ifaces)| {
                        let name = path.rsplit('/').next().unwrap_or("unknown").to_string();
                        let asset = ifaces.get(asset_iface);
                        let serial = asset
                            .and_then(|a| a.get("SerialNumber"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("").to_string();
                        let part_number = asset
                            .and_then(|a| a.get("PartNumber"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("").to_string();
                        let model = asset
                            .and_then(|a| a.get("Model"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("").to_string();
                        idx += 1;
                        json!({
                            "MemberId": idx.to_string(),
                            "Name": name,
                            "SerialNumber": serial,
                            "PartNumber": part_number,
                            "Model": model,
                            "Status": { "State": "Enabled", "Health": "OK" }
                        })
                    })
                    .collect()
            }
            Err(_) => vec![],
        }
    } else {
        // No DBus — return empty assembly
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#Assembly.v1_5_0.Assembly",
        "@odata.id": format!("/redfish/v1/Chassis/{}/Assembly", chassis_id),
        "Id": "Assembly",
        "Name": "Assembly",
        "Assemblies": assemblies,
        "Assemblies@odata.count": assemblies.len()
    })))
}

// ---------------------------------------------------------------------------
// PowerSubsystem
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/PowerSubsystem
///
/// Returns the PowerSubsystem resource for this chassis.
///
/// Reference: DMTF Redfish PowerSubsystem schema v1.1.0
/// Upstream: redfish-core/lib/power_subsystem.hpp
///
/// The PowerSubsystem is the newer replacement for the legacy Power resource.
/// It provides links to the PowerSupplies sub-collection.
pub async fn get_chassis_power_subsystem(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/PowerSubsystem", chassis_id);
    validate_chassis_id(&chassis_id)?;

    Ok(Json(json!({
        "@odata.type": "#PowerSubsystem.v1_1_0.PowerSubsystem",
        "@odata.id": format!("/redfish/v1/Chassis/{}/PowerSubsystem", chassis_id),
        "Id": "PowerSubsystem",
        "Name": "Power Subsystem",
        "Description": "Chassis Power Subsystem",
        "Status": { "State": "Enabled", "Health": "OK" },
        "PowerSupplies": {
            "@odata.id": format!(
                "/redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies",
                chassis_id
            )
        }
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}/PowerSubsystem/PowerSupplies
///
/// Returns the PowerSupply collection for this chassis.
///
/// On OpenBMC, power supply objects have interface
/// `xyz.openbmc_project.Inventory.Item.PowerSupply` in the inventory tree.
pub async fn get_chassis_power_supplies(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies",
        chassis_id
    );
    validate_chassis_id(&chassis_id)?;

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
                let psu_iface = "xyz.openbmc_project.Inventory.Item.PowerSupply";
                let mut psus: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(psu_iface))
                    .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                    .collect();
                psus.sort();
                psus.iter()
                    .map(|id| json!({
                        "@odata.id": format!(
                            "/redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies/{}",
                            chassis_id, id
                        )
                    }))
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate PSUs from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#PowerSupplyCollection.PowerSupplyCollection",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies",
            chassis_id
        ),
        "Name": "Power Supply Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

// ---------------------------------------------------------------------------
// ThermalSubsystem + Fans
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/ThermalSubsystem
///
/// Returns the ThermalSubsystem resource for this chassis.
///
/// Reference: DMTF Redfish ThermalSubsystem schema v1_3_0
/// Upstream: redfish-core/lib/thermal_subsystem.hpp
///
/// The ThermalSubsystem is the newer replacement for the legacy Thermal resource.
/// It provides links to the Fans sub-collection and ThermalMetrics.
pub async fn get_chassis_thermal_subsystem(
    State(_state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/ThermalSubsystem",
        chassis_id
    );
    validate_chassis_id(&chassis_id)?;

    Ok(Json(json!({
        "@odata.type": "#ThermalSubsystem.v1_3_0.ThermalSubsystem",
        "@odata.id": format!("/redfish/v1/Chassis/{}/ThermalSubsystem", chassis_id),
        "Id": "ThermalSubsystem",
        "Name": "Thermal Subsystem",
        "Description": "Chassis Thermal Subsystem",
        "Status": { "State": "Enabled", "Health": "OK" },
        "Fans": {
            "@odata.id": format!(
                "/redfish/v1/Chassis/{}/ThermalSubsystem/Fans",
                chassis_id
            )
        },
        "ThermalMetrics": {
            "@odata.id": format!(
                "/redfish/v1/Chassis/{}/ThermalSubsystem/ThermalMetrics",
                chassis_id
            )
        }
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}/ThermalSubsystem/Fans
///
/// Returns the Fan collection for this chassis.
///
/// Reference: DMTF Redfish FanCollection schema
/// Upstream: redfish-core/lib/fan.hpp
///
/// On OpenBMC, fan sensor objects live under `/xyz/openbmc_project/sensors/fan_tach/`
/// with interface `xyz.openbmc_project.Sensor.Value`.
pub async fn get_chassis_fans(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/ThermalSubsystem/Fans",
        chassis_id
    );
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
                let mut fan_ids: Vec<String> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        (path.contains("/sensors/fan_tach/") || path.contains("/sensors/fan/"))
                            && ifaces.contains_key(sensor_iface)
                    })
                    .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                    .collect();
                fan_ids.sort();
                fan_ids.dedup();
                fan_ids.iter()
                    .map(|id| json!({
                        "@odata.id": format!(
                            "/redfish/v1/Chassis/{}/ThermalSubsystem/Fans/{}",
                            chassis_id, id
                        )
                    }))
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate fans from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#FanCollection.FanCollection",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/ThermalSubsystem/Fans",
            chassis_id
        ),
        "Name": "Fan Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Chassis/{chassis_id}/ThermalSubsystem/Fans/{fan_id}
///
/// Returns a single Fan resource.
///
/// Reference: DMTF Redfish Fan schema v1_5_0
/// Upstream: redfish-core/lib/fan.hpp
pub async fn get_chassis_fan(
    State(state): State<Arc<AppState>>,
    Path((chassis_id, fan_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/ThermalSubsystem/Fans/{}",
        chassis_id, fan_id
    );
    validate_chassis_id(&chassis_id)?;

    let (speed_rpm, status_health) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        let sensor_iface = "xyz.openbmc_project.Sensor.Value";

        // Try fan_tach first, then fan
        let paths = [
            format!("/xyz/openbmc_project/sensors/fan_tach/{}", fan_id),
            format!("/xyz/openbmc_project/sensors/fan/{}", fan_id),
        ];
        let mut rpm = None;
        let mut health = "OK";
        for path in &paths {
            if let Ok(props) = client.get_all_properties(path, sensor_iface).await {
                rpm = props.get("Value").and_then(|v| v.as_f64());
                // Check for alarm condition
                if props.get("WarningAlarmHigh").and_then(|v| v.as_bool()).unwrap_or(false)
                    || props.get("WarningAlarmLow").and_then(|v| v.as_bool()).unwrap_or(false)
                {
                    health = "Warning";
                }
                if props.get("CriticalAlarmHigh").and_then(|v| v.as_bool()).unwrap_or(false)
                    || props.get("CriticalAlarmLow").and_then(|v| v.as_bool()).unwrap_or(false)
                {
                    health = "Critical";
                }
                break;
            }
        }
        (rpm, health.to_string())
    } else {
        (None, "OK".to_string())
    };

    Ok(Json(json!({
        "@odata.type": "#Fan.v1_5_0.Fan",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/ThermalSubsystem/Fans/{}",
            chassis_id, fan_id
        ),
        "Id": fan_id,
        "Name": fan_id,
        "SpeedPercent": {
            "DataSourceUri": format!(
                "/redfish/v1/Chassis/{}/Sensors/fan_tach_{}",
                chassis_id, fan_id
            ),
            "Reading": speed_rpm
        },
        "Status": { "State": "Enabled", "Health": status_health }
    })))
}

// ---------------------------------------------------------------------------
// Cables
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Cables
///
/// Returns the Cable collection.
///
/// Reference: DMTF Redfish CableCollection schema
/// Upstream: redfish-core/lib/cable.hpp
///
/// On OpenBMC, cable inventory objects have interface
/// `xyz.openbmc_project.Inventory.Item.Cable` in the inventory tree.
pub async fn get_cables_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Cables");

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
                let cable_iface = "xyz.openbmc_project.Inventory.Item.Cable";
                let mut cable_ids: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(cable_iface))
                    .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                    .collect();
                cable_ids.sort();
                cable_ids.iter()
                    .map(|id| json!({
                        "@odata.id": format!("/redfish/v1/Cables/{}", id)
                    }))
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate cables from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#CableCollection.CableCollection",
        "@odata.id": "/redfish/v1/Cables",
        "Name": "Cable Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Cables/{cable_id}
///
/// Returns a single Cable resource.
///
/// Reference: DMTF Redfish Cable schema v1_2_0
/// Upstream: redfish-core/lib/cable.hpp
///
/// On OpenBMC, cable objects have:
///   interface: xyz.openbmc_project.Inventory.Item.Cable
///   properties: CableTypeDescription, CableStatus, Length
pub async fn get_cable(
    State(state): State<Arc<AppState>>,
    Path(cable_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Cables/{}", cable_id);

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        if let Ok(objects) = client
            .get_managed_objects(
                "xyz.openbmc_project.Inventory.Manager",
                "/xyz/openbmc_project/inventory",
            )
            .await
        {
            let cable_iface = "xyz.openbmc_project.Inventory.Item.Cable";
            for (path, ifaces) in &objects {
                let id = path.rsplit('/').next().unwrap_or("");
                if id == cable_id && ifaces.contains_key(cable_iface) {
                    let props = &ifaces[cable_iface];
                    let cable_type_desc = props
                        .get("CableTypeDescription")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let cable_status = props
                        .get("CableStatus")
                        .and_then(|v| v.as_str())
                        .and_then(|s| {
                            if s.ends_with(".Active") { Some("Active") }
                            else if s.ends_with(".Inactive") { Some("Inactive") }
                            else { None }
                        })
                        .unwrap_or("Normal")
                        .to_string();
                    let length = props
                        .get("Length")
                        .and_then(|v| v.as_f64());
                    return Ok(Json(json!({
                        "@odata.type": "#Cable.v1_2_0.Cable",
                        "@odata.id": format!("/redfish/v1/Cables/{}", cable_id),
                        "Id": cable_id,
                        "Name": format!("Cable {}", cable_id),
                        "CableTypeDescription": cable_type_desc,
                        "CableStatus": cable_status,
                        "LengthMeters": length,
                        "Status": { "State": "Enabled", "Health": "OK" }
                    })));
                }
            }
        }
    }
    Err(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod chassis_new_tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_chassis_power_subsystem() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_power_subsystem(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#PowerSubsystem.v1_1_0.PowerSubsystem");
        assert_eq!(json["Id"], "PowerSubsystem");
        assert!(json["PowerSupplies"]["@odata.id"].is_string());
    }

    #[tokio::test]
    async fn test_get_chassis_thermal_subsystem() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_thermal_subsystem(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ThermalSubsystem.v1_3_0.ThermalSubsystem");
        assert_eq!(json["Id"], "ThermalSubsystem");
        assert!(json["Fans"]["@odata.id"].is_string());
    }

    #[tokio::test]
    async fn test_get_chassis_fans_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_fans(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_cables_collection_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_cables_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#CableCollection.CableCollection");
        assert_eq!(json["Members@odata.count"], 0);
    }
}

// ---------------------------------------------------------------------------
// PowerSupply instance (TODO 2)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/PowerSubsystem/PowerSupplies/{psu_id}
///
/// Returns a single PowerSupply resource.
///
/// Reference: DMTF Redfish PowerSupply schema v1_6_0
/// Upstream: redfish-core/lib/power_supply.hpp
///
/// On OpenBMC, power supply objects are at inventory paths with interface
/// `xyz.openbmc_project.Inventory.Item.PowerSupply`.
/// Asset and state properties come from Decorator.Asset and Inventory.Item.
pub async fn get_chassis_power_supply(
    State(state): State<Arc<AppState>>,
    Path((chassis_id, psu_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies/{}",
        chassis_id, psu_id
    );
    validate_chassis_id(&chassis_id)?;

    let (manufacturer, model, serial, part_number, present, health) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());
            if let Ok(objects) = client
                .get_managed_objects(
                    "xyz.openbmc_project.Inventory.Manager",
                    "/xyz/openbmc_project/inventory",
                )
                .await
            {
                let psu_iface = "xyz.openbmc_project.Inventory.Item.PowerSupply";
                let asset_iface = "xyz.openbmc_project.Inventory.Decorator.Asset";
                let item_iface = "xyz.openbmc_project.Inventory.Item";

                let found = objects.iter().find(|(path, ifaces)| {
                    ifaces.contains_key(psu_iface)
                        && path.rsplit('/').next() == Some(psu_id.as_str())
                });

                match found {
                    Some((_, ifaces)) => {
                        let asset = ifaces.get(asset_iface);
                        let item = ifaces.get(item_iface);

                        let mfr = asset.and_then(|a| a.get("Manufacturer"))
                            .and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let mdl = asset.and_then(|a| a.get("Model"))
                            .and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                        let sn = asset.and_then(|a| a.get("SerialNumber"))
                            .and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let pn = asset.and_then(|a| a.get("PartNumber"))
                            .and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let is_present = item.and_then(|i| i.get("Present"))
                            .and_then(|v| v.as_bool()).unwrap_or(true);
                        let hlth = if is_present { "OK" } else { "Critical" };
                        (mfr, mdl, sn, pn, is_present, hlth.to_string())
                    }
                    None => return Err(StatusCode::NOT_FOUND),
                }
            } else {
                return Err(StatusCode::NOT_FOUND);
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        };

    Ok(Json(json!({
        "@odata.type": "#PowerSupply.v1_6_0.PowerSupply",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/PowerSubsystem/PowerSupplies/{}",
            chassis_id, psu_id
        ),
        "Id": psu_id,
        "Name": psu_id,
        "Manufacturer": manufacturer,
        "Model": model,
        "SerialNumber": serial,
        "PartNumber": part_number,
        "Status": {
            "State": if present { "Enabled" } else { "Absent" },
            "Health": health
        }
    })))
}

// ---------------------------------------------------------------------------
// ThermalMetrics (TODO 3)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/ThermalSubsystem/ThermalMetrics
///
/// Returns real-time thermal sensor readings for this chassis.
///
/// Reference: DMTF Redfish ThermalMetrics schema v1_3_2
/// Upstream: redfish-core/lib/thermal_metrics.hpp
///
/// On OpenBMC, temperature sensors are at paths under
/// `/xyz/openbmc_project/sensors/temperature/` with interface
/// `xyz.openbmc_project.Sensor.Value`.
pub async fn get_chassis_thermal_metrics(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Chassis/{}/ThermalSubsystem/ThermalMetrics",
        chassis_id
    );
    validate_chassis_id(&chassis_id)?;

    let temperature_readings: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
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
                let mut readings: Vec<Value> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        path.contains("/sensors/temperature/") && ifaces.contains_key(sensor_iface)
                    })
                    .filter_map(|(path, ifaces)| {
                        let props = ifaces.get(sensor_iface)?;
                        let reading = props.get("Value")?.as_f64()?;
                        let sensor_name = path.rsplit('/').next().unwrap_or("sensor");
                        // Build a sensor URI matching the Sensors collection scheme
                        let sensor_id = format!("temperature_{}", sensor_name);
                        Some(json!({
                            "DataSourceUri": format!(
                                "/redfish/v1/Chassis/{}/Sensors/{}",
                                chassis_id, sensor_id
                            ),
                            "Reading": reading,
                            "PhysicalContext": "SystemBoard"
                        }))
                    })
                    .collect();
                readings.sort_by_key(|v| {
                    v["DataSourceUri"].as_str().unwrap_or("").to_string()
                });
                readings
            }
            Err(e) => {
                warn!("Failed to enumerate temperature sensors for ThermalMetrics: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#ThermalMetrics.v1_3_2.ThermalMetrics",
        "@odata.id": format!(
            "/redfish/v1/Chassis/{}/ThermalSubsystem/ThermalMetrics",
            chassis_id
        ),
        "Id": "ThermalMetrics",
        "Name": "Chassis Thermal Metrics",
        "Description": "Real-time thermal sensor readings",
        "TemperatureReadingsCelsius@odata.count": temperature_readings.len(),
        "TemperatureReadingsCelsius": temperature_readings,
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

// ---------------------------------------------------------------------------
// PCIeSlots (TODO 5)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Chassis/{chassis_id}/PCIeSlots
///
/// Returns the PCIeSlots resource for this chassis.
///
/// Reference: DMTF Redfish PCIeSlots schema v1_7_0
/// Upstream: redfish-core/lib/pcie_slots.hpp
///
/// On OpenBMC, PCIe slot objects have interface
/// `xyz.openbmc_project.Inventory.Item.PCIeSlot` in the inventory tree.
/// Properties include SlotType (e.g. FullLength, HalfLength) and
/// PCIeType (Gen3, Gen4, Gen5).
pub async fn get_chassis_pcie_slots(
    State(state): State<Arc<AppState>>,
    Path(chassis_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Chassis/{}/PCIeSlots", chassis_id);
    validate_chassis_id(&chassis_id)?;

    let slots: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Inventory.Manager",
                "/xyz/openbmc_project/inventory",
            )
            .await
        {
            Ok(objects) => {
                let slot_iface = "xyz.openbmc_project.Inventory.Item.PCIeSlot";
                let mut slot_list: Vec<Value> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(slot_iface))
                    .map(|(path, ifaces)| {
                        let props = &ifaces[slot_iface];
                        let slot_id = path.rsplit('/').next().unwrap_or("slot0").to_string();

                        // Map OpenBMC SlotType enum → Redfish SlotType string
                        let slot_type = props.get("SlotType")
                            .and_then(|v| v.as_str())
                            .map(|s| {
                                if s.ends_with(".FullLength") { "FullLength" }
                                else if s.ends_with(".HalfLength") { "HalfLength" }
                                else if s.ends_with(".LowProfile") { "LowProfile" }
                                else if s.ends_with(".Mini") { "Mini" }
                                else if s.ends_with(".M2") { "M2" }
                                else if s.ends_with(".OEM") { "OEM" }
                                else { "OEM" }
                            })
                            .unwrap_or("OEM");

                        // Map OpenBMC PCIeType enum → Redfish PCIeTypes string
                        let pcie_type = props.get("PCIeType")
                            .and_then(|v| v.as_str())
                            .map(|s| {
                                if s.ends_with(".Gen1") { "Gen1" }
                                else if s.ends_with(".Gen2") { "Gen2" }
                                else if s.ends_with(".Gen3") { "Gen3" }
                                else if s.ends_with(".Gen4") { "Gen4" }
                                else if s.ends_with(".Gen5") { "Gen5" }
                                else { "Gen3" }
                            })
                            .unwrap_or("Gen3");

                        let lanes = props.get("Lanes")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(16);

                        json!({
                            "SlotType": slot_type,
                            "PCIeType": pcie_type,
                            "Lanes": lanes,
                            "Status": { "State": "Enabled", "Health": "OK" },
                            "Location": {
                                "PartLocation": {
                                    "ServiceLabel": slot_id
                                }
                            }
                        })
                    })
                    .collect();
                slot_list.sort_by_key(|v| {
                    v["Location"]["PartLocation"]["ServiceLabel"].as_str().unwrap_or("").to_string()
                });
                slot_list
            }
            Err(e) => {
                warn!("Failed to enumerate PCIe slots from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#PCIeSlots.v1_7_0.PCIeSlots",
        "@odata.id": format!("/redfish/v1/Chassis/{}/PCIeSlots", chassis_id),
        "Id": "PCIeSlots",
        "Name": "PCIe Slots",
        "Description": "PCIe slot inventory for this chassis",
        "Slots": slots
    })))
}

#[cfg(test)]
mod chassis_round3_tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_chassis_thermal_metrics_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_thermal_metrics(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ThermalMetrics.v1_3_2.ThermalMetrics");
        assert_eq!(json["Id"], "ThermalMetrics");
        assert_eq!(json["TemperatureReadingsCelsius@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_chassis_pcie_slots_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_chassis_pcie_slots(State(state), Path("chassis".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#PCIeSlots.v1_7_0.PCIeSlots");
        assert_eq!(json["Id"], "PCIeSlots");
    }
}
