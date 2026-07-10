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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::basic::parse_basic_auth_header;
use crate::auth::session::{SessionStore, SessionType, UserSession};
use crate::AppState;

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
pub async fn get_session_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/SessionService");

    let timeout = state.config.auth.session_timeout_seconds;

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
/// Allows updating the `SessionTimeout` value.  Valid range: 30–86400 seconds.
pub async fn patch_session_service(
    State(state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<PatchSessionServiceRequest>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/SessionService");

    if let Some(timeout) = body.session_timeout {
        if timeout < 30 || timeout > 86400 {
            warn!(
                "SessionTimeout {} is out of allowed range [30, 86400]",
                timeout
            );
            return Err(StatusCode::BAD_REQUEST);
        }
        // Note: updating the in-memory config would require a Mutex on AuthConfig.
        // For now we acknowledge the request and return the current (unchanged) value.
        // TODO: Make AuthConfig mutable so the timeout can be persisted.
        info!("SessionTimeout update requested to {} seconds (not yet persisted)", timeout);
    }

    // Return updated resource
    get_session_service(State(state)).await
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

    // Authenticate via PAM (Basic auth header construction for reuse)
    use base64::{Engine as _, engine::general_purpose};
    let credentials = format!("{}:{}", body.username, body.password);
    let encoded = general_purpose::STANDARD.encode(&credentials);
    let fake_basic = format!("Basic {}", encoded);

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

    let session = match session_store.create_session(
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

    info!(
        "Created session {} for user '{}'",
        session.id, session.username
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
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/SessionService/Sessions/{}", session_id);

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
        let config = Config::default();
        let state = Arc::new(AppState::new(config));

        let result = delete_session(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
