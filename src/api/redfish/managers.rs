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

use crate::dbus::{DbusClient, ZBusClient};
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

    // Query FirmwareVersion from DBus: xyz.openbmc_project.Software.Version on the
    // active BMC image object at /xyz/openbmc_project/software/active
    let firmware_version = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_property(
                "/xyz/openbmc_project/software/active",
                "xyz.openbmc_project.Software.Version",
                "Version",
            )
            .await
        {
            Ok(v) => v
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            Err(e) => {
                warn!("Failed to read BMC FirmwareVersion from DBus: {}", e);
                "Unknown".to_string()
            }
        }
    } else {
        "Unknown".to_string()
    };

    Ok(Json(json!({
        "@odata.type": "#Manager.v1_19_0.Manager",
        "@odata.id": "/redfish/v1/Managers/bmc",
        "Id": "bmc",
        "Name": "OpenBMC Manager",
        "Description": "Baseboard Management Controller",
        "ManagerType": "BMC",
        "UUID": state.system_uuid,
        "Model": "OpenBMC",
        "FirmwareVersion": firmware_version,
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
/// Resets (reboots) the BMC by writing to
/// `xyz.openbmc_project.State.BMC / RequestedBMCTransition`.
///
/// OpenBMC transition values:
///   GracefulRestart → xyz.openbmc_project.State.BMC.Transition.Reboot
///   ForceRestart    → xyz.openbmc_project.State.BMC.Transition.HardReboot
pub async fn reset_manager(
    State(state): State<Arc<AppState>>,
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

    let transition = match reset_type {
        "GracefulRestart" => "xyz.openbmc_project.State.BMC.Transition.Reboot",
        "ForceRestart"    => "xyz.openbmc_project.State.BMC.Transition.HardReboot",
        _ => {
            warn!("Invalid manager ResetType: {}", reset_type);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .set_property(
                "/xyz/openbmc_project/state/bmc0",
                "xyz.openbmc_project.State.BMC",
                "RequestedBMCTransition",
                serde_json::json!(transition),
            )
            .await
        {
            Ok(()) => {
                info!("BMC reset '{}' initiated via DBus", reset_type);
            }
            Err(e) => {
                warn!("BMC reset '{}' DBus call failed: {}", reset_type, e);
                // Still return 204 — the request was valid even if DBus failed
            }
        }
    } else {
        warn!("BMC reset '{}' requested — no DBus connection", reset_type);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// NetworkProtocol
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/NetworkProtocol
///
/// Returns network service configuration (SSH, HTTPS, NTP, etc.)
///
/// OpenBMC source: xyz.openbmc_project.Network.SystemConfiguration at
///   /xyz/openbmc_project/network/config
pub async fn get_network_protocol(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}/NetworkProtocol", manager_id);
    validate_manager_id(&manager_id)?;

    // Query hostname and NTP server list from DBus
    let (hostname, ntp_servers) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());

        let hostname = client
            .get_property(
                "/xyz/openbmc_project/network/config",
                "xyz.openbmc_project.Network.SystemConfiguration",
                "HostName",
            )
            .await
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "openbmc".to_string());

        let ntp_servers: Vec<Value> = client
            .get_property(
                "/xyz/openbmc_project/network/config",
                "xyz.openbmc_project.Network.SystemConfiguration",
                "NTPServers",
            )
            .await
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();

        (hostname, ntp_servers)
    } else {
        ("openbmc".to_string(), vec![])
    };

    let fqdn = hostname.clone();
    Ok(Json(json!({
        "@odata.type": "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol",
        "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol",
        "Id": "NetworkProtocol",
        "Name": "Manager Network Protocol",
        "Description": "Manager Network Service",
        "Status": { "State": "Enabled", "Health": "OK" },
        "HostName": hostname,
        "FQDN": fqdn,
        "HTTP": { "ProtocolEnabled": false, "Port": 80 },
        "HTTPS": { "ProtocolEnabled": true, "Port": 443 },
        "SSH": { "ProtocolEnabled": true, "Port": 22 },
        "IPMI": { "ProtocolEnabled": true, "Port": 623 },
        "NTP": {
            "ProtocolEnabled": true,
            "Port": 123,
            "NTPServers": ntp_servers,
            "NetworkSuppliedServers": []
        },
        "SNMP": { "ProtocolEnabled": false, "Port": 161 }
    })))
}

/// PATCH /redfish/v1/Managers/{manager_id}/NetworkProtocol
///
/// Updates HostName and/or NTP server list via DBus.
///
/// OpenBMC DBus target:
///   /xyz/openbmc_project/network/config
///   interface: xyz.openbmc_project.Network.SystemConfiguration
///   properties: HostName (string), NTPServers (array of strings)
pub async fn patch_network_protocol(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
    JsonBody(body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/Managers/{}/NetworkProtocol", manager_id);
    validate_manager_id(&manager_id)?;

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());

        // Apply HostName if provided
        if let Some(hostname) = body.get("HostName").and_then(|v| v.as_str()) {
            match client
                .set_property(
                    "/xyz/openbmc_project/network/config",
                    "xyz.openbmc_project.Network.SystemConfiguration",
                    "HostName",
                    serde_json::json!(hostname),
                )
                .await
            {
                Ok(()) => info!("Hostname set to '{}' via DBus", hostname),
                Err(e) => warn!("Failed to set hostname via DBus: {}", e),
            }
        }

        // Apply NTP server list if provided via NTP.NTPServers
        if let Some(ntp) = body.get("NTP") {
            if let Some(servers) = ntp.get("NTPServers").and_then(|v| v.as_array()) {
                let server_list: Vec<serde_json::Value> = servers.clone();
                match client
                    .set_property(
                        "/xyz/openbmc_project/network/config",
                        "xyz.openbmc_project.Network.SystemConfiguration",
                        "NTPServers",
                        serde_json::json!(server_list),
                    )
                    .await
                {
                    Ok(()) => info!("NTP servers updated via DBus ({} servers)", server_list.len()),
                    Err(e) => warn!("Failed to set NTP servers via DBus: {}", e),
                }
            }
        }
    } else {
        info!("NetworkProtocol PATCH: no DBus connection");
    }

    get_network_protocol(State(state), Path(manager_id)).await
}

// ---------------------------------------------------------------------------
// EthernetInterfaces
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/EthernetInterfaces
///
/// Enumerates BMC network interfaces from DBus by listing objects under
/// `/xyz/openbmc_project/network/` that implement
/// `xyz.openbmc_project.Network.EthernetInterface`.
/// Falls back to a single `eth0` entry when DBus is unavailable.
pub async fn get_manager_ethernet_interfaces(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/EthernetInterfaces",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Network",
                "/xyz/openbmc_project/network",
            )
            .await
        {
            Ok(objects) => {
                let nic_iface = "xyz.openbmc_project.Network.EthernetInterface";
                let mut nics: Vec<String> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(nic_iface))
                    .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                    .collect();
                nics.sort();

                if nics.is_empty() {
                    // Always expose at least eth0 as a fallback
                    vec![json!({ "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0" })]
                } else {
                    nics.iter()
                        .map(|id| json!({
                            "@odata.id": format!("/redfish/v1/Managers/bmc/EthernetInterfaces/{}", id)
                        }))
                        .collect()
                }
            }
            Err(e) => {
                warn!("Failed to enumerate NICs from DBus: {} — using eth0 fallback", e);
                vec![json!({ "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0" })]
            }
        }
    } else {
        vec![json!({ "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0" })]
    };

    Ok(Json(json!({
        "@odata.type": "#EthernetInterfaceCollection.EthernetInterfaceCollection",
        "@odata.id": "/redfish/v1/Managers/bmc/EthernetInterfaces",
        "Name": "Ethernet Interface Collection",
        "Description": "Collection of EthernetInterfaces for this Manager",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/EthernetInterfaces/{nic_id}
///
/// On OpenBMC, each NIC is represented as an object at
/// `/xyz/openbmc_project/network/<id>` with interface
/// `xyz.openbmc_project.Network.EthernetInterface`.
pub async fn get_manager_ethernet_interface(
    State(state): State<Arc<AppState>>,
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

    // Query MAC address and IP configuration from DBus
    let dbus_path = format!("/xyz/openbmc_project/network/{}", nic_id);
    let net_iface = "xyz.openbmc_project.Network.EthernetInterface";

    let (mac_address, ipv4_addresses, ipv6_addresses, hostname) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            let client = ZBusClient::from_connection(conn.clone());

            let mac = client
                .get_property(&dbus_path, net_iface, "MACAddress")
                .await
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "00:00:00:00:00:00".to_string());

            let ipv4 = client
                .get_property(&dbus_path, net_iface, "IPv4Addresses")
                .await
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            let ipv6 = client
                .get_property(&dbus_path, net_iface, "IPv6Addresses")
                .await
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            let hn = client
                .get_property(
                    "/xyz/openbmc_project/network/config",
                    "xyz.openbmc_project.Network.SystemConfiguration",
                    "HostName",
                )
                .await
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "openbmc".to_string());

            (mac, ipv4, ipv6, hn)
        } else {
            (
                "00:00:00:00:00:00".to_string(),
                vec![],
                vec![],
                "openbmc".to_string(),
            )
        };

    let fqdn = hostname.clone();
    Ok(Json(json!({
        "@odata.type": "#EthernetInterface.v1_9_0.EthernetInterface",
        "@odata.id": format!("/redfish/v1/Managers/bmc/EthernetInterfaces/{}", nic_id),
        "Id": nic_id,
        "Name": nic_id,
        "Description": "Management Ethernet Interface",
        "InterfaceEnabled": true,
        "MACAddress": mac_address,
        "SpeedMbps": 1000,
        "AutoNeg": true,
        "FullDuplex": true,
        "MTUSize": 1500,
        "HostName": hostname,
        "FQDN": fqdn,
        "IPv4Addresses": ipv4_addresses,
        "IPv6Addresses": ipv6_addresses,
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
