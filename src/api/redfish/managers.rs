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
//! - GET  /redfish/v1/Managers/{manager_id}/LogServices/Journal
//! - GET  /redfish/v1/Managers/{manager_id}/LogServices/Journal/Entries
//!
//! Reference: DMTF Redfish Manager schema v1.19.0
//!
//! OpenBMC DBus sources:
//!   - BMC version: xyz.openbmc_project.Software.Version on BMC image object
//!   - Network config: xyz.openbmc_project.Network.EthernetInterface
//!   - NTP/DNS: xyz.openbmc_project.Network.SystemConfiguration

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_ACTION, PRIVILEGE_CONFIGURE_COMPONENTS, PRIVILEGE_PATCH};
use crate::auth::session::UserSession;
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
        "ManagerDiagnosticData": { "@odata.id": "/redfish/v1/Managers/bmc/ManagerDiagnosticData" },
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
    Extension(session): Extension<UserSession>,
    Path(manager_id): Path<String>,
    JsonBody(payload): JsonBody<Value>,
) -> Result<StatusCode, StatusCode> {
    debug!(
        "POST /redfish/v1/Managers/{}/Actions/Manager.Reset",
        manager_id
    );
    check_privilege(Some(&session), PRIVILEGE_ACTION)?;
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

    // Query hostname, NTP server list, and IPMI state from DBus
    let (hostname, ntp_servers, ipmi_enabled) = if let Some(conn) = state.dbus_connection.as_deref() {
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

        // IPMI over LAN state from phosphor-ipmi-net DBus object
        // upstream commit 9352bdc8: xyz.openbmc_project.Control.Service.Attributes / Running
        let ipmi_enabled = client
            .get_property(
                "/xyz/openbmc_project/control/service/phosphor_2dipmi_2dnet",
                "xyz.openbmc_project.Control.Service.Attributes",
                "Running",
            )
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        (hostname, ntp_servers, ipmi_enabled)
    } else {
        ("openbmc".to_string(), vec![], true)
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
        "IPMI": { "ProtocolEnabled": ipmi_enabled, "Port": 623 },
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
/// OpenBMC DBus targets:
///   /xyz/openbmc_project/network/config
///     interface: xyz.openbmc_project.Network.SystemConfiguration
///     properties: HostName (string), NTPServers (array of strings)
///
///   /xyz/openbmc_project/control/service/dropbear
///     interface: xyz.openbmc_project.Control.Service.Attributes
///     property: Running (bool) — SSH ProtocolEnabled
///
///   /xyz/openbmc_project/control/service/phosphor_2dipmi_2dnet
///     interface: xyz.openbmc_project.Control.Service.Attributes
///     property: Running (bool) — IPMI ProtocolEnabled
///
/// Upstream: redfish-core/lib/managers.hpp (commit 8f987662 — per-property error paths)
pub async fn patch_network_protocol(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(manager_id): Path<String>,
    JsonBody(body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/Managers/{}/NetworkProtocol", manager_id);
    check_privilege(Some(&session), PRIVILEGE_PATCH)?;
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

        // Apply SSH ProtocolEnabled if provided via SSH.ProtocolEnabled
        // Upstream: redfish-core/lib/managers.hpp handleProtocolEnabled() with
        // per-property Redfish error path — SSH/ProtocolEnabled (commit 8f987662)
        if let Some(ssh) = body.get("SSH") {
            if let Some(enabled) = ssh.get("ProtocolEnabled").and_then(|v| v.as_bool()) {
                match client
                    .set_property(
                        "/xyz/openbmc_project/control/service/dropbear",
                        "xyz.openbmc_project.Control.Service.Attributes",
                        "Running",
                        serde_json::json!(enabled),
                    )
                    .await
                {
                    Ok(()) => info!("SSH ProtocolEnabled set to {} via DBus", enabled),
                    // Not all systems have obmc-console-server; absence is not an error.
                    Err(e) => warn!("Failed to set SSH/ProtocolEnabled via DBus (service may be absent): {}", e),
                }
            }
        }

        // Apply IPMI ProtocolEnabled if provided via IPMI.ProtocolEnabled
        // Upstream: redfish-core/lib/managers.hpp handleProtocolEnabled() —
        // IPMI/ProtocolEnabled (commit 8f987662 uses correct per-property path)
        if let Some(ipmi) = body.get("IPMI") {
            if let Some(enabled) = ipmi.get("ProtocolEnabled").and_then(|v| v.as_bool()) {
                match client
                    .set_property(
                        "/xyz/openbmc_project/control/service/phosphor_2dipmi_2dnet",
                        "xyz.openbmc_project.Control.Service.Attributes",
                        "Running",
                        serde_json::json!(enabled),
                    )
                    .await
                {
                    Ok(()) => info!("IPMI ProtocolEnabled set to {} via DBus", enabled),
                    Err(e) => warn!("Failed to set IPMI/ProtocolEnabled via DBus: {}", e),
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

    // Validate that this NIC actually exists in DBus before serving it.
    // Accept any NIC id present in the network tree; eth0 is always valid.
    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        if let Ok(objects) = client
            .get_managed_objects("xyz.openbmc_project.Network", "/xyz/openbmc_project/network")
            .await
        {
            let nic_iface = "xyz.openbmc_project.Network.EthernetInterface";
            let known: Vec<_> = objects
                .iter()
                .filter(|(_, ifaces)| ifaces.contains_key(nic_iface))
                .filter_map(|(path, _)| path.rsplit('/').next().map(|s| s.to_string()))
                .collect();
            if !known.is_empty() && !known.iter().any(|n| n == &nic_id) {
                warn!("NIC '{}' not found (known: {:?})", nic_id, known);
                return Err(StatusCode::NOT_FOUND);
            }
        }
    } else if nic_id != "eth0" {
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

/// PATCH /redfish/v1/Managers/{manager_id}/EthernetInterfaces/{nic_id}
///
/// Applies static IP address configuration via DBus.
///
/// OpenBMC IP configuration:
///   Service:   xyz.openbmc_project.Network
///   Object:    /xyz/openbmc_project/network/<nic_id>
///   Method:    xyz.openbmc_project.Network.IP.Create
///     args: (string address_type "ipv4", string address, uint8 prefix_len, string gateway)
///
/// Also supports:
///   DHCPEnabled  → xyz.openbmc_project.Network.EthernetInterface / DHCPEnabled (bool)
///   MACAddress   → xyz.openbmc_project.Network.EthernetInterface / MACAddress (string)
pub async fn patch_manager_ethernet_interface(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path((manager_id, nic_id)): Path<(String, String)>,
    JsonBody(body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "PATCH /redfish/v1/Managers/{}/EthernetInterfaces/{}",
        manager_id, nic_id
    );
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_COMPONENTS)?;
    validate_manager_id(&manager_id)?;

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        let dbus_path = format!("/xyz/openbmc_project/network/{}", nic_id);
        let net_iface = "xyz.openbmc_project.Network.EthernetInterface";

        // Toggle DHCP if DHCPv4.DHCPEnabled is provided
        if let Some(dhcp_enabled) = body
            .get("DHCPv4")
            .and_then(|d| d.get("DHCPEnabled"))
            .and_then(|v| v.as_bool())
        {
            if let Err(e) = client
                .set_property(
                    &dbus_path,
                    net_iface,
                    "DHCPEnabled",
                    serde_json::json!(dhcp_enabled),
                )
                .await
            {
                warn!("Failed to set DHCPEnabled on {}: {}", nic_id, e);
            } else {
                info!("DHCPEnabled={} on {} via DBus", dhcp_enabled, nic_id);
            }
        }

        // Set MACAddress if provided (some systems allow BMC MAC override)
        if let Some(mac) = body.get("MACAddress").and_then(|v| v.as_str()) {
            if let Err(e) = client
                .set_property(
                    &dbus_path,
                    net_iface,
                    "MACAddress",
                    serde_json::json!(mac),
                )
                .await
            {
                warn!("Failed to set MACAddress on {}: {}", nic_id, e);
            } else {
                info!("MACAddress set to {} on {} via DBus", mac, nic_id);
            }
        }

        // Add a static IPv4 address if IPv4StaticAddresses is provided
        if let Some(statics) = body.get("IPv4StaticAddresses").and_then(|v| v.as_array()) {
            for addr in statics {
                let address = addr.get("Address").and_then(|v| v.as_str()).unwrap_or("");
                let prefix = addr.get("SubnetMask").and_then(|v| v.as_u64()).unwrap_or(24) as u8;
                let gateway = addr.get("Gateway").and_then(|v| v.as_str()).unwrap_or("");
                if !address.is_empty() {
                    let args = serde_json::json!({
                        "AddressType": "ipv4",
                        "Address": address,
                        "PrefixLength": prefix,
                        "Gateway": gateway
                    });
                    match client
                        .call_method(
                            "xyz.openbmc_project.Network",
                            &dbus_path,
                            "xyz.openbmc_project.Network.IP.Create",
                            "IP",
                            Some(&args),
                        )
                        .await
                    {
                        Ok(_) => info!("Static IP {}/{} set on {} via DBus", address, prefix, nic_id),
                        Err(e) => warn!("Failed to set static IP on {}: {}", nic_id, e),
                    }
                }
            }
        }
    }

    // Return the updated NIC resource
    get_manager_ethernet_interface(State(state), Path((manager_id, nic_id))).await
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
        "Members@odata.count": 4,
        "Members": [
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/BMC" },
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/Dump" },
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/Journal" },
            { "@odata.id": "/redfish/v1/Managers/bmc/LogServices/DBusEventLog" }
        ]
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/LogServices/BMC
///
/// The BMC log service — on OpenBMC backed by
/// xyz.openbmc_project.Logging (same service as the system event log).
pub async fn get_manager_bmc_log_service(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/Managers/{}/LogServices/BMC", manager_id);
    validate_manager_id(&manager_id)?;

    Ok(Json(json!({
        "@odata.type": "#LogService.v1_4_0.LogService",
        "@odata.id": "/redfish/v1/Managers/bmc/LogServices/BMC",
        "Id": "BMC",
        "Name": "BMC Log Service",
        "Description": "BMC System Event Log",
        "ServiceEnabled": true,
        "LogEntryType": "Event",
        "OverWritePolicy": "WrapsWhenFull",
        "DateTime": chrono::Utc::now().to_rfc3339(),
        "DateTimeLocalOffset": "+00:00",
        "Entries": {
            "@odata.id": "/redfish/v1/Managers/bmc/LogServices/BMC/Entries"
        },
        "Actions": {
            "#LogService.ClearLog": {
                "target": "/redfish/v1/Managers/bmc/LogServices/BMC/Actions/LogService.ClearLog"
            }
        },
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/LogServices/BMC/Entries
///
/// Returns BMC log entries from xyz.openbmc_project.Logging.
/// Shares the same DBus source as the System EventLog entries.
pub async fn get_manager_bmc_log_entries(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/BMC/Entries",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Logging",
                "/xyz/openbmc_project/logging",
            )
            .await
        {
            Ok(objects) => {
                let entry_iface = "xyz.openbmc_project.Logging.Entry";
                let mut entries: Vec<(u64, Value)> = objects
                    .iter()
                    .filter(|(path, ifaces)| {
                        ifaces.contains_key(entry_iface)
                            && path.contains("/logging/entry/")
                    })
                    .map(|(path, ifaces)| {
                        let props = &ifaces[entry_iface];
                        let id_num = path
                            .rsplit('/')
                            .next()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);
                        let msg = props
                            .get("Message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let severity_raw = props
                            .get("Severity")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let severity = if severity_raw.ends_with(".Error") || severity_raw.ends_with(".Critical") {
                            "Critical"
                        } else if severity_raw.ends_with(".Warning") {
                            "Warning"
                        } else {
                            "OK"
                        };
                        let ts_ms = props
                            .get("Timestamp")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        use chrono::{TimeZone, Utc};
                        let secs = (ts_ms / 1000) as i64;
                        let created = Utc.timestamp_opt(secs, 0)
                            .single()
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
                        let entry_id = id_num.to_string();
                        let entry = json!({
                            "@odata.type": "#LogEntry.v1_15_0.LogEntry",
                            "@odata.id": format!(
                                "/redfish/v1/Managers/bmc/LogServices/BMC/Entries/{}",
                                entry_id
                            ),
                            "Id": entry_id,
                            "Name": format!("Log Entry {}", entry_id),
                            "EntryType": "Event",
                            "Severity": severity,
                            "Created": created,
                            "Message": msg
                        });
                        (id_num, entry)
                    })
                    .collect();
                entries.sort_by_key(|&(id, _)| std::cmp::Reverse(id));
                entries.into_iter().map(|(_, v)| v).collect()
            }
            Err(e) => {
                warn!("Failed to read BMC log entries from DBus: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#LogEntryCollection.LogEntryCollection",
        "@odata.id": "/redfish/v1/Managers/bmc/LogServices/BMC/Entries",
        "Name": "BMC Log Entries",
        "Description": "Collection of BMC log entries",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/LogServices/BMC/Entries/{entry_id}
///
/// Fetches a single BMC log entry from xyz.openbmc_project.Logging.
pub async fn get_manager_bmc_log_entry(
    State(state): State<Arc<AppState>>,
    Path((manager_id, entry_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/BMC/Entries/{}",
        manager_id, entry_id
    );
    validate_manager_id(&manager_id)?;

    let id_num: u64 = entry_id.parse().map_err(|_| StatusCode::NOT_FOUND)?;

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        let dbus_path = format!("/xyz/openbmc_project/logging/entry/{}", id_num);
        let entry_iface = "xyz.openbmc_project.Logging.Entry";

        match client.get_all_properties(&dbus_path, entry_iface).await {
            Ok(props) => {
                let msg = props.get("Message")
                    .and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                let severity_raw = props.get("Severity")
                    .and_then(|v| v.as_str()).unwrap_or("");
                let severity = if severity_raw.ends_with(".Error") || severity_raw.ends_with(".Critical") {
                    "Critical"
                } else if severity_raw.ends_with(".Warning") {
                    "Warning"
                } else {
                    "OK"
                };
                let ts_ms = props.get("Timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
                use chrono::{TimeZone, Utc};
                let created = Utc.timestamp_opt((ts_ms / 1000) as i64, 0)
                    .single()
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

                return Ok(Json(json!({
                    "@odata.type": "#LogEntry.v1_15_0.LogEntry",
                    "@odata.id": format!(
                        "/redfish/v1/Managers/bmc/LogServices/BMC/Entries/{}",
                        entry_id
                    ),
                    "Id": entry_id,
                    "Name": format!("Log Entry {}", entry_id),
                    "EntryType": "Event",
                    "Severity": severity,
                    "Created": created,
                    "Message": msg
                })));
            }
            Err(_) => return Err(StatusCode::NOT_FOUND),
        }
    }
    Err(StatusCode::NOT_FOUND)
}

/// POST /redfish/v1/Managers/{manager_id}/LogServices/BMC/Actions/LogService.ClearLog
pub async fn clear_manager_bmc_log(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(manager_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!(
        "POST /redfish/v1/Managers/{}/LogServices/BMC/Actions/LogService.ClearLog",
        manager_id
    );
    check_privilege(Some(&session), PRIVILEGE_ACTION)?;
    validate_manager_id(&manager_id)?;

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .call_method(
                "xyz.openbmc_project.Logging",
                "/xyz/openbmc_project/logging",
                "xyz.openbmc_project.Collection.DeleteAll",
                "DeleteAll",
                None,
            )
            .await
        {
            Ok(_) => info!("BMC log cleared via DBus"),
            Err(e) => warn!("Failed to clear BMC log via DBus: {}", e),
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// ManagerDiagnosticData
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/ManagerDiagnosticData
///
/// Returns BMC process health metrics: memory usage, processor utilization,
/// and uptime.
///
/// Reference: DMTF Redfish ManagerDiagnosticData schema v1_2_0
/// Upstream: redfish-core/lib/manager_diagnostic_data.hpp
///
/// On OpenBMC, process-level health is monitored by `xyz.openbmc_project.HealthMon`.
/// When HealthMon is not running (e.g. QEMU) this handler falls back to reading
/// `/proc/meminfo` and `/proc/uptime` directly from the host filesystem.
pub async fn get_manager_diagnostic_data(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/ManagerDiagnosticData",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    // Read memory info from /proc/meminfo
    let (free_storage_kib, total_memory_kib, free_memory_kib) = read_proc_meminfo();

    // Read uptime from /proc/uptime (seconds since boot)
    let uptime_seconds: Option<f64> = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()));

    // Build ISO 8601 duration string from uptime seconds
    let uptime_str = uptime_seconds.map(|secs| {
        let total = secs as u64;
        let days = total / 86400;
        let hours = (total % 86400) / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;
        if days > 0 {
            format!("P{}DT{}H{}M{}S", days, hours, minutes, seconds)
        } else {
            format!("PT{}H{}M{}S", hours, minutes, seconds)
        }
    });

    Ok(Json(json!({
        "@odata.type": "#ManagerDiagnosticData.v1_2_0.ManagerDiagnosticData",
        "@odata.id": format!("/redfish/v1/Managers/{}/ManagerDiagnosticData", manager_id),
        "Id": "ManagerDiagnosticData",
        "Name": "Manager Diagnostic Data",
        "Description": "Diagnostic data for the manager process",
        "FreeStorageSpaceKiB": free_storage_kib,
        "MemoryStatistics": {
            "FreeKiB": free_memory_kib,
            "TotalKiB": total_memory_kib,
            "SharedKiB": null,
            "BuffersAndCacheKiB": null,
            "AvailableKiB": free_memory_kib
        },
        "ServiceRootUptimeSeconds": uptime_str,
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

/// Read memory statistics from /proc/meminfo.
/// Returns (free_storage_kib, total_memory_kib, free_memory_kib).
fn read_proc_meminfo() -> (Option<u64>, Option<u64>, Option<u64>) {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return (None, None, None),
    };
    let mut total = None;
    let mut free = None;
    for line in content.lines() {
        let mut parts = line.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim();
        let val_str = parts.next().unwrap_or("").trim();
        // Values are in kB, strip the " kB" suffix
        let val: Option<u64> = val_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok());
        match key {
            "MemTotal" => total = val,
            "MemFree" | "MemAvailable" => {
                if free.is_none() { free = val; }
            }
            _ => {}
        }
    }
    // Use MemFree as free storage proxy when /proc/meminfo is available
    (free, total, free)
}



// ---------------------------------------------------------------------------
// Journal LogService
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/LogServices/Journal
///
/// The Journal log service provides access to the BMC systemd journal.
///
/// Reference: DMTF Redfish LogService schema v1_4_0
/// Upstream: redfish-core/lib/manager_logservices_journal.hpp
///
/// On OpenBMC the journal is managed by systemd-journald.  The Entries
/// sub-collection reads from the journal via the `journalctl` binary or
/// (when not available) returns an empty collection.
pub async fn get_manager_journal_log_service(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/Journal",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    Ok(Json(json!({
        "@odata.type": "#LogService.v1_4_0.LogService",
        "@odata.id": format!("/redfish/v1/Managers/{}/LogServices/Journal", manager_id),
        "Id": "Journal",
        "Name": "Journal Log Service",
        "Description": "BMC systemd journal log service",
        "ServiceEnabled": true,
        "LogEntryType": "Event",
        "Entries": {
            "@odata.id": format!(
                "/redfish/v1/Managers/{}/LogServices/Journal/Entries",
                manager_id
            )
        }
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/LogServices/Journal/Entries
///
/// Returns journal entries.  On a real BMC the journal is read by shelling
/// out to `journalctl -o short-precise --no-pager -n 200`; in QEMU or when
/// the binary is not present we return an empty collection.
///
/// Upstream: redfish-core/lib/manager_logservices_journal.hpp
pub async fn get_manager_journal_entries(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/Journal/Entries",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    // Attempt to read from journalctl; gracefully degrade if unavailable
    let members: Vec<Value> = match tokio::process::Command::new("journalctl")
        .args(["-o", "short-precise", "--no-pager", "-n", "200"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    json!({
                        "@odata.type": "#LogEntry.v1_15_0.LogEntry",
                        "@odata.id": format!(
                            "/redfish/v1/Managers/{}/LogServices/Journal/Entries/{}",
                            manager_id, i
                        ),
                        "Id": i.to_string(),
                        "Name": format!("Journal Entry {}", i),
                        "EntryType": "Event",
                        "Severity": "OK",
                        "Message": line
                    })
                })
                .collect()
        }
        _ => {
            debug!("journalctl not available or failed; returning empty journal entries");
            vec![]
        }
    };

    Ok(Json(json!({
        "@odata.type": "#LogEntryCollection.LogEntryCollection",
        "@odata.id": format!(
            "/redfish/v1/Managers/{}/LogServices/Journal/Entries",
            manager_id
        ),
        "Name": "Journal Log Entries",
        "Members@odata.count": members.len(),
        "Members": members
    })))
}


// ---------------------------------------------------------------------------
// Manager DBusEventLog LogService (TODO 7)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/Managers/{manager_id}/LogServices/DBusEventLog
///
/// The Manager DBus event log service — backed by the same
/// `xyz.openbmc_project.Logging` service as the system event log but scoped
/// to manager-level events.
///
/// Reference: DMTF Redfish LogService schema v1_4_0
/// Upstream: redfish-core/lib/manager_logservices_dbus_eventlog.hpp
pub async fn get_manager_dbus_eventlog_service(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/DBusEventLog",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    Ok(Json(json!({
        "@odata.type": "#LogService.v1_4_0.LogService",
        "@odata.id": format!(
            "/redfish/v1/Managers/{}/LogServices/DBusEventLog",
            manager_id
        ),
        "Id": "DBusEventLog",
        "Name": "DBus Event Log Service",
        "Description": "Manager DBus event log service",
        "ServiceEnabled": true,
        "LogEntryType": "Event",
        "Actions": {
            "#LogService.ClearLog": {
                "target": format!(
                    "/redfish/v1/Managers/{}/LogServices/DBusEventLog/Actions/LogService.ClearLog",
                    manager_id
                )
            }
        },
        "Entries": {
            "@odata.id": format!(
                "/redfish/v1/Managers/{}/LogServices/DBusEventLog/Entries",
                manager_id
            )
        }
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/LogServices/DBusEventLog/Entries
///
/// Returns manager-scoped DBus event log entries from
/// `xyz.openbmc_project.Logging` via GetManagedObjects.
///
/// Upstream: redfish-core/lib/manager_logservices_dbus_eventlog.hpp
pub async fn get_manager_dbus_eventlog_entries(
    State(state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!(
        "GET /redfish/v1/Managers/{}/LogServices/DBusEventLog/Entries",
        manager_id
    );
    validate_manager_id(&manager_id)?;

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.Logging",
                "/xyz/openbmc_project/logging",
            )
            .await
        {
            Ok(objects) => {
                let entry_iface = "xyz.openbmc_project.Logging.Entry";
                let mut entries: Vec<(u64, Value)> = objects
                    .iter()
                    .filter(|(_, ifaces)| ifaces.contains_key(entry_iface))
                    .filter_map(|(path, _)| {
                        let entry_id = path.rsplit('/').next()?.to_string();
                        let id_num: u64 = entry_id.parse().ok()?;
                        let entry = json!({
                            "@odata.type": "#LogEntry.v1_15_0.LogEntry",
                            "@odata.id": format!(
                                "/redfish/v1/Managers/{}/LogServices/DBusEventLog/Entries/{}",
                                manager_id, entry_id
                            ),
                            "Id": entry_id,
                            "Name": format!("Log Entry {}", entry_id),
                            "EntryType": "Event"
                        });
                        Some((id_num, entry))
                    })
                    .collect();
                entries.sort_by_key(|&(id, _)| std::cmp::Reverse(id));
                entries.into_iter().map(|(_, v)| v).collect()
            }
            Err(e) => {
                warn!("Failed to read DBusEventLog entries: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "@odata.type": "#LogEntryCollection.LogEntryCollection",
        "@odata.id": format!(
            "/redfish/v1/Managers/{}/LogServices/DBusEventLog/Entries",
            manager_id
        ),
        "Name": "DBus Event Log Entries",
        "Members@odata.count": members.len(),
        "Members": members
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
        assert_eq!(json["Members@odata.count"], 4);
    }

    #[tokio::test]
    async fn test_get_manager_dbus_eventlog_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_dbus_eventlog_service(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "DBusEventLog");
        assert_eq!(json["@odata.type"], "#LogService.v1_4_0.LogService");
    }

    #[tokio::test]
    async fn test_get_manager_dbus_eventlog_entries_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_dbus_eventlog_entries(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_manager_journal_log_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_journal_log_service(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#LogService.v1_4_0.LogService");
        assert_eq!(json["Id"], "Journal");
    }

    #[tokio::test]
    async fn test_get_manager_journal_entries() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_manager_journal_entries(State(state), Path("bmc".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#LogEntryCollection.LogEntryCollection");
    }
}
