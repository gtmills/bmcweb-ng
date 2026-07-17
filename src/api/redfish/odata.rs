//! Redfish OData service document and $metadata endpoints
//!
//! Implements:
//! - GET /redfish/v1/odata      — OData service document
//! - GET /redfish/v1/$metadata  — OData metadata (CSDL XML)
//!
//! Reference: DMTF DSP0266 §12.6 (OData service document),
//!            OData JSON Format v4.01 §5 (service document),
//!            Redfish Specification DSP0266 §12.6.2
//!
//! Upstream: redfish-core/lib/odata.hpp, redfish-core/lib/metadata.hpp
//!
//! The OData service document lists the top-level Redfish collections as
//! OData singletons / entity sets so that generic OData clients can discover
//! the service.  The $metadata endpoint returns a minimal CSDL XML document
//! pointing to the DMTF schema files; on a real BMC these are served from
//! /usr/share/www/redfish/v1/schema/.

use axum::{extract::State, http::StatusCode, response::Json};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::debug;

use crate::AppState;

// ---------------------------------------------------------------------------
// OData service document
// ---------------------------------------------------------------------------

/// GET /redfish/v1/odata
///
/// Returns the OData service document listing the top-level Redfish singletons
/// and collections.  Required by the Redfish OData conformance profile (§12.6).
///
/// Upstream: redfish-core/lib/odata.hpp `redfishOdataGet`
pub async fn get_odata(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/odata");

    // Top-level Redfish resources as OData service document entries.
    // "kind": "Singleton" for single resources; collections are also listed
    // as "Singleton" because Redfish spec treats collection resources as
    // addressable singletons at their canonical URIs.
    let entries: Vec<Value> = [
        ("$metadata",      "/redfish/v1/$metadata",        "Singleton"),
        ("odata",          "/redfish/v1/odata",            "Singleton"),
        ("JsonSchemas",    "/redfish/v1/JsonSchemas",       "Singleton"),
        ("Service",        "/redfish/v1",                  "Singleton"),
        ("Systems",        "/redfish/v1/Systems",          "Singleton"),
        ("Chassis",        "/redfish/v1/Chassis",          "Singleton"),
        ("Managers",       "/redfish/v1/Managers",         "Singleton"),
        ("SessionService", "/redfish/v1/SessionService",   "Singleton"),
        ("AccountService", "/redfish/v1/AccountService",   "Singleton"),
        ("UpdateService",  "/redfish/v1/UpdateService",    "Singleton"),
        ("EventService",   "/redfish/v1/EventService",     "Singleton"),
        ("TaskService",    "/redfish/v1/TaskService",      "Singleton"),
        ("TelemetryService", "/redfish/v1/TelemetryService", "Singleton"),
        ("AggregationService", "/redfish/v1/AggregationService", "Singleton"),
    ]
    .iter()
    .map(|(name, url, kind)| {
        json!({
            "kind": kind,
            "name": name,
            "url":  url
        })
    })
    .collect();

    Ok(Json(json!({
        "@odata.context": "/redfish/v1/$metadata",
        "value": entries
    })))
}

// ---------------------------------------------------------------------------
// OData $metadata (CSDL XML)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/$metadata
///
/// Returns a minimal OData CSDL XML document referencing the DMTF Redfish
/// schema files.  On a real BMC, bmcweb builds this from the XML files in
/// `/usr/share/www/redfish/v1/schema/`.  We return a static document covering
/// the schemas that bmcweb-ng actually uses.
///
/// Upstream: redfish-core/lib/metadata.hpp `handleMetadataGet`
pub async fn get_metadata(
    State(_state): State<Arc<AppState>>,
) -> axum::response::Response {
    debug!("GET /redfish/v1/$metadata");

    // Minimal CSDL XML covering all schema types used by bmcweb-ng.
    // Real bmcweb builds this dynamically from /usr/share/www XML files;
    // we emit a static document so OData clients can parse the namespace list.
    let schemas: &[(&str, &str)] = &[
        ("ServiceRoot",            "ServiceRoot_v1"),
        ("ComputerSystem",         "ComputerSystem_v1"),
        ("ComputerSystemCollection","ComputerSystemCollection_v1"),
        ("Chassis",                "Chassis_v1"),
        ("ChassisCollection",      "ChassisCollection_v1"),
        ("Manager",                "Manager_v1"),
        ("ManagerCollection",      "ManagerCollection_v1"),
        ("Processor",              "Processor_v1"),
        ("Memory",                 "Memory_v1"),
        ("Storage",                "Storage_v1"),
        ("Drive",                  "Drive_v1"),
        ("StorageController",      "StorageController_v1"),
        ("EthernetInterface",      "EthernetInterface_v1"),
        ("NetworkAdapter",         "NetworkAdapter_v1"),
        ("PCIeDevice",             "PCIeDevice_v1"),
        ("PCIeSlots",              "PCIeSlots_v1"),
        ("Bios",                   "Bios_v1"),
        ("LogService",             "LogService_v1"),
        ("LogEntry",               "LogEntry_v1"),
        ("EventService",           "EventService_v1"),
        ("EventDestination",       "EventDestination_v1"),
        ("SessionService",         "SessionService_v1"),
        ("Session",                "Session_v1"),
        ("AccountService",         "AccountService_v1"),
        ("ManagerAccount",         "ManagerAccount_v1"),
        ("Role",                   "Role_v1"),
        ("UpdateService",          "UpdateService_v1"),
        ("SoftwareInventory",      "SoftwareInventory_v1"),
        ("TaskService",            "TaskService_v1"),
        ("Task",                   "Task_v1"),
        ("TelemetryService",       "TelemetryService_v1"),
        ("MetricDefinition",       "MetricDefinition_v1"),
        ("MetricReport",           "MetricReport_v1"),
        ("MetricReportDefinition", "MetricReportDefinition_v1"),
        ("AggregationService",     "AggregationService_v1"),
        ("Fabric",                 "Fabric_v1"),
        ("Switch",                 "Switch_v1"),
        ("FabricAdapter",          "FabricAdapter_v1"),
        ("Power",                  "Power_v1"),
        ("Thermal",                "Thermal_v1"),
        ("Sensor",                 "Sensor_v1"),
        ("Cable",                  "Cable_v1"),
        ("Assembly",               "Assembly_v1"),
        ("Certificate",            "Certificate_v1"),
        ("CertificateService",     "CertificateService_v1"),
    ];

    let mut refs = String::new();
    for (schema, ver) in schemas {
        refs.push_str(&format!(
            "    <edmx:Reference Uri=\"https://redfish.dmtf.org/schemas/{}.json\">\n\
                     <edmx:Include Namespace=\"{}\"/>\n\
                 </edmx:Reference>\n",
            ver, schema
        ));
    }

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<edmx:Edmx Version="4.0"
    xmlns:edmx="http://docs.oasis-open.org/odata/ns/edmx">
{refs}    <edmx:DataServices>
        <Schema xmlns="http://docs.oasis-open.org/odata/ns/edm"
                Namespace="Service">
            <EntityContainer Name="Service" Extends="ServiceRoot.v1_0_0.ServiceContainer"/>
        </Schema>
    </edmx:DataServices>
</edmx:Edmx>
"#
    );

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/xml")
        .body(axum::body::Body::from(xml))
        .unwrap_or_else(|_| {
            axum::response::Response::builder()
                .status(500)
                .body(axum::body::Body::empty())
                .unwrap()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_odata() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_odata(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.context"], "/redfish/v1/$metadata");
        let value = json["value"].as_array().unwrap();
        assert!(value.len() > 5);
        // Service root must be listed
        assert!(value.iter().any(|v| v["url"] == "/redfish/v1"));
    }

    #[tokio::test]
    async fn test_get_metadata_returns_xml() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let resp = get_metadata(State(state)).await;
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("xml"));
    }
}
