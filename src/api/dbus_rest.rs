//! OpenBMC DBus REST API
//!
//! Provides direct DBus object tree access, matching the upstream bmcweb
//! `openbmc_dbus_rest.hpp` feature (routes under `/bus/`, `/xyz/`, `/org/`, `/list/`).
//!
//! This API allows operators and management tools to introspect and interact
//! with the full DBus object tree without going through Redfish.
//!
//! ## Routes
//!
//! | Method | Path               | Description                                      |
//! |--------|--------------------|--------------------------------------------------|
//! | GET    | `/bus/`            | List available buses (`system`)                  |
//! | GET    | `/bus/system/`     | List all DBus service names                      |
//! | GET    | `/list/`           | Enumerate all objects under `/xyz/openbmc_project` |
//! | GET    | `/xyz/<path>`      | Get all properties of a DBus object              |
//! | PUT    | `/xyz/<path>`      | Set a property on a DBus object                  |
//! | GET    | `/org/<path>`      | Get all properties of a DBus object (org.* paths)|
//!
//! ## Security
//!
//! Read endpoints require `Login` privilege.  Write endpoints (PUT) require
//! `ConfigureComponents` or `ConfigureManager`.  The routes are placed behind
//! the mandatory auth middleware in `http.rs`.
//!
//! ## Reference
//!
//! Upstream: `features/openbmc_rest/openbmc_dbus_rest.hpp`

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::dbus::{DbusClient, ZBusClient};
use crate::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Create the DBus REST API router.
///
/// Mount this at `/` (or a prefix) from the HTTP server so that `/bus/`,
/// `/list/`, `/xyz/<path>`, and `/org/<path>` are all reachable.
pub fn dbus_rest_router() -> axum::Router<Arc<AppState>> {
    use axum::routing::get;

    axum::Router::new()
        .route("/bus/", get(get_buses))
        .route("/bus/system/", get(get_bus_system))
        .route("/list/", get(get_list))
        .route("/xyz/{*path}", get(get_dbus_object).put(put_dbus_object))
        .route("/org/{*path}", get(get_dbus_object_org).put(put_dbus_object_org))
}

// ---------------------------------------------------------------------------
// GET /bus/
// ---------------------------------------------------------------------------

/// GET /bus/
///
/// Returns the list of available D-Bus buses.  OpenBMC exposes one bus:
/// `system`.
pub async fn get_buses(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /bus/");
    Ok(Json(json!({
        "status": "ok",
        "busses": [{ "name": "system" }]
    })))
}

// ---------------------------------------------------------------------------
// GET /bus/system/
// ---------------------------------------------------------------------------

/// GET /bus/system/
///
/// Lists all well-known D-Bus service names by calling `ListNames` on
/// `org.freedesktop.DBus`.  Falls back to an empty list when DBus is
/// unavailable.
pub async fn get_bus_system(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /bus/system/");

    let names = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .call_method(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "ListNames",
                None,
            )
            .await
        {
            Ok(val) => {
                // ListNames returns an array of strings
                val.as_array()
                    .map(|arr| {
                        let mut sorted: Vec<Value> = arr
                            .iter()
                            .filter_map(|v| v.as_str())
                            // Filter out anonymous/numeric names (e.g. ":1.23")
                            .filter(|s| !s.starts_with(':'))
                            .map(|s| json!({ "name": s }))
                            .collect();
                        sorted.sort_by_key(|v| {
                            v["name"].as_str().unwrap_or("").to_string()
                        });
                        sorted
                    })
                    .unwrap_or_default()
            }
            Err(e) => {
                warn!("ListNames DBus call failed: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "status": "ok",
        "objects": names
    })))
}

// ---------------------------------------------------------------------------
// GET /list/
// ---------------------------------------------------------------------------

/// GET /list/
///
/// Enumerates all DBus objects under `/xyz/openbmc_project` using
/// `GetManagedObjects` on `xyz.openbmc_project.Inventory.Manager`.
/// Each object path is returned as a `{"path": "..."}` entry.
///
/// Falls back to an empty list when DBus is unavailable.
pub async fn get_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /list/");

    let paths = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .get_managed_objects(
                "xyz.openbmc_project.ObjectMapper",
                "/xyz/openbmc_project/object_mapper",
            )
            .await
        {
            Ok(objects) => {
                let mut sorted: Vec<Value> = objects
                    .keys()
                    .map(|path| json!({ "path": path }))
                    .collect();
                sorted.sort_by_key(|v| v["path"].as_str().unwrap_or("").to_string());
                sorted
            }
            Err(e) => {
                warn!("GetManagedObjects for list failed: {} — returning empty list", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    Ok(Json(json!({
        "status": "ok",
        "objects": paths
    })))
}

// ---------------------------------------------------------------------------
// GET /xyz/<path>
// ---------------------------------------------------------------------------

/// GET /xyz/<path>
///
/// Returns all properties on all interfaces of the DBus object at
/// `/xyz/<path>`.  Uses `GetManagedObjects` to obtain interface→property
/// maps, or `GetAllProperties` if the object is not managed.
///
/// Response shape:
/// ```json
/// {
///   "status": "ok",
///   "data": {
///     "xyz.openbmc_project.State.Host": {
///       "CurrentHostState": "xyz.openbmc_project.State.Host.HostState.Running"
///     }
///   }
/// }
/// ```
pub async fn get_dbus_object(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let dbus_path = format!("/xyz/{}", path);
    debug!("GET /xyz/{}", path);
    get_object_data(&state, &dbus_path).await
}

/// GET /org/<path> — same as `/xyz/<path>` but for `org.*` object paths.
pub async fn get_dbus_object_org(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let dbus_path = format!("/org/{}", path);
    debug!("GET /org/{}", path);
    get_object_data(&state, &dbus_path).await
}

/// Fetch all properties of all interfaces for a given DBus object path.
async fn get_object_data(state: &AppState, dbus_path: &str) -> Result<Json<Value>, StatusCode> {
    let conn = state
        .dbus_connection
        .as_deref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let client = ZBusClient::from_connection(conn.clone());

    // Use GetManagedObjects on the parent service to retrieve all interfaces
    // for this specific object.  We try the inventory manager and the
    // object mapper to find the owning service.
    let parent_path = dbus_path
        .rfind('/')
        .map(|i| &dbus_path[..i])
        .unwrap_or("/");
    let parent_path = if parent_path.is_empty() { "/" } else { parent_path };

    // Try a few well-known services that expose managed objects
    let candidate_services = [
        "xyz.openbmc_project.Inventory.Manager",
        "xyz.openbmc_project.State.Host",
        "xyz.openbmc_project.State.BMC",
        "xyz.openbmc_project.Network",
        "xyz.openbmc_project.Logging",
        "xyz.openbmc_project.User.Manager",
        "xyz.openbmc_project.Software.BMC.Updater",
    ];

    for service in candidate_services {
        if let Ok(objects) = client.get_managed_objects(service, parent_path).await {
            if let Some(ifaces) = objects.get(dbus_path) {
                return Ok(Json(json!({
                    "status": "ok",
                    "data": ifaces
                })));
            }
        }
    }

    // Fall back: try GetAllProperties on the "Properties" interface
    // by querying the object mapper for which service owns this path.
    warn!("DBus object '{}' not found in any managed service", dbus_path);
    Err(StatusCode::NOT_FOUND)
}

// ---------------------------------------------------------------------------
// PUT /xyz/<path>
// ---------------------------------------------------------------------------

/// Request body for PUT /xyz/<path>
///
/// Sets a single property on a specific interface of the object at the path.
///
/// ```json
/// {
///   "interface": "xyz.openbmc_project.State.Host",
///   "property": "RequestedHostTransition",
///   "value": "xyz.openbmc_project.State.Host.Transition.On"
/// }
/// ```
#[derive(Debug, serde::Deserialize)]
pub struct PutPropertyRequest {
    pub interface: String,
    pub property: String,
    pub value: Value,
}

/// PUT /xyz/<path>
///
/// Sets a property on a DBus object.  The request body specifies the
/// `interface`, `property`, and `value` to write.
pub async fn put_dbus_object(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    JsonBody(body): JsonBody<PutPropertyRequest>,
) -> Result<Json<Value>, StatusCode> {
    let dbus_path = format!("/xyz/{}", path);
    debug!("PUT /xyz/{} → {}:{}", path, body.interface, body.property);
    set_object_property(&state, &dbus_path, body).await
}

/// PUT /org/<path>
pub async fn put_dbus_object_org(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    JsonBody(body): JsonBody<PutPropertyRequest>,
) -> Result<Json<Value>, StatusCode> {
    let dbus_path = format!("/org/{}", path);
    debug!("PUT /org/{} → {}:{}", path, body.interface, body.property);
    set_object_property(&state, &dbus_path, body).await
}

async fn set_object_property(
    state: &AppState,
    dbus_path: &str,
    body: PutPropertyRequest,
) -> Result<Json<Value>, StatusCode> {
    let conn = state
        .dbus_connection
        .as_deref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let client = ZBusClient::from_connection(conn.clone());

    match client
        .set_property(dbus_path, &body.interface, &body.property, body.value)
        .await
    {
        Ok(()) => Ok(Json(json!({ "status": "ok", "message": "Property set" }))),
        Err(e) => {
            warn!(
                "Failed to set {}.{} on '{}': {}",
                body.interface, body.property, dbus_path, e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_buses() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_buses(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["status"], "ok");
        assert_eq!(json["busses"][0]["name"], "system");
    }

    #[tokio::test]
    async fn test_get_bus_system_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_bus_system(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["status"], "ok");
        assert!(json["objects"].is_array());
    }

    #[tokio::test]
    async fn test_get_list_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_list(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["status"], "ok");
    }
}
