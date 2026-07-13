//! bmcweb-ng library
//!
//! Core library for the bmcweb-ng BMC webserver.

use std::sync::Arc;
use tokio::sync::RwLock;
use zbus::Connection;

pub mod api;
pub mod auth;
pub mod config;
pub mod dbus;
pub mod observability;
pub mod persistent_data;
pub mod protocol;
pub mod services;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Configuration
    pub config: Arc<config::Config>,
    /// DBus connection (optional, may not be available on all platforms)
    pub dbus_connection: Option<Arc<Connection>>,
    /// System UUID (persistent across restarts)
    pub system_uuid: String,
    /// Session store for managing user sessions
    pub session_store: Option<Arc<auth::SessionStore>>,
    /// Metrics collector for Prometheus
    pub metrics: Option<Arc<observability::Metrics>>,
    /// Event service for managing subscriptions
    pub event_service: Option<Arc<services::EventService>>,
    /// Task service for managing long-running operations
    pub task_service: Option<Arc<services::TaskService>>,
    /// Update service for firmware updates
    pub update_service: Option<Arc<services::UpdateService>>,
    /// In-memory Telemetry Triggers (created/deleted via REST)
    pub telemetry_triggers: Arc<RwLock<Vec<serde_json::Value>>>,
}

impl AppState {
    /// Create a new application state
    pub fn new(config: config::Config) -> Self {
        // Create session store from config
        let session_store = Some(Arc::new(auth::SessionStore::new(
            config.auth.session_timeout_seconds,
            config.auth.max_sessions,
        )));

        // Initialize metrics if enabled
        let metrics = if config.metrics.enabled {
            match observability::Metrics::new() {
                Ok(m) => Some(Arc::new(m)),
                Err(e) => {
                    eprintln!("Failed to initialize metrics: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Initialize event service
        let event_service = Some(Arc::new(services::EventService::new(64)));

        // Initialize task service (max 100 tasks, 24 hour retention)
        let task_service = Some(Arc::new(services::TaskService::new(100, 24)));

        // Initialize update service (max 2 concurrent updates)
        let update_service = Some(Arc::new(services::UpdateService::new(2)));

        Self {
            config: Arc::new(config),
            dbus_connection: None,
            system_uuid: uuid::Uuid::new_v4().to_string(),
            session_store,
            metrics,
            event_service,
            task_service,
            update_service,
            telemetry_triggers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set the DBus connection
    pub fn with_dbus(mut self, connection: Connection) -> Self {
        self.dbus_connection = Some(Arc::new(connection));
        self
    }

    /// Set the system UUID
    pub fn with_uuid(mut self, uuid: String) -> Self {
        self.system_uuid = uuid;
        self
    }
}
