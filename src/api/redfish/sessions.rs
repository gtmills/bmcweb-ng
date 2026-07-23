//! Redfish SessionService and Sessions endpoints
//!
//! Implements the Redfish SessionService resource family:
//! - GET  /redfish/v1/SessionService
//! - PATCH /redfish/v1/SessionService
//! - GET  /redfish/v1/SessionService/Sessions
//! - POST /redfish/v1/SessionService/Sessions  (login — no auth required)
//! - GET  /redfish/v1/SessionService/Sessions/{session_id}
//! - DELETE /redfish/v1/SessionService/Sessions/{session_id}
//!
//! Reference: DMTF DSP0266 Redfish Specification, SessionService schema v1.0.2

use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_CONFIGURE_USERS, PRIVILEGE_DELETE_SESSION, PRIVILEGE_PATCH};
use crate::auth::session::{SessionType, UserSession};
use crate::dbus::{DbusClient, ZBusClient};
use crate::AppState;

/// Fetch the Redfish role for a username from DBus.
///
/// Uses `xyz.openbmc_project.User.Manager.GetUserInfo` on the user manager
/// service to retrieve the group membership, which maps to a Redfish role.
///
/// Returns "ReadOnly" on any error so that the session is still usable with
/// minimal access rather than failing entirely.
async fn fetch_user_role(
    state: &AppState,
    username: &str,
) -> String {
    let conn = match state.dbus_connection.as_deref() {
        Some(c) => c,
        None => return "ReadOnly".to_string(),
    };

    let client = ZBusClient::from_connection(conn.clone());

    // xyz.openbmc_project.User.Manager.GetUserInfo returns a dict of user
    // attributes.  The "UserGroups" key contains a list of group strings;
    // the first group that maps to a Redfish role wins.
    //
    // OpenBMC user group → Redfish role mapping:
    //   priv-admin    → Administrator
    //   priv-operator → Operator
    //   priv-user     → ReadOnly
    //   priv-noaccess → NoAccess
    match client
        .call_method(
            "xyz.openbmc_project.User.Manager",
            "/xyz/openbmc_project/user",
            "xyz.openbmc_project.User.Manager",
            "GetUserInfo",
            Some(&serde_json::json!(username)),
        )
        .await
    {
        Ok(info) => {
            // Response is a dict. Prefer UserGroups when present, but some
            // systems expose the values as nested zvariant structures, so fall
            // back to string matching on the JSON representation as needed.
            let info_str = info.to_string();
            if let Some(groups) = info.get("UserGroups").and_then(|v| v.as_array()) {
                for group in groups {
                    let role = match group.as_str().unwrap_or("") {
                        "priv-admin" => Some("Administrator"),
                        "priv-operator" => Some("Operator"),
                        "priv-user" => Some("ReadOnly"),
                        "priv-noaccess" => Some("NoAccess"),
                        _ => None,
                    };
                    if let Some(r) = role {
                        return r.to_string();
                    }
                }
            }
            if let Some(privilege) = info.get("UserPrivilege").and_then(|v| v.as_str()) {
                return match privilege {
                    "priv-admin" => "Administrator",
                    "priv-operator" => "Operator",
                    "priv-user" => "ReadOnly",
                    "priv-noaccess" => "NoAccess",
                    _ => "ReadOnly",
                }
                .to_string();
            }
            if info_str.contains("priv-admin") {
                return "Administrator".to_string();
            }
            if info_str.contains("priv-operator") {
                return "Operator".to_string();
            }
            if info_str.contains("priv-noaccess") {
                return "NoAccess".to_string();
            }
            if info_str.contains("priv-user") {
                return "ReadOnly".to_string();
            }
            warn!("Could not determine role for user '{}' from GetUserInfo response", username);
            "ReadOnly".to_string()
        }
        Err(e) => {
            warn!("GetUserInfo DBus call failed for '{}': {} — defaulting to ReadOnly", username, e);
            "ReadOnly".to_string()
        }
    }
}

/// Request body for creating a new session (POST /redfish/v1/SessionService/Sessions)
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(rename = "UserName")]
    pub username: String,
    #[serde(rename = "Password")]
    pub password: String,
    /// Optional client-supplied context string included in session responses
    #[serde(rename = "Context")]
    pub context: Option<String>,
}

/// Request body for PATCH SessionService (update session timeout)
#[derive(Debug, Deserialize)]
pub struct PatchSessionServiceRequest {
    #[serde(rename = "SessionTimeout")]
    pub session_timeout: Option<u64>,
}

/// Build a Redfish Session JSON object from a [`UserSession`]
fn session_to_json(session: &UserSession) -> Value {
    json!({
        "@odata.type": "#Session.v1_7_0.Session",
        "@odata.id": format!("/redfish/v1/SessionService/Sessions/{}", session.id),
        "Id": session.id,
        "Name": "User Session",
        "Description": "Manager User Session",
        "UserName": session.username,
        "ClientOriginIPAddress": session.client_ip.to_string(),
        "CreatedTime": session.created_at.to_rfc3339(),
    })
}

/// GET /redfish/v1/SessionService
///
/// Returns the SessionService resource describing session management parameters.
/// `SessionTimeout` reflects the live value including any PATCH updates.
pub async fn get_session_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/SessionService");

    // Read the live timeout from the session store (may have been updated via PATCH).
    let timeout = state
        .session_store
        .as_ref()
        .map(|s| s.timeout_seconds() as u64)
        .unwrap_or(state.config.auth.session_timeout_seconds);

    let response = json!({
        "@odata.type": "#SessionService.v1_0_2.SessionService",
        "@odata.id": "/redfish/v1/SessionService",
        "Id": "SessionService",
        "Name": "Session Service",
        "Description": "Session Service",
        "ServiceEnabled": true,
        "SessionTimeout": timeout,
        "Sessions": {
            "@odata.id": "/redfish/v1/SessionService/Sessions"
        }
    });

    Ok(Json(response))
}

/// PATCH /redfish/v1/SessionService
///
/// Updates `SessionTimeout`.  Valid range: 30–86400 seconds.
/// The new value is persisted in the `SessionStore` and immediately reflected
/// in subsequent GET calls and new session expiration calculations.
///
/// On out-of-range values, returns 400 with `PropertyValueOutOfRange` — not
/// `PropertyValueNotInList` which is semantically incorrect for a bounded
/// range.
///
/// Upstream: redfish-core/lib/redfish_sessions.hpp (commit 524de3de)
pub async fn patch_session_service(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<PatchSessionServiceRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    debug!("PATCH /redfish/v1/SessionService");
    check_privilege(Some(&session), PRIVILEGE_PATCH)
        .map_err(|s| (s, Json(json!({"error": {"code": "Base.1.0.InsufficientPrivilege", "message": "Insufficient privileges"}}))))?;

    if let Some(timeout) = body.session_timeout {
        let session_store = state
            .session_store
            .as_ref()
            .ok_or_else(|| (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"code": "Base.1.0.InternalError", "message": "Session store unavailable"}})),
            ))?;

        if session_store.set_timeout_seconds(timeout as i64).is_err() {
            warn!("PATCH SessionService rejected: SessionTimeout {} out of range [30, 86400]", timeout);
            // Use PropertyValueOutOfRange (not PropertyValueNotInList — the value
            // is rejected because it exceeds bounds, not because it is absent from
            // a discrete list).  Upstream fix: commit 524de3de.
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "Base.1.0.PropertyValueOutOfRange",
                        "message": format!(
                            "The value {} for SessionTimeout is out of range [30, 86400].",
                            timeout
                        ),
                        "@Message.ExtendedInfo": [{
                            "@odata.type": "#Message.v1_1_1.Message",
                            "MessageId": "Base.1.19.PropertyValueOutOfRange",
                            "Message": format!(
                                "The value {} for the property SessionTimeout is not in the allowable range of 30 to 86400.",
                                timeout
                            ),
                            "MessageArgs": [timeout.to_string(), "SessionTimeout"]
                        }]
                    }
                })),
            ));
        }

        info!("SessionTimeout updated to {} seconds", timeout);
    }

    get_session_service(State(state)).await
        .map_err(|s| (s, Json(json!({"error": {"code": "Base.1.0.InternalError", "message": "Failed to read session service"}}))))
}

/// GET /redfish/v1/SessionService/Sessions
///
/// Returns the SessionCollection listing all active sessions.
pub async fn get_sessions_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/SessionService/Sessions");

    let session_store = state
        .session_store
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let sessions = session_store.get_all_sessions();
    let members: Vec<Value> = sessions
        .iter()
        .map(|s| {
            json!({
                "@odata.id": format!("/redfish/v1/SessionService/Sessions/{}", s.id)
            })
        })
        .collect();
    let count = members.len();

    let response = json!({
        "@odata.type": "#SessionCollection.SessionCollection",
        "@odata.id": "/redfish/v1/SessionService/Sessions",
        "Name": "Session Collection",
        "Description": "Session Collection",
        "Members@odata.count": count,
        "Members": members,
    });

    Ok(Json(response))
}

/// POST /redfish/v1/SessionService/Sessions
///
/// Creates a new session (login). This endpoint does **not** require prior
/// authentication — it is how credentials are exchanged for a session token.
///
/// On success: 201 Created with `X-Auth-Token` response header and session body.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    JsonBody(body): JsonBody<CreateSessionRequest>,
) -> Response {
    debug!("POST /redfish/v1/SessionService/Sessions for user: {}", body.username);

    if body.username.is_empty() || body.password.is_empty() {
        warn!("Missing username or password in session creation request");
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "Base.1.0.GeneralError",
                    "message": "UserName and Password are required"
                }
            })),
        )
            .into_response();
    }

    // Authenticate via PAM
    match crate::auth::basic::authenticate_with_pam(&body.username, &body.password) {
        Ok(_) => {}
        Err(e) => {
            warn!(
                "Session creation authentication failed for '{}': {}",
                body.username, e
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": {
                        "code": "Base.1.0.NoValidSession",
                        "message": "Invalid username or password"
                    }
                })),
            )
                .into_response();
        }
    }

    let session_store = match state.session_store.as_ref() {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"code": "Base.1.0.InternalError", "message": "Session store unavailable"}})),
            )
                .into_response();
        }
    };

    // Derive client IP
    let client_ip = crate::auth::middleware::extract_client_ip(&headers);

    let mut session = match session_store.create_session(
        body.username.clone(),
        client_ip,
        SessionType::Token,
    ) {
        Some(s) => s,
        None => {
            warn!("Session limit reached, cannot create new session");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": {
                        "code": "Base.1.0.GeneralError",
                        "message": "Maximum session limit reached"
                    }
                })),
            )
                .into_response();
        }
    };

    // Fetch the user's Redfish role from DBus and store it on the session
    let role = fetch_user_role(&state, &body.username).await;
    session_store.set_session_role(&session.id, role.clone());
    session.set_role(role);

    info!(
        "Created session {} for user '{}' with role '{}'",
        session.id, session.username, session.role
    );

    let token = session.token.clone().unwrap_or_default();
    let location = format!("/redfish/v1/SessionService/Sessions/{}", session.id);
    let body_json = session_to_json(&session);

    (
        StatusCode::CREATED,
        [
            ("X-Auth-Token", token.as_str()),
            ("Location", location.as_str()),
        ],
        Json(body_json),
    )
        .into_response()
}

/// GET /redfish/v1/SessionService/Sessions/{session_id}
///
/// Returns information about a specific session.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/SessionService/Sessions/{}", session_id);

    let session_store = state
        .session_store
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    match session_store.get_session(&session_id) {
        Some(session) => Ok(Json(session_to_json(&session))),
        None => {
            warn!("Session '{}' not found", session_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// DELETE /redfish/v1/SessionService/Sessions/{session_id}
///
/// Deletes (terminates) a session. Users may only delete their own sessions
/// unless they hold the `ConfigureUsers` privilege.
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Extension(caller): Extension<UserSession>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/SessionService/Sessions/{}", session_id);
    // ConfigureSelf lets users delete their own session; ConfigureUsers
    // (held by Administrator) lets them delete any session.
    let allowed = {
        let session_store = state.session_store.as_ref()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        // Allow if the caller is deleting their own session
        let is_own = session_store.get_session(&session_id)
            .map(|s| s.username == caller.username)
            .unwrap_or(false);
        is_own || check_privilege(Some(&caller), PRIVILEGE_CONFIGURE_USERS).is_ok()
    };
    if !allowed {
        check_privilege(Some(&caller), PRIVILEGE_DELETE_SESSION)?;
    }

    let session_store = state
        .session_store
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify the session exists before attempting deletion
    if session_store.get_session(&session_id).is_none() {
        warn!("Session '{}' not found for deletion", session_id);
        return Err(StatusCode::NOT_FOUND);
    }

    session_store.delete_session(&session_id);
    info!("Deleted session '{}'", session_id);

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_session_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = get_session_service(State(state)).await;
        assert!(result.is_ok());

        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#SessionService.v1_0_2.SessionService");
        assert_eq!(json["Id"], "SessionService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_sessions_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = get_sessions_collection(State(state)).await;
        assert!(result.is_ok());

        let json = result.unwrap().0;
        assert_eq!(
            json["@odata.type"],
            "#SessionCollection.SessionCollection"
        );
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = get_session(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_session_not_found() {
        use std::net::{IpAddr, Ipv4Addr};
        let mut admin = UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = delete_session(
            State(state),
            Extension(admin),
            Path("nonexistent".to_string()),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_patch_session_service_out_of_range_returns_property_value_out_of_range() {
        use std::net::{IpAddr, Ipv4Addr};

        let config = crate::config::Config::default();
        // AppState::new already initialises the SessionStore from config
        let state = Arc::new(crate::AppState::new(config));

        let mut admin = UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        // Value 10 is below the valid range 30-86400 → must return PropertyValueOutOfRange
        let body = PatchSessionServiceRequest { session_timeout: Some(10) };
        let result = patch_session_service(
            State(state),
            Extension(admin),
            JsonBody(body),
        ).await;
        assert!(result.is_err());
        let (status, json_body) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body.0["error"]["code"],
            "Base.1.0.PropertyValueOutOfRange"
        );
    }
}
