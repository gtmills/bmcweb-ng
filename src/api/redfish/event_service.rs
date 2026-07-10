//! Redfish EventService endpoints
//!
//! Implements the Redfish EventService resource family:
//! - GET  /redfish/v1/EventService
//! - PATCH /redfish/v1/EventService
//! - POST /redfish/v1/EventService/Actions/EventService.SubmitTestEvent
//! - GET  /redfish/v1/EventService/Subscriptions
//! - POST /redfish/v1/EventService/Subscriptions
//! - GET  /redfish/v1/EventService/Subscriptions/{subscription_id}
//! - DELETE /redfish/v1/EventService/Subscriptions/{subscription_id}
//!
//! Reference: DMTF DSP0266, EventService schema v1.10.1,
//! EventDestination schema v1.13.1

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::services::{EventMessage, EventSubscription, EventType, Protocol};
use crate::AppState;

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
pub async fn get_event_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/EventService");

    let sub_count = state
        .event_service
        .as_ref()
        .map(|s| s.subscription_count())
        .unwrap_or(0);

    let response = json!({
        "@odata.type": "#EventService.v1_10_1.EventService",
        "@odata.id": "/redfish/v1/EventService",
        "Id": "EventService",
        "Name": "Event Service",
        "Description": "Redfish Event Service",
        "ServiceEnabled": true,
        "DeliveryRetryAttempts": 3,
        "DeliveryRetryIntervalSeconds": 60,
        "EventTypesForSubscription": [
            "StatusChange",
            "ResourceAdded",
            "ResourceRemoved",
            "ResourceUpdated",
            "Alert"
        ],
        "RegistryPrefixes": [],
        "ResourceTypes": [],
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
/// Updates delivery retry configuration.
pub async fn patch_event_service(
    State(state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<PatchEventServiceRequest>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/EventService");

    if let Some(attempts) = body.delivery_retry_attempts {
        info!("DeliveryRetryAttempts update to {} (not yet persisted)", attempts);
    }
    if let Some(interval) = body.delivery_retry_interval_seconds {
        info!("DeliveryRetryIntervalSeconds update to {} (not yet persisted)", interval);
    }

    // TODO: persist these settings
    get_event_service(State(state)).await
}

/// POST /redfish/v1/EventService/Actions/EventService.SubmitTestEvent
///
/// Submits a test event to all matching subscribers for validation purposes.
pub async fn submit_test_event(
    State(state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<SubmitTestEventRequest>,
) -> Result<StatusCode, StatusCode> {
    debug!("POST /redfish/v1/EventService/Actions/EventService.SubmitTestEvent");

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
    JsonBody(body): JsonBody<CreateSubscriptionRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    debug!("POST /redfish/v1/EventService/Subscriptions");

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
    let _location = format!("/redfish/v1/EventService/Subscriptions/{}", sub.id);
    let body_json = subscription_to_json(&sub);

    Ok((StatusCode::CREATED, Json(body_json)))
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
    Path(sub_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/EventService/Subscriptions/{}", sub_id);

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
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = delete_subscription(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_parse_event_types() {
        assert_eq!(parse_event_type("Alert"), Some(EventType::Alert));
        assert_eq!(parse_event_type("StatusChange"), Some(EventType::StatusChange));
        assert_eq!(parse_event_type("Unknown"), None);
    }
}
