//! Redfish CertificateService endpoints
//!
//! Implements:
//! - GET  /redfish/v1/CertificateService
//! - GET  /redfish/v1/CertificateService/CertificateLocations
//!
//! On OpenBMC, TLS certificates are managed via:
//!   xyz.openbmc_project.Certs.Manager (service)
//!   /xyz/openbmc_project/certs/server/https (object path for HTTPS cert)
//!
//! Reference: DMTF Redfish CertificateService schema v1.0.4

use axum::{extract::State, http::StatusCode, response::Json};
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
}
