//! Redfish Fabric, Switch, and FabricAdapter endpoints
//!
//! Implements:
//! - GET /redfish/v1/Fabrics
//! - GET /redfish/v1/Fabrics/{fabric_id}
//! - GET /redfish/v1/Fabrics/{fabric_id}/Switches
//! - GET /redfish/v1/Fabrics/{fabric_id}/Switches/{switch_id}
//!
//! Reference: DMTF Redfish Fabric schema v1.3.0, Switch schema v1.7.0
//! Upstream: redfish-core/lib/fabric.hpp
//!
//! OpenBMC DBus sources:
//!   - PCIe switches: xyz.openbmc_project.Inventory.Item.PCIeSwitch
//!     Enumerated via xyz.openbmc_project.Inventory.Manager/GetManagedObjects

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
// Fabrics collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Fabrics
///
/// Returns the Fabrics collection.  On OpenBMC the only fabric type
/// currently supported is PCIe (via PCIeSwitch inventory objects).
/// When no switches are found in DBus the collection is empty.
///
/// Upstream: redfish-core/lib/fabric.hpp
pub async fn get_fabrics_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Fabrics");

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
                let switch_iface = "xyz.openbmc_project.Inventory.Item.PCIeSwitch";
                // Fabric IDs are the parent path segment of each switch
                let mut fabric_ids: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(switch_iface))
                    .filter_map(|(path, _)| {
                        let parts: Vec<&str> = path.split('/').collect();
                        // .../fabric/<fabric_id>/switch/<switch_id>
                        parts.iter().position(|&s| s == "fabric")
                            .and_then(|i| parts.get(i + 1))
                            .map(|s| s.to_string())
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                fabric_ids.sort();

                if fabric_ids.is_empty() {
                    // Synthesise one fabric named "HGX" when any PCIeSwitch exists
                    // but is not nested under a fabric path
                    let has_switches = objects
                        .iter()
                        .any(|(_, ifaces)| ifaces.contains_key(switch_iface));
                    if has_switches {
                        fabric_ids.push("HGX".to_string());
                    }
                }

                fabric_ids
                    .iter()
                    .map(|id| json!({ "@odata.id": format!("/redfish/v1/Fabrics/{}", id) }))
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate Fabrics from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#FabricCollection.FabricCollection",
        "@odata.id": "/redfish/v1/Fabrics",
        "Name": "Fabric Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

// ---------------------------------------------------------------------------
// Fabric instance
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Fabrics/{fabric_id}
///
/// Returns a single Fabric resource.  Upstream bmcweb maps fabric to PCIe
/// switches aggregated under a fabric inventory parent path.
///
/// Upstream: redfish-core/lib/fabric.hpp
pub async fn get_fabric(
    State(_state): State<Arc<AppState>>,
    Path(fabric_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Fabrics/{}", fabric_id);

    Ok(Json(json!({
        "@odata.type": "#Fabric.v1_3_0.Fabric",
        "@odata.id": format!("/redfish/v1/Fabrics/{}", fabric_id),
        "Id": fabric_id,
        "Name": fabric_id,
        "FabricType": "PCIe",
        "Status": { "State": "Enabled", "Health": "OK" },
        "Switches": {
            "@odata.id": format!("/redfish/v1/Fabrics/{}/Switches", fabric_id)
        }
    })))
}

// ---------------------------------------------------------------------------
// Switches collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Fabrics/{fabric_id}/Switches
///
/// Returns all PCIe switches associated with this fabric.  Enumerates
/// `xyz.openbmc_project.Inventory.Item.PCIeSwitch` objects from DBus.
///
/// Upstream: redfish-core/lib/fabric.hpp
pub async fn get_fabric_switches(
    State(state): State<Arc<AppState>>,
    Path(fabric_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Fabrics/{}/Switches", fabric_id);

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
                let switch_iface = "xyz.openbmc_project.Inventory.Item.PCIeSwitch";
                let mut switch_ids: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(switch_iface))
                    .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                    .collect();
                switch_ids.sort();

                switch_ids
                    .iter()
                    .map(|id| {
                        json!({
                            "@odata.id": format!(
                                "/redfish/v1/Fabrics/{}/Switches/{}",
                                fabric_id, id
                            )
                        })
                    })
                    .collect()
            }
            Err(e) => {
                warn!("Failed to enumerate Switches from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#SwitchCollection.SwitchCollection",
        "@odata.id": format!("/redfish/v1/Fabrics/{}/Switches", fabric_id),
        "Name": "Switch Collection",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

// ---------------------------------------------------------------------------
// Switch instance
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Fabrics/{fabric_id}/Switches/{switch_id}
///
/// Returns a single PCIe Switch resource.
///
/// Upstream: redfish-core/lib/fabric.hpp `handleFabricSwitchPathSwitchGet`
pub async fn get_fabric_switch(
    State(_state): State<Arc<AppState>>,
    Path((fabric_id, switch_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Fabrics/{}/Switches/{}",
        fabric_id, switch_id
    );

    Ok(Json(json!({
        "@odata.type": "#Switch.v1_7_0.Switch",
        "@odata.id": format!("/redfish/v1/Fabrics/{}/Switches/{}", fabric_id, switch_id),
        "Id": switch_id,
        "Name": switch_id,
        "SwitchType": "PCIe",
        "Status": { "State": "Enabled", "Health": "OK" },
        "Ports": {
            "@odata.id": format!(
                "/redfish/v1/Fabrics/{}/Switches/{}/Ports",
                fabric_id, switch_id
            )
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_fabrics_collection_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_fabrics_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#FabricCollection.FabricCollection");
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_fabric() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_fabric(State(state), Path("HGX".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Fabric.v1_3_0.Fabric");
        assert_eq!(json["FabricType"], "PCIe");
    }

    #[tokio::test]
    async fn test_get_fabric_switches_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_fabric_switches(State(state), Path("HGX".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_fabric_switch() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_fabric_switch(
            State(state),
            Path(("HGX".to_string(), "NVSwitch_0".to_string())),
        )
        .await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#Switch.v1_7_0.Switch");
        assert_eq!(json["SwitchType"], "PCIe");
    }
}
