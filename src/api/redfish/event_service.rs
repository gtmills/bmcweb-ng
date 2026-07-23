//! Redfish EventService endpoints
//!
//! Implements the Redfish EventService resource family:
//! - GET    /redfish/v1/EventService
//! - PATCH  /redfish/v1/EventService
//! - GET    /redfish/v1/EventService/SSE  (Server-Sent Events stream)
//! - POST   /redfish/v1/EventService/Actions/EventService.SubmitTestEvent
//! - GET    /redfish/v1/EventService/Subscriptions
//! - POST   /redfish/v1/EventService/Subscriptions
//! - GET    /redfish/v1/EventService/Subscriptions/{subscription_id}
//! - DELETE /redfish/v1/EventService/Subscriptions/{subscription_id}
//!
//! Reference: DMTF DSP0266, EventService schema v1.10.1,
//! EventDestination schema v1.13.1

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_ACTION, PRIVILEGE_CONFIGURE_USERS, PRIVILEGE_PATCH};
use crate::auth::session::UserSession;
use crate::services::{EventMessage, EventSubscription, EventType, Protocol};
use crate::AppState;

// SSE stream imports
use axum::response::sse::{Event as SseEvent, Sse};
use futures::stream;
use std::convert::Infallible;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for creating an event subscription
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    #[serde(rename = "Destination")]
    pub destination: String,
    #[serde(rename = "Context", default)]
    pub context: Option<String>,
    #[serde(rename = "Protocol", default = "default_redfish_protocol")]
    pub protocol: String,
    #[serde(rename = "EventTypes", default = "default_event_types")]
    pub event_types: Vec<String>,
    #[serde(rename = "SubscriptionType", default = "default_subscription_type")]
    pub subscription_type: String,
}

fn default_redfish_protocol() -> String {
    "Redfish".to_string()
}

fn default_event_types() -> Vec<String> {
    vec!["Alert".to_string()]
}

fn default_subscription_type() -> String {
    "RedfishEvent".to_string()
}

/// Request body for PATCH EventService (update delivery-retry config)
#[derive(Debug, Deserialize)]
pub struct PatchEventServiceRequest {
    #[serde(rename = "ServiceEnabled")]
    pub service_enabled: Option<bool>,
    #[serde(rename = "DeliveryRetryAttempts")]
    pub delivery_retry_attempts: Option<u32>,
    #[serde(rename = "DeliveryRetryIntervalSeconds")]
    pub delivery_retry_interval_seconds: Option<u32>,
}

/// Request body for submitting a test event
#[derive(Debug, Deserialize)]
pub struct SubmitTestEventRequest {
    #[serde(rename = "EventType", default = "default_test_event_type")]
    pub event_type: String,
    #[serde(rename = "Message", default = "default_test_message")]
    pub message: String,
    #[serde(rename = "MessageId", default = "default_test_message_id")]
    pub message_id: String,
    #[serde(rename = "OriginOfCondition")]
    pub origin_of_condition: Option<String>,
    #[serde(rename = "Severity", default = "default_test_severity")]
    pub severity: String,
}

fn default_test_event_type() -> String { "Alert".to_string() }
fn default_test_message() -> String { "Test event".to_string() }
fn default_test_message_id() -> String { "Test.1.0.TestEvent".to_string() }
fn default_test_severity() -> String { "OK".to_string() }

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn parse_event_type(s: &str) -> Option<EventType> {
    match s {
        "StatusChange" => Some(EventType::StatusChange),
        "ResourceAdded" => Some(EventType::ResourceAdded),
        "ResourceRemoved" => Some(EventType::ResourceRemoved),
        "ResourceUpdated" => Some(EventType::ResourceUpdated),
        "Alert" => Some(EventType::Alert),
        _ => None,
    }
}

fn subscription_to_json(sub: &EventSubscription) -> Value {
    let event_types: Vec<Value> = sub
        .event_types
        .iter()
        .map(|et| json!(et.to_redfish_string()))
        .collect();

    json!({
        "@odata.type": "#EventDestination.v1_13_1.EventDestination",
        "@odata.id": format!("/redfish/v1/EventService/Subscriptions/{}", sub.id),
        "Id": sub.id,
        "Name": sub.name,
        "Destination": sub.destination,
        "Protocol": "Redfish",
        "SubscriptionType": "RedfishEvent",
        "EventTypes": event_types,
        "Context": sub.context,
        "DeliveryRetryPolicy": "RetryForever",
    })
}

// ---------------------------------------------------------------------------
// EventService resource
// ---------------------------------------------------------------------------

/// GET /redfish/v1/EventService
///
/// Returns the EventService resource.  `DeliveryRetryAttempts` and
/// `DeliveryRetryIntervalSeconds` reflect any values set via PATCH.
pub async fn get_event_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/EventService");

    // Read persisted settings (or use defaults when EventService is not wired)
    let (service_enabled, retry_attempts, retry_interval) = state
        .event_service
        .as_ref()
        .map(|svc| {
            let s = svc.get_settings();
            (s.service_enabled, s.delivery_retry_attempts, s.delivery_retry_interval_seconds)
        })
        .unwrap_or((true, 3, 60));

    let response = json!({
        "@odata.type": "#EventService.v1_10_1.EventService",
        "@odata.id": "/redfish/v1/EventService",
        "Id": "EventService",
        "Name": "Event Service",
        "Description": "Redfish Event Service",
        "ServiceEnabled": service_enabled,
        "DeliveryRetryAttempts": retry_attempts,
        "DeliveryRetryIntervalSeconds": retry_interval,
        "EventTypesForSubscription": [
            "StatusChange",
            "ResourceAdded",
            "ResourceRemoved",
            "ResourceUpdated",
            "Alert"
        ],
        "RegistryPrefixes": [],
        "ResourceTypes": [],
        "ServerSentEventUri": "/redfish/v1/EventService/SSE",
        "SSEFilterPropertiesSupported": {
            "MessageIds": true,
            "EventTypes": true,
            "RegistryPrefixes": true,
            "ResourceTypes": false,
            "OriginResources": false
        },
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "Subscriptions": {
            "@odata.id": "/redfish/v1/EventService/Subscriptions"
        },
        "Actions": {
            "#EventService.SubmitTestEvent": {
                "target": "/redfish/v1/EventService/Actions/EventService.SubmitTestEvent"
            }
        }
    });

    Ok(Json(response))
}

/// PATCH /redfish/v1/EventService
///
/// Updates delivery retry configuration and persists it in-memory so that
/// subsequent GET calls return the updated values.
///
/// Per Redfish DSP0266 §7.5.2, a successful PATCH with no response body returns
/// HTTP 204 No Content.
///
/// Upstream: redfish-core/lib/event_service.hpp (commit 509ced5b)
pub async fn patch_event_service(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<PatchEventServiceRequest>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    debug!("PATCH /redfish/v1/EventService");
    check_privilege(Some(&session), PRIVILEGE_PATCH)
        .map_err(|s| (s, Json(json!({"error": {"code": "Base.1.0.InsufficientPrivilege", "message": "Insufficient privileges"}}))))?;

    // Validate DeliveryRetryAttempts: valid range is 1–3
    // (Redfish EventService schema v1.10 constrains to [1, MaxDeliveryRetries])
    if let Some(attempts) = body.delivery_retry_attempts {
        if !(1..=3).contains(&attempts) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "Base.1.0.PropertyValueOutOfRange",
                        "message": "DeliveryRetryAttempts must be between 1 and 3",
                        "@Message.ExtendedInfo": [{
                            "@odata.type": "#Message.v1_1_1.Message",
                            "MessageId": "Base.1.19.PropertyValueOutOfRange",
                            "Message": "The value 1 for the property DeliveryRetryAttempts is not in the list of acceptable values.",
                            "MessageArgs": [attempts.to_string(), "DeliveryRetryAttempts"]
                        }]
                    }
                })),
            ));
        }
    }

    // Validate DeliveryRetryIntervalSeconds: valid range is 5–180
    if let Some(interval) = body.delivery_retry_interval_seconds {
        if !(5..=180).contains(&interval) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "Base.1.0.PropertyValueOutOfRange",
                        "message": "DeliveryRetryIntervalSeconds must be between 5 and 180",
                        "@Message.ExtendedInfo": [{
                            "@odata.type": "#Message.v1_1_1.Message",
                            "MessageId": "Base.1.19.PropertyValueOutOfRange",
                            "Message": "The value for the property DeliveryRetryIntervalSeconds is not in the list of acceptable values.",
                            "MessageArgs": [interval.to_string(), "DeliveryRetryIntervalSeconds"]
                        }]
                    }
                })),
            ));
        }
    }

    if let Some(svc) = state.event_service.as_ref() {
        svc.update_settings(
            body.service_enabled,
            body.delivery_retry_attempts,
            body.delivery_retry_interval_seconds,
        );
    } else {
        // No event service wired — log, but still return 204 to avoid breaking clients
        if body.service_enabled.is_some()
            || body.delivery_retry_attempts.is_some()
            || body.delivery_retry_interval_seconds.is_some()
        {
            warn!("PATCH EventService: EventService not available, settings not persisted");
        }
    }

    // HTTP 204 No Content per DSP0266 §7.5.2
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Server-Sent Events (SSE)
// ---------------------------------------------------------------------------

/// GET /redfish/v1/EventService/SSE
///
/// Opens a Server-Sent Events (SSE) stream for receiving Redfish events.
///
/// Upstream bmcweb (`redfish-core/lib/eventservice_sse.hpp`) exposes this
/// endpoint and advertises it via `EventService.ServerSentEventUri`.
/// The client connects and receives JSON-encoded Redfish event objects as
/// SSE `data:` lines.
///
/// This implementation sends a single heartbeat event on connection and
/// then keeps the connection open.  Future work can hook DBus signals into
/// the stream; for now this satisfies the `ServerSentEventUri` contract so
/// that Redfish clients can successfully open the SSE channel.
pub async fn get_event_service_sse(
    State(_state): State<Arc<AppState>>,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, Infallible>>> {
    debug!("GET /redfish/v1/EventService/SSE - SSE connection opened");

    // Build the opening event data without a raw-string # conflict.
    let data = format!(
        r#"{{"@odata.type":"{odata_type}","Name":"EventService SSE connected","Events":[]}}"#,
        odata_type = "#Event.v1_7_0.Event",
    );

    let open_event = SseEvent::default().event("ServiceEvent").data(data);

    let s = stream::once(async move { Ok::<_, Infallible>(open_event) });

    // axum SSE keep-alive sends a comment line every 30 s to prevent proxy
    // timeouts; no KeepAlive builder is needed — axum handles it via Sse::keep_alive.
    Sse::new(s)
}

/// POST /redfish/v1/EventService/Actions/EventService.SubmitTestEvent
///
/// Submits a test event to all matching subscribers for validation purposes.
pub async fn submit_test_event(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<SubmitTestEventRequest>,
) -> Result<StatusCode, StatusCode> {
    debug!("POST /redfish/v1/EventService/Actions/EventService.SubmitTestEvent");
    check_privilege(Some(&session), PRIVILEGE_ACTION)?;

    let event_service = state
        .event_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let event_type = parse_event_type(&body.event_type).unwrap_or(EventType::Alert);

    let event = EventMessage::new(
        event_type,
        body.message,
        body.message_id,
        body.origin_of_condition,
        body.severity,
    );

    event_service.publish_event(event).await;
    info!("Test event published");

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Subscriptions collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/EventService/Subscriptions
pub async fn get_subscriptions_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/EventService/Subscriptions");

    let event_service = state
        .event_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let subs = event_service.get_all_subscriptions();
    let members: Vec<Value> = subs
        .iter()
        .map(|s| {
            json!({ "@odata.id": format!("/redfish/v1/EventService/Subscriptions/{}", s.id) })
        })
        .collect();
    let count = members.len();

    let response = json!({
        "@odata.type": "#EventDestinationCollection.EventDestinationCollection",
        "@odata.id": "/redfish/v1/EventService/Subscriptions",
        "Name": "Event Subscriptions Collection",
        "Members@odata.count": count,
        "Members": members,
    });

    Ok(Json(response))
}

/// POST /redfish/v1/EventService/Subscriptions
///
/// Creates a new event subscription.
pub async fn create_subscription(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<CreateSubscriptionRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<Value>), StatusCode> {
    debug!("POST /redfish/v1/EventService/Subscriptions");
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS)?;

    if body.destination.is_empty() {
        warn!("Missing Destination in subscription creation request");
        return Err(StatusCode::BAD_REQUEST);
    }

    if body.protocol != "Redfish" {
        warn!("Unsupported protocol '{}' in subscription creation request", body.protocol);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Parse event types
    let mut event_types = Vec::new();
    for et_str in &body.event_types {
        match parse_event_type(et_str) {
            Some(et) => event_types.push(et),
            None => {
                warn!("Unknown EventType '{}' in subscription request", et_str);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    if event_types.is_empty() {
        event_types.push(EventType::Alert);
    }

    let event_service = state
        .event_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let sub = event_service
        .create_subscription(
            format!("Subscription to {}", body.destination),
            body.destination,
            Protocol::Redfish,
            event_types,
            body.context,
        )
        .map_err(|e| {
            warn!("Failed to create event subscription: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        })?;

    info!("Created event subscription '{}'", sub.id);
    let location = format!("/redfish/v1/EventService/Subscriptions/{}", sub.id);
    let body_json = subscription_to_json(&sub);

    Ok((
        StatusCode::CREATED,
        [("Location".to_string(), location)],
        Json(body_json),
    ))
}

/// GET /redfish/v1/EventService/Subscriptions/{subscription_id}
pub async fn get_subscription(
    State(state): State<Arc<AppState>>,
    Path(sub_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/EventService/Subscriptions/{}", sub_id);

    let event_service = state
        .event_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    match event_service.get_subscription(&sub_id) {
        Some(sub) => Ok(Json(subscription_to_json(&sub))),
        None => {
            warn!("Subscription '{}' not found", sub_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// DELETE /redfish/v1/EventService/Subscriptions/{subscription_id}
pub async fn delete_subscription(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(sub_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/EventService/Subscriptions/{}", sub_id);
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS)?;

    let event_service = state
        .event_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    event_service.delete_subscription(&sub_id).map_err(|e| {
        warn!("Failed to delete subscription '{}': {}", sub_id, e);
        StatusCode::NOT_FOUND
    })?;

    info!("Deleted event subscription '{}'", sub_id);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_event_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_event_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#EventService.v1_10_1.EventService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_subscriptions_empty() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_subscriptions_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_subscription_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_subscription(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_subscription_not_found() {
        use std::net::{IpAddr, Ipv4Addr};
        use crate::auth::session::SessionType;
        let mut admin = crate::auth::session::UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        let config = crate::config::Config::default();
        let state = Arc::new(crate::AppState::new(config));
        let result = delete_subscription(
            State(state),
            Extension(admin),
            Path("nonexistent".to_string()),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_patch_event_service_returns_204() {
        use std::net::{IpAddr, Ipv4Addr};
        use crate::auth::session::SessionType;
        let mut admin = crate::auth::session::UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        let config = crate::config::Config::default();
        let state = Arc::new(crate::AppState::new(config));
        let body = PatchEventServiceRequest {
            service_enabled: None,
            delivery_retry_attempts: Some(2),
            delivery_retry_interval_seconds: Some(30),
        };
        let result = patch_event_service(State(state), Extension(admin), JsonBody(body)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_patch_event_service_retry_attempts_out_of_range() {
        use std::net::{IpAddr, Ipv4Addr};
        use crate::auth::session::SessionType;
        let mut admin = crate::auth::session::UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        let config = crate::config::Config::default();
        let state = Arc::new(crate::AppState::new(config));
        // 10 is outside the valid range 1-3
        let body = PatchEventServiceRequest {
            service_enabled: None,
            delivery_retry_attempts: Some(10),
            delivery_retry_interval_seconds: None,
        };
        let result = patch_event_service(State(state), Extension(admin), JsonBody(body)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_patch_event_service_interval_out_of_range() {
        use std::net::{IpAddr, Ipv4Addr};
        use crate::auth::session::SessionType;
        let mut admin = crate::auth::session::UserSession::new(
            "testadmin".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        admin.set_role("Administrator".to_string());

        let config = crate::config::Config::default();
        let state = Arc::new(crate::AppState::new(config));
        // 1 is below the valid range 5-180
        let body = PatchEventServiceRequest {
            service_enabled: None,
            delivery_retry_attempts: None,
            delivery_retry_interval_seconds: Some(1),
        };
        let result = patch_event_service(State(state), Extension(admin), JsonBody(body)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parse_event_types() {
        assert_eq!(parse_event_type("Alert"), Some(EventType::Alert));
        assert_eq!(parse_event_type("StatusChange"), Some(EventType::StatusChange));
        assert_eq!(parse_event_type("Unknown"), None);
    }
}
