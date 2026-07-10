//! Redfish Manager and ManagerCollection endpoints
//!
//! Implements:
//! - GET  /redfish/v1/Managers
//! - GET  /redfish/v1/Managers/{manager_id}
//! - POST /redfish/v1/Managers/{manager_id}/Actions/Manager.Reset
//! - GET  /redfish/v1/Managers/{manager_id}/NetworkProtocol
//! - PATCH /redfish/v1/Managers/{manager_id}/NetworkProtocol
//! - GET  /redfish/v1/Managers/{manager_id}/EthernetInterfaces
//! - GET  /redfish/v1/Managers/{manager_id}/EthernetInterfaces/{nic_id}
//! - GET  /redfish/v1/Managers/{manager_id}/LogServices
//!
//! Reference: DMTF Redfish Manager schema v1.19.0
//!
//! OpenBMC DBus sources:
//!   - BMC version: xyz.openbmc_project.Software.Version on BMC image object
//!   - Network config: xyz.openbmc_project.Network.EthernetInterface
//!   - NTP/DNS: xyz.openbmc_project.Network.SystemConfiguration

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_manager_id(manager_id: &str) -> Result<(), StatusCode> {
    if manager_id == "bmc" {
        Ok(())
    } else {
        warn!("Manager '{}' not found", manager_id);
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Managers collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers
pub async fn get_managers_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers");

    Ok(Json(json!({
        "@odata.type": "#ManagerCollection.ManagerCollection",
        "@odata.id": "/redfish/v1/Managers",
        "Name": "Manager Collection",
        "Members@odata.count": 1,
        "Members": [{ "@odata.id": "/redfish/v1/Managers/bmc" }]
    })))
}

/// GET /redfish/v1/Managers/{manager_id}
pub async fn get_manager(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}", manager_id);
    validate_manager_id(&manager_id)?;

    // TODO: Query FirmwareVersion from DBus:
    //   xyz.openbmc_project.Software.Version / Version on the active BMC image
    Ok(Json(json!({
        "@odata.type": "#Manager.v1_19_0.Manager",
        "@odata.id": "/redfish/v1/Managers/bmc",
        "Id": "bmc",
        "Name": "OpenBMC Manager",
        "Description": "Baseboard Management Controller",
        "ManagerType": "BMC",
        "UUID": state.system_uuid,
        "Model": "OpenBMC",
        "FirmwareVersion": "Unknown",
        "Status": { "State": "Enabled", "Health": "OK" },
        "PowerState": "On",
        "DateTime": chrono::Utc::now().to_rfc3339(),
        "DateTimeLocalOffset": "+00:00",
        "ServiceEntryPointUUID": state.system_uuid,
        "Links": {
            "ManagerForServers": [{ "@odata.id": "/redfish/v1/Systems/system" }],
            "ManagerForChassis": [{ "@odata.id": "/redfish/v1/Chassis/chassis" }],
            "ManagerInChassis": { "@odata.id": "/redfish/v1/Chassis/chassis" }
        },
        "EthernetInterfaces": { "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces" },
        "NetworkProtocol": { "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol" },
        "LogServices": { "@odata.id": "/redfish/v1/Managers/bmc/LogServices" },
        "SerialConsole": {
            "ServiceEnabled": true,
            "MaxConcurrentSessions": 15,
            "ConnectTypesSupported": ["SSH", "IPMI"]
        },
        "CommandShell": {
            "ServiceEnabled": true,
            "MaxConcurrentSessions": 4,
            "ConnectTypesSupported": ["SSH"]
        },
        "GraphicalConsole": {
            "ServiceEnabled": false,
            "MaxConcurrentSessions": 0,
            "ConnectTypesSupported": []
        },
        "Actions": {
            "#Manager.Reset": {
                "target": "/redfish/v1/Managers/bmc/Actions/Manager.Reset",
                "@Redfish.ActionInfo": "/redfish/v1/Managers/bmc/ResetActionInfo",
                "ResetType@Redfish.AllowableValues": ["GracefulRestart", "ForceRestart"]
            }
        }
    })))
}

/// POST /redfish/v1/Managers/{manager_id}/Actions/Manager.Reset
///
/// Resets (reboots) the BMC.
///
/// On OpenBMC this writes `xyz.openbmc_project.State.BMC.Transition.Reboot`
/// to `xyz.openbmc_project.State.BMC` / `RequestedBMCTransition`.
pub async fn reset_manager(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
    JsonBody(payload): JsonBody<Value>,
) -> Result<StatusCode, StatusCode> {
    debug!(
        "POST /redfish/v1/Managers/{}/Actions/Manager.Reset",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    let reset_type = payload
        .get("ResetType")
        .and_then(|v| v.as_str())
        .unwrap_or("GracefulRestart");

    match reset_type {
        "GracefulRestart" | "ForceRestart" => {
            // TODO: Write to xyz.openbmc_project.State.BMC.RequestedBMCTransition
            warn!(
                "Manager reset '{}' requested — DBus implementation pending",
                reset_type
            );
            Ok(StatusCode::NO_CONTENT)
        }
        _ => {
            warn!("Invalid manager ResetType: {}", reset_type);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

// ---------------------------------------------------------------------------
// NetworkProtocol
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/NetworkProtocol
///
/// Returns network service configuration (SSH, HTTPS, NTP, etc.)
///
/// OpenBMC source: xyz.openbmc_project.Network.SystemConfiguration
pub async fn get_network_protocol(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}/NetworkProtocol", manager_id);
    validate_manager_id(&manager_id)?;

    // TODO: Query NTP servers and hostname from DBus
    Ok(Json(json!({
        "@odata.type": "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol",
        "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol",
        "Id": "NetworkProtocol",
        "Name": "Manager Network Protocol",
        "Description": "Manager Network Service",
        "Status": { "State": "Enabled", "Health": "OK" },
        "HostName": "openbmc",
        "FQDN": "openbmc",
        "HTTP": { "ProtocolEnabled": false, "Port": 80 },
        "HTTPS": { "ProtocolEnabled": true, "Port": 443 },
        "SSH": { "ProtocolEnabled": true, "Port": 22 },
        "IPMI": { "ProtocolEnabled": true, "Port": 623 },
        "NTP": {
            "ProtocolEnabled": true,
            "Port": 123,
            "NTPServers": [],
            "NetworkSuppliedServers": []
        },
        "SNMP": { "ProtocolEnabled": false, "Port": 161 }
    })))
}

/// PATCH /redfish/v1/Managers/{manager_id}/NetworkProtocol
///
/// Updates network service configuration.
pub async fn patch_network_protocol(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
    JsonBody(_body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/Managers/{}/NetworkProtocol", manager_id);
    validate_manager_id(&manager_id)?;

    // TODO: Apply changes via DBus xyz.openbmc_project.Network.SystemConfiguration
    info!("NetworkProtocol PATCH requested (DBus implementation pending)");

    get_network_protocol(State(_state), Path(manager_id)).await
}

// ---------------------------------------------------------------------------
// EthernetInterfaces
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/EthernetInterfaces
pub async fn get_manager_ethernet_interfaces(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/EthernetInterfaces",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    // TODO: Enumerate NICs from DBus xyz.openbmc_project.Network.*
    Ok(Json(json!({
        "@odata.type": "#EthernetInterfaceCollection.EthernetInterfaceCollection",
        "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces",
        "Name": "Ethernet Interface Collection",
        "Description": "Collection of EthernetInterfaces for this Manager",
        "Members@odata.count": 1,
        "Members": [
            { "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0" }
        ]
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/EthernetInterfaces/{nic_id}
///
/// On OpenBMC, each NIC is represented as an object at
/// `/xyz/openbmc_project/network/<id>` with interface
/// `xyz.openbmc_project.Network.EthernetInterface`.
pub async fn get_manager_ethernet_interface(
    State(_state): State<Arc<AppState>>,
    Path((manager_id, nic_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/EthernetInterfaces/{}",
        manager_id, nic_id
    );
    validate_manager_id(&manager_id)?;

    if nic_id != "eth0" {
        warn!("NIC '{}' not found on manager '{}'", nic_id, manager_id);
        return Err(StatusCode::NOT_FOUND);
    }

    // TODO: Query interface properties from DBus
    Ok(Json(json!({
        "@odata.type": "#EthernetInterface.v1_9_0.EthernetInterface",
        "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0",
        "Id": "eth0",
        "Name": "eth0",
        "Description": "Management Ethernet Interface",
        "InterfaceEnabled": true,
        "MACAddress": "00:00:00:00:00:00",
        "SpeedMbps": 1000,
        "AutoNeg": true,
        "FullDuplex": true,
        "MTUSize": 1500,
        "HostName": "openbmc",
        "FQDN": "openbmc",
        "IPv4Addresses": [],
        "IPv6Addresses": [],
        "IPv4StaticAddresses": [],
        "IPv6StaticAddresses": [],
        "NameServers": [],
        "StaticNameServers": [],
        "DHCPv4": {
            "DHCPEnabled": true,
            "UseDNSServers": true,
            "UseNTPServers": true,
            "UseGateway": true,
            "UseDomainName": true
        },
        "DHCPv6": {
            "OperatingMode": "Stateful",
            "UseDNSServers": true,
            "UseNTPServers": true,
            "UseDomainName": true
        },
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

// ---------------------------------------------------------------------------
// LogServices
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/LogServices
pub async fn get_manager_log_services(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}/LogServices", manager_id);
    validate_manager_id(&manager_id)?;

    Ok(Json(json!({
        "@odata.type": "#LogServiceCollection.LogServiceCollection",
        "@odata.id": "/redfish/v1/Managers/bmc/LogServices",
        "Name": "Log Services Collection",
        "Description": "Collection of LogServices for this Manager",
        "Members@odata.count": 2,
        "Members": [
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/BMC" },
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/Dump" }
        ]
    })))
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
        assert!(json["NetworkProtocol"]["@odata.id"].is_string());
        assert!(json["EthernetInterfaces"]["@odata.id"].is_string());
        assert!(json["LogServices"]["@odata.id"].is_string());
    }

    #[tokio::test]
    async fn test_get_manager_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager(State(state), Path("invalid".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_network_protocol() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_network_protocol(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol");
        assert_eq!(json["HTTPS"]["ProtocolEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_manager_ethernet_interfaces() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_ethernet_interfaces(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_manager_log_services() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_log_services(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 2);
    }
}
