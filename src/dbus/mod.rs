//! DBus abstraction layer
//!
//! Provides a mockable, async interface to OpenBMC DBus services via zbus.
//!
//! # Architecture
//!
//! The [`DbusClient`] trait abstracts all DBus interactions so that the service
//! layer can be unit-tested without a real DBus session.  Two implementations
//! are provided:
//!
//! - [`ZBusClient`] – production implementation backed by zbus
//! - [`MockDbusClient`] – in-process mock for unit tests
//!
//! # OpenBMC DBus Conventions
//!
//! OpenBMC services follow a consistent naming scheme:
//!   - Well-known name:  `xyz.openbmc_project.<service>`
//!   - Object paths:     `/xyz/openbmc_project/<category>/<id>`
//!   - Interfaces:       `xyz.openbmc_project.<category>.<Interface>`
//!
//! Common DBus methods used by Redfish handlers:
//!   - `org.freedesktop.DBus.Properties.Get` — read a single property
//!   - `org.freedesktop.DBus.Properties.Set` — write a single property
//!   - `org.freedesktop.DBus.Properties.GetAll` — read all properties on an interface
//!   - `org.freedesktop.DBus.ObjectManager.GetManagedObjects` — enumerate objects

use async_trait::async_trait;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};
use zbus::names::InterfaceName;
use zbus::zvariant::Optional;

/// A variant value returned from DBus.
///
/// zbus uses `zbus::zvariant::Value` internally; we wrap it in a
/// [`serde_json::Value`] to keep the service layer independent of zbus.
pub type DbusValue = Value;

// ---------------------------------------------------------------------------
// DbusClient trait
// ---------------------------------------------------------------------------

/// Async DBus client trait — the primary abstraction for all DBus interactions.
#[async_trait]
pub trait DbusClient: Send + Sync {
    /// Get a single property from a DBus object.
    ///
    /// # Arguments
    /// * `path`      – DBus object path
    /// * `interface` – DBus interface name
    /// * `property`  – Property name
    async fn get_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
    ) -> Result<DbusValue>;

    /// Set a single property on a DBus object.
    async fn set_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
        value: DbusValue,
    ) -> Result<()>;

    /// Get all properties on an interface.
    async fn get_all_properties(
        &self,
        path: &str,
        interface: &str,
    ) -> Result<HashMap<String, DbusValue>>;

    /// Call a DBus method and return the result as JSON.
    async fn call_method(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        method: &str,
        args: Option<&Value>,
    ) -> Result<DbusValue>;

    /// Enumerate all managed objects under a service root.
    ///
    /// Calls `org.freedesktop.DBus.ObjectManager.GetManagedObjects` on
    /// `destination` at `path`.
    async fn get_managed_objects(
        &self,
        destination: &str,
        path: &str,
    ) -> Result<HashMap<String, HashMap<String, HashMap<String, DbusValue>>>>;
}

// ---------------------------------------------------------------------------
// ZBusClient — production implementation
// ---------------------------------------------------------------------------

/// Production DBus client backed by [zbus](https://docs.rs/zbus/).
pub struct ZBusClient {
    connection: Arc<zbus::Connection>,
}

impl ZBusClient {
    /// Create a new client connected to the system bus.
    pub async fn new() -> Result<Self> {
        let connection = zbus::Connection::system()
            .await
            .context("Failed to connect to the DBus system bus")?;
        Ok(Self {
            connection: Arc::new(connection),
        })
    }

    /// Create a client from an existing connection.
    pub fn from_connection(connection: zbus::Connection) -> Self {
        Self {
            connection: Arc::new(connection),
        }
    }
}

#[async_trait]
impl DbusClient for ZBusClient {
    async fn get_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
    ) -> Result<DbusValue> {
        debug!(
            "DBus GetProperty: path={} interface={} property={}",
            path, interface, property
        );

        let proxy = zbus::fdo::PropertiesProxy::builder(&self.connection)
            .path(path)
            .context("Invalid DBus path")?
            .build()
            .await
            .context("Failed to build Properties proxy")?;

        let iface_name = InterfaceName::try_from(interface)
            .with_context(|| format!("Invalid interface name: {}", interface))?;

        let value = proxy
            .get(iface_name, property)
            .await
            .with_context(|| format!("Failed to get property {}.{} at {}", interface, property, path))?;

        // Convert zvariant::OwnedValue → serde_json::Value via serialization
        let json_value = zvariant_to_json(value.into())
            .with_context(|| format!("Failed to convert DBus value for {}.{}", interface, property))?;

        Ok(json_value)
    }

    async fn set_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
        _value: DbusValue,
    ) -> Result<()> {
        debug!(
            "DBus SetProperty: path={} interface={} property={}",
            path, interface, property
        );

        // TODO: Convert serde_json::Value back to zvariant::Value for the Set call.
        // This requires knowing the D-Bus signature of the property.
        // For now this is a placeholder; real implementations use typed proxies.
        warn!(
            "DBus SetProperty not yet fully implemented for {}.{} at {}",
            interface, property, path
        );
        Err(anyhow!("SetProperty not yet implemented"))
    }

    async fn get_all_properties(
        &self,
        path: &str,
        interface: &str,
    ) -> Result<HashMap<String, DbusValue>> {
        debug!(
            "DBus GetAllProperties: path={} interface={}",
            path, interface
        );

        let proxy = zbus::fdo::PropertiesProxy::builder(&self.connection)
            .path(path)
            .context("Invalid DBus path")?
            .build()
            .await
            .context("Failed to build Properties proxy")?;

        let iface_name = InterfaceName::try_from(interface)
            .with_context(|| format!("Invalid interface name: {}", interface))?;
        let opt_iface: Optional<InterfaceName<'_>> = Optional::from(Some(iface_name));

        let props = proxy
            .get_all(opt_iface)
            .await
            .with_context(|| format!("Failed to get all properties on {} at {}", interface, path))?;

        let mut result = HashMap::new();
        for (key, value) in props {
            match zvariant_to_json(value.into()) {
                Ok(v) => { result.insert(key, v); }
                Err(e) => {
                    warn!("Failed to convert property '{}': {}", key, e);
                }
            }
        }

        Ok(result)
    }

    async fn call_method(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        method: &str,
        _args: Option<&Value>,
    ) -> Result<DbusValue> {
        debug!(
            "DBus call_method: dest={} path={} interface={} method={}",
            destination, path, interface, method
        );

        // Generic method calls require knowing the full signature.
        // Specific callers should use typed zbus proxies generated from
        // introspection XML.  This generic path is a fallback.
        //
        // TODO: Implement a signature-aware generic call path.
        warn!(
            "Generic call_method not fully implemented for {}.{} at {}",
            interface, method, path
        );
        Err(anyhow!("Generic call_method not yet fully implemented; use typed proxies"))
    }

    async fn get_managed_objects(
        &self,
        destination: &str,
        path: &str,
    ) -> Result<HashMap<String, HashMap<String, HashMap<String, DbusValue>>>> {
        debug!(
            "DBus GetManagedObjects: dest={} path={}",
            destination, path
        );

        let proxy = zbus::fdo::ObjectManagerProxy::builder(&self.connection)
            .destination(destination)
            .context("Invalid DBus destination")?
            .path(path)
            .context("Invalid DBus path")?
            .build()
            .await
            .context("Failed to build ObjectManager proxy")?;

        let managed = proxy
            .get_managed_objects()
            .await
            .context("GetManagedObjects call failed")?;

        let mut result: HashMap<String, HashMap<String, HashMap<String, DbusValue>>> =
            HashMap::new();

        for (obj_path, interfaces) in managed {
            let path_str = obj_path.to_string();
            let mut iface_map: HashMap<String, HashMap<String, DbusValue>> = HashMap::new();
            for (iface_name, props) in interfaces {
                let mut prop_map: HashMap<String, DbusValue> = HashMap::new();
                for (prop_name, prop_value) in props {
                    match zvariant_to_json(prop_value.into()) {
                        Ok(v) => { prop_map.insert(prop_name.to_string(), v); }
                        Err(e) => {
                            warn!("Skipping property '{}' on '{}': {}", prop_name, iface_name, e);
                        }
                    }
                }
                iface_map.insert(iface_name.to_string(), prop_map);
            }
            result.insert(path_str, iface_map);
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Type conversion helper
// ---------------------------------------------------------------------------

/// Convert a [`zbus::zvariant::OwnedValue`] to a [`serde_json::Value`].
///
/// zvariant supports a rich type system; we map it to JSON idiomatically.
fn zvariant_to_json(value: zbus::zvariant::OwnedValue) -> Result<Value> {
    // Use the zvariant serialisation route via serde
    let json_value = serde_json::to_value(&value)
        .context("Failed to serialise zvariant::OwnedValue to JSON")?;
    Ok(json_value)
}

// ---------------------------------------------------------------------------
// MockDbusClient — in-memory mock for unit tests
// ---------------------------------------------------------------------------

/// A simple in-memory mock [`DbusClient`] for unit testing.
///
/// Pre-populate it with [`MockDbusClient::set_property`] before running tests.
#[derive(Default, Clone)]
pub struct MockDbusClient {
    properties: Arc<RwLock<HashMap<String, DbusValue>>>,
}

impl MockDbusClient {
    /// Create a new empty mock client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate a property value (call before the code under test runs).
    pub fn set_mock_property(&self, path: &str, interface: &str, property: &str, value: DbusValue) {
        let key = mock_key(path, interface, property);
        self.properties.write().unwrap().insert(key, value);
    }

}

fn mock_key(path: &str, interface: &str, property: &str) -> String {
    format!("{}:{}:{}", path, interface, property)
}

#[async_trait]
impl DbusClient for MockDbusClient {
    async fn get_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
    ) -> Result<DbusValue> {
        let key = mock_key(path, interface, property);
        self.properties
            .read()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("Mock property not found: {}", key))
    }

    async fn set_property(
        &self,
        path: &str,
        interface: &str,
        property: &str,
        value: DbusValue,
    ) -> Result<()> {
        let key = mock_key(path, interface, property);
        self.properties.write().unwrap().insert(key, value);
        Ok(())
    }

    async fn get_all_properties(
        &self,
        path: &str,
        interface: &str,
    ) -> Result<HashMap<String, DbusValue>> {
        let prefix = format!("{}:{}:", path, interface);
        let props = self.properties.read().unwrap();
        let result = props
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix(&prefix)
                    .map(|prop| (prop.to_string(), v.clone()))
            })
            .collect();
        Ok(result)
    }

    async fn call_method(
        &self,
        _destination: &str,
        path: &str,
        interface: &str,
        method: &str,
        _args: Option<&Value>,
    ) -> Result<DbusValue> {
        let key = mock_key(path, interface, method);
        self.properties
            .read()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("Mock method result not found: {}", key))
    }

    async fn get_managed_objects(
        &self,
        _destination: &str,
        _path: &str,
    ) -> Result<HashMap<String, HashMap<String, HashMap<String, DbusValue>>>> {
        // Return an empty map by default; callers can pre-populate via set_mock_property.
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_get_property() {
        let mock = MockDbusClient::new();
        mock.set_mock_property(
            "/xyz/openbmc_project/state/host0",
            "xyz.openbmc_project.State.Host",
            "CurrentHostState",
            json!("xyz.openbmc_project.State.Host.HostState.Running"),
        );

        let result = mock
            .get_property(
                "/xyz/openbmc_project/state/host0",
                "xyz.openbmc_project.State.Host",
                "CurrentHostState",
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            json!("xyz.openbmc_project.State.Host.HostState.Running")
        );
    }

    #[tokio::test]
    async fn test_mock_set_and_get_property() {
        let mock = MockDbusClient::new();
        mock.set_property(
            "/test/path",
            "test.Interface",
            "TestProp",
            json!(42),
        )
        .await
        .unwrap();

        let val = mock
            .get_property("/test/path", "test.Interface", "TestProp")
            .await
            .unwrap();

        assert_eq!(val, json!(42));
    }

    #[tokio::test]
    async fn test_mock_get_property_not_found() {
        let mock = MockDbusClient::new();
        let result = mock
            .get_property("/missing/path", "missing.Interface", "Prop")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_get_all_properties() {
        let mock = MockDbusClient::new();
        mock.set_mock_property("/a/path", "test.Iface", "Prop1", json!("val1"));
        mock.set_mock_property("/a/path", "test.Iface", "Prop2", json!("val2"));

        let all = mock.get_all_properties("/a/path", "test.Iface").await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["Prop1"], json!("val1"));
        assert_eq!(all["Prop2"], json!("val2"));
    }
}
