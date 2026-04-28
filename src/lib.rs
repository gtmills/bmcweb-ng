//! bmcweb-ng library
//!
//! Core library for the bmcweb-ng BMC webserver.

use std::sync::Arc;
use zbus::Connection;

pub mod api;
pub mod auth;
pub mod config;
pub mod dbus;
pub mod observability;
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
}

impl AppState {
    /// Create a new application state
    pub fn new(config: config::Config) -> Self {
        Self {
            config: Arc::new(config),
            dbus_connection: None,
            system_uuid: uuid::Uuid::new_v4().to_string(),
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
