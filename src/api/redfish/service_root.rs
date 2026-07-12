//! Redfish ServiceRoot endpoint and Registry / JsonSchema collections
//!
//! Implements:
//! - GET /redfish/v1/
//! - GET /redfish/v1/Registries
//! - GET /redfish/v1/Registries/{registry_id}
//! - GET /redfish/v1/JsonSchemas
//! - GET /redfish/v1/JsonSchemas/{schema_id}
//!
//! # Registries
//!
//! The following standard Redfish registries are advertised:
//!   - Base v1.17.0         (DMTF)
//!   - TaskEvent v1.0.3     (DMTF)
//!   - ResourceEvent v1.3.0 (DMTF)
//!   - HeartbeatEvent v1.0.1 (DMTF)
//!   - OpenBMC v1.0.0       (OpenBMC project)
//!
//! Registry URI locations point to the canonical DMTF repository.
//!
//! # JsonSchemas
//!
//! The key Redfish schemas used by the endpoints implemented in bmcweb-ng
//! are advertised.  Schema download URIs point to the DMTF schema repository.

use axum::{extract::{Path, State}, response::Json, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

// ---------------------------------------------------------------------------
// Registry data
// ---------------------------------------------------------------------------

/// A statically defined Redfish Message Registry entry.
struct RegistryDef {
    /// Registry name/prefix (e.g. "Base")
    name: &'static str,
    /// Owning organisation ("DMTF" or "OpenBMC")
    owner: &'static str,
    /// Full schema version (e.g. "1.17.0")
    version: &'static str,
    /// URI to the registry JSON file (canonical DMTF / OpenBMC URL)
    uri: &'static str,
    /// Language (always "en")
    language: &'static str,
}

/// All registries advertised by bmcweb-ng.
const REGISTRIES: &[RegistryDef] = &[
    RegistryDef {
        name:     "Base",
        owner:    "DMTF",
        version:  "1.17.0",
        uri:      "https://redfish.dmtf.org/registries/Base.1.17.0.json",
        language: "en",
    },
    RegistryDef {
        name:     "TaskEvent",
        owner:    "DMTF",
        version:  "1.0.3",
        uri:      "https://redfish.dmtf.org/registries/TaskEvent.1.0.3.json",
        language: "en",
    },
    RegistryDef {
        name:     "ResourceEvent",
        owner:    "DMTF",
        version:  "1.3.0",
        uri:      "https://redfish.dmtf.org/registries/ResourceEvent.1.3.0.json",
        language: "en",
    },
    RegistryDef {
        name:     "HeartbeatEvent",
        owner:    "DMTF",
        version:  "1.0.1",
        uri:      "https://redfish.dmtf.org/registries/HeartbeatEvent.1.0.1.json",
        language: "en",
    },
    RegistryDef {
        name:     "OpenBMC",
        owner:    "OpenBMC",
        version:  "1.0.0",
        uri:      "https://raw.githubusercontent.com/openbmc/bmcweb/main/redfish-core/include/registries/openbmc.json",
        language: "en",
    },
];

// ---------------------------------------------------------------------------
// JsonSchema data
// ---------------------------------------------------------------------------

/// A statically defined Redfish JsonSchema entry.
struct SchemaDef {
    /// Schema identifier (e.g. "ComputerSystem")
    name: &'static str,
    /// Full schema type string (e.g. "#ComputerSystem.v1_20_0.ComputerSystem")
    schema_type: &'static str,
    /// Schema URI at the DMTF repository
    uri: &'static str,
}

/// Key JsonSchemas advertised by bmcweb-ng.
const SCHEMAS: &[SchemaDef] = &[
    SchemaDef { name: "ServiceRoot",     schema_type: "#ServiceRoot.v1_15_0.ServiceRoot",         uri: "https://redfish.dmtf.org/schemas/v1/ServiceRoot.v1_15_0.json" },
    SchemaDef { name: "ComputerSystem",  schema_type: "#ComputerSystem.v1_20_0.ComputerSystem",   uri: "https://redfish.dmtf.org/schemas/v1/ComputerSystem.v1_20_0.json" },
    SchemaDef { name: "Chassis",         schema_type: "#Chassis.v1_23_0.Chassis",                 uri: "https://redfish.dmtf.org/schemas/v1/Chassis.v1_23_0.json" },
    SchemaDef { name: "Manager",         schema_type: "#Manager.v1_19_0.Manager",                 uri: "https://redfish.dmtf.org/schemas/v1/Manager.v1_19_0.json" },
    SchemaDef { name: "SessionService",  schema_type: "#SessionService.v1_0_2.SessionService",    uri: "https://redfish.dmtf.org/schemas/v1/SessionService.v1_0_2.json" },
    SchemaDef { name: "Session",         schema_type: "#Session.v1_7_0.Session",                  uri: "https://redfish.dmtf.org/schemas/v1/Session.v1_7_0.json" },
    SchemaDef { name: "AccountService",  schema_type: "#AccountService.v1_12_0.AccountService",   uri: "https://redfish.dmtf.org/schemas/v1/AccountService.v1_12_0.json" },
    SchemaDef { name: "ManagerAccount",  schema_type: "#ManagerAccount.v1_12_0.ManagerAccount",   uri: "https://redfish.dmtf.org/schemas/v1/ManagerAccount.v1_12_0.json" },
    SchemaDef { name: "Role",            schema_type: "#Role.v1_3_1.Role",                        uri: "https://redfish.dmtf.org/schemas/v1/Role.v1_3_1.json" },
    SchemaDef { name: "EventService",    schema_type: "#EventService.v1_10_1.EventService",       uri: "https://redfish.dmtf.org/schemas/v1/EventService.v1_10_1.json" },
    SchemaDef { name: "EventDestination",schema_type: "#EventDestination.v1_13_1.EventDestination",uri: "https://redfish.dmtf.org/schemas/v1/EventDestination.v1_13_1.json" },
    SchemaDef { name: "TaskService",     schema_type: "#TaskService.v1_2_0.TaskService",          uri: "https://redfish.dmtf.org/schemas/v1/TaskService.v1_2_0.json" },
    SchemaDef { name: "Task",            schema_type: "#Task.v1_8_0.Task",                        uri: "https://redfish.dmtf.org/schemas/v1/Task.v1_8_0.json" },
    SchemaDef { name: "UpdateService",   schema_type: "#UpdateService.v1_14_0.UpdateService",     uri: "https://redfish.dmtf.org/schemas/v1/UpdateService.v1_14_0.json" },
    SchemaDef { name: "SoftwareInventory",schema_type: "#SoftwareInventory.v1_10_0.SoftwareInventory", uri: "https://redfish.dmtf.org/schemas/v1/SoftwareInventory.v1_10_0.json" },
    SchemaDef { name: "LogService",      schema_type: "#LogService.v1_4_0.LogService",            uri: "https://redfish.dmtf.org/schemas/v1/LogService.v1_4_0.json" },
    SchemaDef { name: "LogEntry",        schema_type: "#LogEntry.v1_15_0.LogEntry",               uri: "https://redfish.dmtf.org/schemas/v1/LogEntry.v1_15_0.json" },
    SchemaDef { name: "Processor",       schema_type: "#Processor.v1_20_0.Processor",             uri: "https://redfish.dmtf.org/schemas/v1/Processor.v1_20_0.json" },
    SchemaDef { name: "Memory",          schema_type: "#Memory.v1_18_0.Memory",                   uri: "https://redfish.dmtf.org/schemas/v1/Memory.v1_18_0.json" },
    SchemaDef { name: "EthernetInterface",schema_type: "#EthernetInterface.v1_12_0.EthernetInterface", uri: "https://redfish.dmtf.org/schemas/v1/EthernetInterface.v1_12_0.json" },
    SchemaDef { name: "NetworkProtocol", schema_type: "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol", uri: "https://redfish.dmtf.org/schemas/v1/ManagerNetworkProtocol.v1_9_0.json" },
    SchemaDef { name: "Power",           schema_type: "#Power.v1_7_2.Power",                      uri: "https://redfish.dmtf.org/schemas/v1/Power.v1_7_2.json" },
    SchemaDef { name: "Thermal",         schema_type: "#Thermal.v1_8_0.Thermal",                  uri: "https://redfish.dmtf.org/schemas/v1/Thermal.v1_8_0.json" },
    SchemaDef { name: "Sensor",          schema_type: "#Sensor.v1_9_0.Sensor",                    uri: "https://redfish.dmtf.org/schemas/v1/Sensor.v1_9_0.json" },
    SchemaDef { name: "CertificateService",schema_type: "#CertificateService.v1_0_3.CertificateService", uri: "https://redfish.dmtf.org/schemas/v1/CertificateService.v1_0_3.json" },
    SchemaDef { name: "TelemetryService",schema_type: "#TelemetryService.v1_3_2.TelemetryService",uri: "https://redfish.dmtf.org/schemas/v1/TelemetryService.v1_3_2.json" },
];

/// GET /redfish/v1/
pub async fn get_service_root(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    // Use the persistent system UUID from AppState (stable across restarts)
    let uuid = state.system_uuid.clone();

    let response = json!({
        "@odata.type": "#ServiceRoot.v1_15_0.ServiceRoot",
        "@odata.id": "/redfish/v1",
        "Id": "RootService",
        "Name": "Root Service",
        "RedfishVersion": "1.17.0",
        "UUID": uuid,
        "Systems": {
            "@odata.id": "/redfish/v1/Systems"
        },
        "Chassis": {
            "@odata.id": "/redfish/v1/Chassis"
        },
        "Managers": {
            "@odata.id": "/redfish/v1/Managers"
        },
        "SessionService": {
            "@odata.id": "/redfish/v1/SessionService"
        },
        "AccountService": {
            "@odata.id": "/redfish/v1/AccountService"
        },
        "EventService": {
            "@odata.id": "/redfish/v1/EventService"
        },
        "Tasks": {
            "@odata.id": "/redfish/v1/TaskService"
        },
        "UpdateService": {
            "@odata.id": "/redfish/v1/UpdateService"
        },
        "CertificateService": {
            "@odata.id": "/redfish/v1/CertificateService"
        },
        "TelemetryService": {
            "@odata.id": "/redfish/v1/TelemetryService"
        },
        "Registries": {
            "@odata.id": "/redfish/v1/Registries"
        },
        "JsonSchemas": {
            "@odata.id": "/redfish/v1/JsonSchemas"
        },
        "Cables": {
            "@odata.id": "/redfish/v1/Cables"
        },
        "Fabrics": {
            "@odata.id": "/redfish/v1/Fabrics"
        },
        "Links": {
            "Sessions": {
                "@odata.id": "/redfish/v1/SessionService/Sessions"
            },
            "ManagerProvidingService": {
                "@odata.id": "/redfish/v1/Managers/bmc"
            }
        },
        "ProtocolFeaturesSupported": {
            "ExcerptQuery": false,
            "ExpandQuery": {
                "ExpandAll": false,
                "Levels": false,
                "Links": false,
                "NoLinks": false
            },
            "FilterQuery": false,
            "OnlyMemberQuery": true,
            "SelectQuery": true,
            "DeepOperations": {
                "DeepPOST": false,
                "DeepPATCH": false
            }
        }
    });

    Ok(Json(response))
}

/// GET /redfish/v1/Registries
///
/// Returns the complete Redfish Message Registry collection listing all
/// standard DMTF registries plus the OpenBMC-specific registry.
pub async fn get_registries_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    let members: Vec<Value> = REGISTRIES
        .iter()
        .map(|r| json!({ "@odata.id": format!("/redfish/v1/Registries/{}", r.name) }))
        .collect();
    let count = members.len();

    Ok(Json(json!({
        "@odata.type": "#MessageRegistryFileCollection.MessageRegistryFileCollection",
        "@odata.id": "/redfish/v1/Registries",
        "Name": "MessageRegistryFile Collection",
        "Description": "Collection of Redfish Message Registry Files",
        "Members@odata.count": count,
        "Members": members
    })))
}

/// GET /redfish/v1/Registries/{registry_id}
///
/// Returns a `MessageRegistryFile` resource for the named registry.
/// The `Location` array points to the canonical DMTF / OpenBMC URI where
/// the registry JSON can be downloaded.
pub async fn get_registry(
    State(_state): State<Arc<AppState>>,
    Path(registry_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let reg = REGISTRIES
        .iter()
        .find(|r| r.name == registry_id.as_str())
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({
        "@odata.type": "#MessageRegistryFile.v1_1_0.MessageRegistryFile",
        "@odata.id": format!("/redfish/v1/Registries/{}", reg.name),
        "Id": reg.name,
        "Name": format!("{} Message Registry File", reg.name),
        "Description": format!("{} {} Message Registry File Location", reg.owner, reg.name),
        "Languages": [ reg.language ],
        "Registry": format!("{}.{}", reg.name, reg.version),
        "Location": [
            {
                "Language": reg.language,
                "Uri": reg.uri,
                "PublicationUri": reg.uri,
            }
        ]
    })))
}

/// GET /redfish/v1/JsonSchemas
///
/// Returns the Redfish JsonSchema collection listing the key schemas
/// used by the endpoints implemented in bmcweb-ng.
pub async fn get_json_schemas_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    let members: Vec<Value> = SCHEMAS
        .iter()
        .map(|s| json!({ "@odata.id": format!("/redfish/v1/JsonSchemas/{}", s.name) }))
        .collect();
    let count = members.len();

    Ok(Json(json!({
        "@odata.type": "#JsonSchemaFileCollection.JsonSchemaFileCollection",
        "@odata.id": "/redfish/v1/JsonSchemas",
        "Name": "JsonSchema File Collection",
        "Description": "Collection of Redfish JsonSchema Files",
        "Members@odata.count": count,
        "Members": members
    })))
}

/// GET /redfish/v1/JsonSchemas/{schema_id}
///
/// Returns a `JsonSchemaFile` resource for the named schema.
/// The `Location` array points to the canonical DMTF schema repository URI.
pub async fn get_json_schema(
    State(_state): State<Arc<AppState>>,
    Path(schema_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let schema = SCHEMAS
        .iter()
        .find(|s| s.name == schema_id.as_str())
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({
        "@odata.type": "#JsonSchemaFile.v1_0_2.JsonSchemaFile",
        "@odata.id": format!("/redfish/v1/JsonSchemas/{}", schema.name),
        "Id": schema.name,
        "Name": format!("{} Schema File", schema.name),
        "Description": format!("Redfish {} Schema File", schema.name),
        "Languages": [ "en" ],
        "Schema": schema.schema_type,
        "Location": [
            {
                "Language": "en",
                "Uri": schema.uri,
                "PublicationUri": schema.uri,
            }
        ]
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_service_root() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = get_service_root(State(state)).await;
        assert!(result.is_ok());

        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ServiceRoot.v1_15_0.ServiceRoot");
        assert_eq!(json["RedfishVersion"], "1.17.0");
    }

    #[tokio::test]
    async fn test_registries_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_registries_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        // Should list all 5 known registries
        assert_eq!(json["Members@odata.count"], 5);
        assert!(json["Members"].as_array().unwrap().iter()
            .any(|m| m["@odata.id"] == "/redfish/v1/Registries/Base"));
        assert!(json["Members"].as_array().unwrap().iter()
            .any(|m| m["@odata.id"] == "/redfish/v1/Registries/OpenBMC"));
    }

    #[tokio::test]
    async fn test_get_registry_base() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_registry(State(state), Path("Base".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "Base");
        assert_eq!(json["@odata.type"], "#MessageRegistryFile.v1_1_0.MessageRegistryFile");
        assert!(json["Location"][0]["Uri"].as_str().unwrap().contains("Base.1.17.0"));
    }

    #[tokio::test]
    async fn test_get_registry_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_registry(State(state), Path("Nonexistent".to_string())).await;
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_json_schemas_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_json_schemas_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        let count = json["Members@odata.count"].as_u64().unwrap();
        assert!(count >= 20, "Expected ≥20 schemas, got {}", count);
        assert!(json["Members"].as_array().unwrap().iter()
            .any(|m| m["@odata.id"] == "/redfish/v1/JsonSchemas/ComputerSystem"));
    }

    #[tokio::test]
    async fn test_get_json_schema_computer_system() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_json_schema(State(state), Path("ComputerSystem".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "ComputerSystem");
        assert_eq!(json["@odata.type"], "#JsonSchemaFile.v1_0_2.JsonSchemaFile");
        assert!(json["Location"][0]["Uri"].as_str().unwrap().contains("ComputerSystem"));
    }

    #[tokio::test]
    async fn test_get_json_schema_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_json_schema(State(state), Path("Nonexistent".to_string())).await;
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
