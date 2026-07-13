//! Redfish CertificateService endpoints
//!
//! Implements:
//! - GET  /redfish/v1/CertificateService
//! - GET  /redfish/v1/CertificateService/CertificateLocations
//! - GET  /redfish/v1/Managers/{manager_id}/NetworkProtocol/HTTPS/Certificates
//! - GET  /redfish/v1/Managers/{manager_id}/NetworkProtocol/HTTPS/Certificates/1
//!
//! On OpenBMC, TLS certificates are managed via:
//!   xyz.openbmc_project.Certs.Manager (service)
//!   /xyz/openbmc_project/certs/server/https (object path for HTTPS cert)
//!
//! Reference: DMTF Redfish CertificateService schema v1.0.4

use axum::{extract::{Path, State}, http::StatusCode, response::Json};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::debug;

use crate::AppState;

/// GET /redfish/v1/CertificateService
pub async fn get_certificate_service(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/CertificateService");

    Ok(Json(json!({
        "@odata.type": "#CertificateService.v1_0_4.CertificateService",
        "@odata.id": "/redfish/v1/CertificateService",
        "Id": "CertificateService",
        "Name": "Certificate Service",
        "Description": "Actions available to manage certificates",
        "ServiceEnabled": true,
        "CertificateLocations": {
            "@odata.id": "/redfish/v1/CertificateService/CertificateLocations"
        },
        "Actions": {
            "#CertificateService.GenerateCSR": {
                "target": "/redfish/v1/CertificateService/Actions/CertificateService.GenerateCSR"
            },
            "#CertificateService.ReplaceCertificate": {
                "target": "/redfish/v1/CertificateService/Actions/CertificateService.ReplaceCertificate"
            }
        }
    })))
}

/// GET /redfish/v1/CertificateService/CertificateLocations
///
/// Lists the certificates installed on this BMC.
/// On OpenBMC, HTTPS server cert lives at:
///   /xyz/openbmc_project/certs/server/https
///   interface: xyz.openbmc_project.Certs.Certificate
pub async fn get_certificate_locations(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/CertificateService/CertificateLocations");

    Ok(Json(json!({
        "@odata.type": "#CertificateLocations.v1_0_2.CertificateLocations",
        "@odata.id": "/redfish/v1/CertificateService/CertificateLocations",
        "Id": "CertificateLocations",
        "Name": "Certificate Locations",
        "Description": "Listing of all certificates and their associated endpoints",
        "Links": {
            "Certificates": [
                {
                    "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol/HTTPS/Certificates/1"
                }
            ],
            "Certificates@odata.count": 1
        }
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/NetworkProtocol/HTTPS/Certificates
pub async fn get_https_certificates_collection(
    State(_state): State<Arc<AppState>>,
    Path(manager_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /Managers/{}/NetworkProtocol/HTTPS/Certificates", manager_id);
    if manager_id != "bmc" {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(json!({
        "@odata.type": "#CertificateCollection.CertificateCollection",
        "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol/HTTPS/Certificates",
        "Name": "HTTPS Certificates Collection",
        "Members@odata.count": 1,
        "Members": [
            { "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol/HTTPS/Certificates/1" }
        ]
    })))
}

/// GET /redfish/v1/Managers/{manager_id}/NetworkProtocol/HTTPS/Certificates/{cert_id}
///
/// Returns the primary HTTPS server certificate.
/// Subject/Issuer/expiry read from DBus `xyz.openbmc_project.Certs.Certificate`
/// on `/xyz/openbmc_project/certs/server/https`.
pub async fn get_https_certificate(
    State(state): State<Arc<AppState>>,
    Path((manager_id, cert_id)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /Managers/{}/NetworkProtocol/HTTPS/Certificates/{}", manager_id, cert_id);
    if manager_id != "bmc" || cert_id != "1" {
        return Err(StatusCode::NOT_FOUND);
    }

    let (subject_cn, issuer_cn, valid_not_after) =
        if let Some(conn) = state.dbus_connection.as_deref() {
            use crate::dbus::{DbusClient, ZBusClient};
            let client = ZBusClient::from_connection(conn.clone());
            let path  = "/xyz/openbmc_project/certs/server/https";
            let iface = "xyz.openbmc_project.Certs.Certificate";

            let raw_subj = client.get_property(path, iface, "Subject").await
                .ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "CN=openbmc".to_string());
            let raw_iss  = client.get_property(path, iface, "Issuer").await
                .ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "CN=openbmc".to_string());
            let exp = client.get_property(path, iface, "ValidNotAfter").await
                .ok().and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "2099-01-01T00:00:00Z".to_string());

            let cn = |s: &str| s.strip_prefix("CN=").unwrap_or(s).to_string();
            (cn(&raw_subj), cn(&raw_iss), exp)
        } else {
            ("openbmc".to_string(), "openbmc".to_string(), "2099-01-01T00:00:00Z".to_string())
        };

    Ok(Json(json!({
        "@odata.type": "#Certificate.v1_8_0.Certificate",
        "@odata.id": "/redfish/v1/Managers/bmc/NetworkProtocol/HTTPS/Certificates/1",
        "Id": "1",
        "Name": "HTTPS Certificate",
        "Description": "HTTPS server certificate for the BMC management interface",
        "CertificateType": "PEM",
        "Subject":  { "CommonName": subject_cn, "Organization": "OpenBMC" },
        "Issuer":   { "CommonName": issuer_cn,  "Organization": "OpenBMC" },
        "ValidNotBefore": "2020-01-01T00:00:00Z",
        "ValidNotAfter":  valid_not_after,
        "KeyUsage": ["DigitalSignature", "KeyEncipherment"],
        "Status": { "State": "Enabled", "Health": "OK" }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_certificate_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_certificate_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "CertificateService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_certificate_locations() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_certificate_locations(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "CertificateLocations");
    }

    #[tokio::test]
    async fn test_https_certificates_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_https_certificates_collection(
            State(state), Path("bmc".to_string())
        ).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_https_certificate() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_https_certificate(
            State(state), Path(("bmc".to_string(), "1".to_string()))
        ).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "1");
        assert_eq!(json["CertificateType"], "PEM");
    }

    #[tokio::test]
    async fn test_https_certificate_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_https_certificate(
            State(state), Path(("bmc".to_string(), "99".to_string()))
        ).await;
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
