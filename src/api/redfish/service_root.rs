//! Redfish ServiceRoot endpoint
//!
//! Implements /redfish/v1/ endpoint

use axum::{response::Json, http::StatusCode};
use serde_json::{json, Value};

/// GET /redfish/v1/
pub async fn get_service_root() -> Result<Json<Value>, StatusCode> {
    // TODO: Implement full ServiceRoot response
    let response = json!({
        "@odata.type": "#ServiceRoot.v1_15_0.ServiceRoot",
        "@odata.id": "/redfish/v1/",
        "Id": "RootService",
        "Name": "Root Service",
        "RedfishVersion": "1.15.0",
        "UUID": "00000000-0000-0000-0000-000000000000",
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
        "TaskService": {
            "@odata.id": "/redfish/v1/TaskService"
        },
        "UpdateService": {
            "@odata.id": "/redfish/v1/UpdateService"
        }
    });

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_root() {
        let result = get_service_root().await;
        assert!(result.is_ok());
        
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ServiceRoot.v1_15_0.ServiceRoot");
        assert_eq!(json["RedfishVersion"], "1.15.0");
    }
}
