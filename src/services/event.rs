//! Event Service
//!
//! Implements the Redfish EventService for managing event subscriptions.
//! Service-level settings (DeliveryRetryAttempts, DeliveryRetryIntervalSeconds)
//! are stored in-memory via an `RwLock`-protected struct so that PATCH
//! `/redfish/v1/EventService` actually persists within the process lifetime.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

/// Event types supported by the service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// Status change events
    StatusChange,
    /// Resource added events
    ResourceAdded,
    /// Resource removed events
    ResourceRemoved,
    /// Resource updated events
    ResourceUpdated,
    /// Alert events
    Alert,
}

impl EventType {
    /// Convert to Redfish event type string
    pub fn to_redfish_string(&self) -> &'static str {
        match self {
            EventType::StatusChange => "StatusChange",
            EventType::ResourceAdded => "ResourceAdded",
            EventType::ResourceRemoved => "ResourceRemoved",
            EventType::ResourceUpdated => "ResourceUpdated",
            EventType::Alert => "Alert",
        }
    }
}

/// Event destination protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    /// Redfish event protocol
    Redfish,
}

/// Event subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    /// Unique subscription ID
    pub id: String,
    /// Subscription name
    pub name: String,
    /// Destination URL for events
    pub destination: String,
    /// Protocol to use
    pub protocol: Protocol,
    /// Event types to subscribe to
    pub event_types: Vec<EventType>,
    /// Context string to include in events
    pub context: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Whether subscription is enabled
    pub enabled: bool,
}

impl EventSubscription {
    /// Create a new event subscription
    pub fn new(
        name: String,
        destination: String,
        protocol: Protocol,
        event_types: Vec<EventType>,
        context: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            destination,
            protocol,
            event_types,
            context,
            created_at: Utc::now(),
            enabled: true,
        }
    }

    /// Check if subscription is interested in an event type
    pub fn is_interested_in(&self, event_type: EventType) -> bool {
        self.enabled && self.event_types.contains(&event_type)
    }
}

/// Event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMessage {
    /// Event ID
    #[serde(rename = "EventId")]
    pub event_id: String,
    /// Event type
    #[serde(rename = "EventType")]
    pub event_type: String,
    /// Event timestamp
    #[serde(rename = "EventTimestamp")]
    pub event_timestamp: DateTime<Utc>,
    /// Message
    #[serde(rename = "Message")]
    pub message: String,
    /// Message ID
    #[serde(rename = "MessageId")]
    pub message_id: String,
    /// Origin of resource
    #[serde(rename = "OriginOfCondition")]
    pub origin_of_condition: Option<String>,
    /// Severity
    #[serde(rename = "Severity")]
    pub severity: String,
}

impl EventMessage {
    /// Create a new event message
    pub fn new(
        event_type: EventType,
        message: String,
        message_id: String,
        origin: Option<String>,
        severity: String,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            event_type: event_type.to_redfish_string().to_string(),
            event_timestamp: Utc::now(),
            message,
            message_id,
            origin_of_condition: origin,
            severity,
        }
    }
}

/// Event payload sent to subscribers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Redfish context
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    /// Event name
    #[serde(rename = "Name")]
    pub name: String,
    /// Context from subscription
    #[serde(rename = "Context", skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Array of events
    #[serde(rename = "Events")]
    pub events: Vec<EventMessage>,
}

impl EventPayload {
    /// Create a new event payload
    pub fn new(context: Option<String>, events: Vec<EventMessage>) -> Self {
        Self {
            odata_type: "#Event.v1_7_0.Event".to_string(),
            name: "Event Array".to_string(),
            context,
            events,
        }
    }
}

/// Mutable service-level settings for EventService.
///
/// Stored behind an `RwLock` so PATCH `/redfish/v1/EventService` can update
/// them and GET immediately reflects the new values.
#[derive(Debug, Clone)]
pub struct EventServiceSettings {
    /// Whether the event service is enabled.  Redfish default: true.
    pub service_enabled: bool,
    /// Number of delivery retries before giving up.  Redfish default: 3.
    pub delivery_retry_attempts: u32,
    /// Seconds between delivery retries.  Redfish default: 60.
    pub delivery_retry_interval_seconds: u32,
}

impl Default for EventServiceSettings {
    fn default() -> Self {
        Self {
            service_enabled: true,
            delivery_retry_attempts: 3,
            delivery_retry_interval_seconds: 60,
        }
    }
}

/// Event Service for managing subscriptions and dispatching events
#[derive(Debug, Clone)]
pub struct EventService {
    subscriptions: Arc<RwLock<HashMap<String, EventSubscription>>>,
    max_subscriptions: usize,
    /// Mutable service settings (retry policy, etc.)
    settings: Arc<RwLock<EventServiceSettings>>,
}

impl EventService {
    /// Create a new event service
    pub fn new(max_subscriptions: usize) -> Self {
        info!("Initializing Event Service with max {} subscriptions", max_subscriptions);
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            max_subscriptions,
            settings: Arc::new(RwLock::new(EventServiceSettings::default())),
        }
    }

    /// Get a snapshot of the current service settings.
    pub fn get_settings(&self) -> EventServiceSettings {
        self.settings.read().unwrap().clone()
    }

    /// Update service settings.  Only `Some` values are applied.
    ///
    /// Includes `service_enabled` to persist `ServiceEnabled` from
    /// `PATCH /redfish/v1/EventService`.
    pub fn update_settings(
        &self,
        service_enabled: Option<bool>,
        delivery_retry_attempts: Option<u32>,
        delivery_retry_interval_seconds: Option<u32>,
    ) {
        let mut s = self.settings.write().unwrap();
        if let Some(v) = service_enabled {
            info!("EventService: ServiceEnabled updated to {}", v);
            s.service_enabled = v;
        }
        if let Some(v) = delivery_retry_attempts {
            info!("EventService: DeliveryRetryAttempts updated to {}", v);
            s.delivery_retry_attempts = v;
        }
        if let Some(v) = delivery_retry_interval_seconds {
            info!("EventService: DeliveryRetryIntervalSeconds updated to {}", v);
            s.delivery_retry_interval_seconds = v;
        }
    }

    /// Create a new subscription
    pub fn create_subscription(
        &self,
        name: String,
        destination: String,
        protocol: Protocol,
        event_types: Vec<EventType>,
        context: Option<String>,
    ) -> Result<EventSubscription> {
        let mut subscriptions = self.subscriptions.write().unwrap();

        // Check subscription limit
        if subscriptions.len() >= self.max_subscriptions {
            return Err(anyhow!("Maximum number of subscriptions reached"));
        }

        // Create subscription
        let subscription = EventSubscription::new(
            name,
            destination,
            protocol,
            event_types,
            context,
        );

        let id = subscription.id.clone();
        info!("Created event subscription: {} to {}", id, subscription.destination);
        
        subscriptions.insert(id.clone(), subscription.clone());
        Ok(subscription)
    }

    /// Get a subscription by ID
    pub fn get_subscription(&self, id: &str) -> Option<EventSubscription> {
        let subscriptions = self.subscriptions.read().unwrap();
        subscriptions.get(id).cloned()
    }

    /// Get all subscriptions
    pub fn get_all_subscriptions(&self) -> Vec<EventSubscription> {
        let subscriptions = self.subscriptions.read().unwrap();
        subscriptions.values().cloned().collect()
    }

    /// Update a subscription
    pub fn update_subscription(
        &self,
        id: &str,
        enabled: Option<bool>,
    ) -> Result<EventSubscription> {
        let mut subscriptions = self.subscriptions.write().unwrap();
        
        let subscription = subscriptions.get_mut(id)
            .ok_or_else(|| anyhow!("Subscription not found"))?;

        if let Some(enabled_val) = enabled {
            subscription.enabled = enabled_val;
            debug!("Updated subscription {} enabled status to {}", id, enabled_val);
        }

        Ok(subscription.clone())
    }

    /// Delete a subscription
    pub fn delete_subscription(&self, id: &str) -> Result<()> {
        let mut subscriptions = self.subscriptions.write().unwrap();
        
        subscriptions.remove(id)
            .ok_or_else(|| anyhow!("Subscription not found"))?;
        
        info!("Deleted event subscription: {}", id);
        Ok(())
    }

    /// Publish an event to all interested subscribers
    pub async fn publish_event(&self, event: EventMessage) {
        let subscriptions = self.subscriptions.read().unwrap();
        
        // Parse event type
        let event_type = match event.event_type.as_str() {
            "StatusChange" => EventType::StatusChange,
            "ResourceAdded" => EventType::ResourceAdded,
            "ResourceRemoved" => EventType::ResourceRemoved,
            "ResourceUpdated" => EventType::ResourceUpdated,
            "Alert" => EventType::Alert,
            _ => {
                warn!("Unknown event type: {}", event.event_type);
                return;
            }
        };

        // Find interested subscribers
        let interested: Vec<_> = subscriptions
            .values()
            .filter(|sub| sub.is_interested_in(event_type))
            .cloned()
            .collect();

        debug!("Publishing event to {} subscribers", interested.len());

        // Send to each subscriber
        for subscription in interested {
            let payload = EventPayload::new(
                subscription.context.clone(),
                vec![event.clone()],
            );

            // Spawn task to send event (non-blocking)
            let dest = subscription.destination.clone();
            let sub_id = subscription.id.clone();
            
            tokio::spawn(async move {
                if let Err(e) = send_event_to_subscriber(&dest, &payload).await {
                    warn!("Failed to send event to subscription {}: {}", sub_id, e);
                }
            });
        }
    }

    /// Get subscription count
    pub fn subscription_count(&self) -> usize {
        let subscriptions = self.subscriptions.read().unwrap();
        subscriptions.len()
    }
}

/// Send event to a subscriber using a plain hyper HTTP/1.1 POST.
///
/// Uses `hyper` directly (already a dependency) to avoid pulling in `reqwest`
/// and its `openssl-sys` transitive dependency, which breaks cross-compilation
/// to `arm-unknown-linux-gnueabihf` without an ARM OpenSSL sysroot.
///
/// Only plain HTTP destinations are supported here — TLS webhook delivery is
/// a Phase 4 enhancement.  For QEMU smoke testing all destinations are HTTP.
async fn send_event_to_subscriber(destination: &str, payload: &EventPayload) -> Result<()> {
    use http_body_util::Full;
    use hyper::Request;
    use hyper::body::Bytes;
    use hyper_util::rt::TokioIo;
    use tokio::net::TcpStream;

    debug!("Sending event to: {}", destination);

    // Parse destination URL
    let uri: hyper::Uri = destination.parse()
        .map_err(|e| anyhow!("Invalid destination URL '{}': {}", destination, e))?;

    let host = uri.host().ok_or_else(|| anyhow!("No host in destination URL"))?;
    let port = uri.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);
    let path_and_query = uri.path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");

    // Serialise payload
    let body_bytes = serde_json::to_vec(payload)
        .map_err(|e| anyhow!("Failed to serialise event payload: {}", e))?;
    let content_length = body_bytes.len();

    // Connect with a 30-second timeout
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| anyhow!("Connection to {} timed out", addr))?
    .map_err(|e| anyhow!("Failed to connect to {}: {}", addr, e))?;

    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| anyhow!("HTTP handshake failed: {}", e))?;

    // Drive the connection in the background
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            warn!("Event HTTP connection error: {}", e);
        }
    });

    let request = Request::builder()
        .method("POST")
        .uri(path_and_query)
        .header("Host", host)
        .header("Content-Type", "application/json")
        .header("Content-Length", content_length)
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| anyhow!("Failed to build HTTP request: {}", e))?;

    let response = sender
        .send_request(request)
        .await
        .map_err(|e| anyhow!("Failed to send event POST: {}", e))?;

    if response.status().is_success() {
        debug!("Event sent successfully to {} (HTTP {})", destination, response.status());
        Ok(())
    } else {
        Err(anyhow!("Subscriber returned HTTP {}", response.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_subscription_creation() {
        let sub = EventSubscription::new(
            "Test Subscription".to_string(),
            "https://example.com/events".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert, EventType::StatusChange],
            Some("test-context".to_string()),
        );

        assert_eq!(sub.name, "Test Subscription");
        assert_eq!(sub.destination, "https://example.com/events");
        assert!(sub.enabled);
        assert_eq!(sub.event_types.len(), 2);
    }

    #[test]
    fn test_event_service() {
        let service = EventService::new(10);
        
        let sub = service.create_subscription(
            "Test".to_string(),
            "https://example.com/events".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert],
            None,
        ).unwrap();

        assert_eq!(service.subscription_count(), 1);
        
        let retrieved = service.get_subscription(&sub.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test");
    }

    #[test]
    fn test_subscription_limit() {
        let service = EventService::new(2);
        
        service.create_subscription(
            "Sub1".to_string(),
            "https://example.com/1".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert],
            None,
        ).unwrap();

        service.create_subscription(
            "Sub2".to_string(),
            "https://example.com/2".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert],
            None,
        ).unwrap();

        // Third subscription should fail
        let result = service.create_subscription(
            "Sub3".to_string(),
            "https://example.com/3".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert],
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_subscription_deletion() {
        let service = EventService::new(10);
        
        let sub = service.create_subscription(
            "Test".to_string(),
            "https://example.com/events".to_string(),
            Protocol::Redfish,
            vec![EventType::Alert],
            None,
        ).unwrap();

        assert_eq!(service.subscription_count(), 1);
        
        service.delete_subscription(&sub.id).unwrap();
        assert_eq!(service.subscription_count(), 0);
    }
}
