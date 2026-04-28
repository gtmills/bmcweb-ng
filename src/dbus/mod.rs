//! DBus abstraction layer
//!
//! Provides a testable interface to DBus services

use async_trait::async_trait;
use anyhow::Result;

/// DBus client trait for dependency injection
#[async_trait]
pub trait DbusClient: Send + Sync {
    /// Get a property from DBus
    async fn get_property<T>(&self, path: &str, interface: &str, property: &str) -> Result<T>
    where
        T: Send;

    /// Call a DBus method
    async fn call_method<T>(&self, path: &str, interface: &str, method: &str) -> Result<T>
    where
        T: Send;
}

// TODO: Implement modules:
// - zbus_impl.rs - Real zbus implementation
// - mock.rs - Mock implementation for testing
