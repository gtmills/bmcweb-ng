//! Redfish ServiceRoot endpoint
//!
//! Implements /redfish/v1/ endpoint

use axum::{extract::State, response::Json, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

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
}
