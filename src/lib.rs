//! bmcweb-ng library
//!
//! Core library for the bmcweb-ng BMC webserver.

pub mod api;
pub mod auth;
pub mod config;
pub mod dbus;
pub mod observability;
pub mod protocol;
pub mod services;

/// Application state shared across handlers
pub struct AppState {
    // TODO: Add shared state fields
}

impl AppState {
    /// Create a new application state
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
