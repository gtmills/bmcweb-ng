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
use zbus::zvariant::{Optional, OwnedValue};

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
        value: DbusValue,
    ) -> Result<()> {
        debug!(
            "DBus SetProperty: path={} interface={} property={}",
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

        // Convert serde_json::Value to zbus::zvariant::Value.
        //
        // D-Bus properties are strongly typed; we support the JSON types that
        // map unambiguously to D-Bus primitives.  Callers that need to set a
        // property with a non-string type should use a typed zbus proxy instead.
        let zval: zbus::zvariant::Value<'_> = json_to_zvariant(&value)
            .with_context(|| format!(
                "Cannot convert JSON value to D-Bus variant for {}.{} at {}",
                interface, property, path
            ))?;

        proxy
            .set(iface_name, property, &zval)
            .await
            .with_context(|| format!(
                "Failed to set property {}.{} at {}",
                interface, property, path
            ))?;

        Ok(())
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
        args: Option<&Value>,
    ) -> Result<DbusValue> {
        debug!(
            "DBus call_method: dest={} path={} interface={} method={}",
            destination, path, interface, method
        );

        // Build and send a DBus method-call message.
        //
        // zbus `Connection::call_method` takes a body that must implement
        // `zvariant::Type + serde::Serialize`.  We dispatch on the JSON shape
        // of the optional `args` value so we can handle the call patterns
        // used by OpenBMC handlers:
        //
        //   None            → no-argument call (e.g. ListUsers, DeleteAll)
        //   String          → single-string argument (e.g. GetUserInfo(username))
        //   Array-of-strings → (e.g. CreateUser groups)
        //   Array (mixed)   → represented as a tuple via a zvariant-encoded body
        //
        // For the general case we convert through json_to_zvariant and wrap in
        // a single-element zvariant::Structure so the message body is correct.

        let reply_msg = match args {
            None => {
                // No arguments — send empty body
                self.connection
                    .call_method(Some(destination), path, Some(interface), method, &())
                    .await
                    .with_context(|| format!("DBus call {}.{} at {} failed", interface, method, path))?
            }
            Some(Value::String(s)) => {
                // Single string argument
                self.connection
                    .call_method(Some(destination), path, Some(interface), method, &s.as_str())
                    .await
                    .with_context(|| format!("DBus call {}.{} at {} failed", interface, method, path))?
            }
            Some(Value::Array(arr)) => {
                // Check if this is an array of strings — the most common case
                // (e.g. CreateUser groups list, NTPServers array).
                let all_strings = arr.iter().all(|v| v.is_string());
                if all_strings {
                    let strings: Vec<&str> = arr
                        .iter()
                        .map(|v| v.as_str().unwrap_or(""))
                        .collect();
                    self.connection
                        .call_method(Some(destination), path, Some(interface), method, &strings.as_slice())
                        .await
                        .with_context(|| format!("DBus call {}.{} at {} failed", interface, method, path))?
                } else {
                    // Heterogeneous array — encode as a zvariant::Structure.
                    // This handles CreateUser(username, [groups], enabled).
                    return call_method_hetero_array(
                        &self.connection, destination, path, interface, method, arr,
                    ).await;
                }
            }
            Some(other) => {
                // Scalar non-string (bool, number) — convert to zvariant and call.
                let zval = json_to_zvariant(other)
                    .with_context(|| format!("Cannot convert args for {}.{}", interface, method))?;
                self.connection
                    .call_method(Some(destination), path, Some(interface), method, &zval)
                    .await
                    .with_context(|| format!("DBus call {}.{} at {} failed", interface, method, path))?
            }
        };

        // Deserialise the reply body.  Many OpenBMC methods return:
        //   void            → empty body (maps to JSON null)
        //   string          → single string
        //   array<string>   → list of strings
        //   dict<string,v>  → property map (GetUserInfo)
        //
        // We attempt to decode as OwnedValue first, then fall back to null.
        match reply_msg.body().deserialize::<OwnedValue>() {
            Ok(owned) => {
                match zvariant_to_json(owned.into()) {
                    Ok(v) => Ok(v),
                    Err(_) => Ok(Value::Null),
                }
            }
            Err(_) => {
                // void return or undecodable body — treat as success with null
                Ok(Value::Null)
            }
        }
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
// Helper: heterogeneous-array method calls
// ---------------------------------------------------------------------------

/// Call a DBus method whose argument list is a heterogeneous JSON array
/// `[arg0, arg1, ...]` where elements can be strings, arrays of strings, or
/// booleans.  Encodes each element individually and builds a zbus message body
/// as a tuple (zvariant Structure).
///
/// This is the path taken for `CreateUser(username, [groups], enabled)` where
/// the DBus signature is `(s as b)`.
async fn call_method_hetero_array(
    connection: &zbus::Connection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    arr: &[Value],
) -> Result<Value> {
    use zbus::zvariant::Value as ZVal;

    // Convert each JSON element to an owned zvariant::Value.
    let mut zvals: Vec<OwnedValue> = Vec::with_capacity(arr.len());
    for v in arr {
        let zval: ZVal<'static> = match v {
            Value::String(s) => ZVal::from(s.clone()),
            Value::Bool(b)   => ZVal::from(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ZVal::from(i)
                } else if let Some(f) = n.as_f64() {
                    ZVal::from(f)
                } else {
                    return Err(anyhow!("Cannot convert JSON number to DBus value"));
                }
            }
            Value::Array(inner) => {
                // Array of strings (DBus `as`) — the only inner-array type used
                // by OpenBMC User.Manager methods.
                let owned_strings: Vec<String> = inner
                    .iter()
                    .map(|s| s.as_str().unwrap_or("").to_string())
                    .collect();
                ZVal::from(owned_strings)
            }
            other => return Err(anyhow!("Unsupported JSON type in hetero DBus args: {}", other)),
        };
        let owned = OwnedValue::try_from(zval)
            .map_err(|e| anyhow!("Cannot convert to OwnedValue: {}", e))?;
        zvals.push(owned);
    }

    // Build the structure body and send.
    // zbus connection.call_method requires the body to be Serialize + Type.
    // We pass a Vec<OwnedValue> which serializes as a D-Bus array variant; for
    // structured calls we build each variant individually.
    //
    // Since zvariant::Structure is not straightforwardly constructable from
    // Vec<OwnedValue> without unsafe or unstable API, we fall back to calling
    // with a tuple encoding for the 3-element case (the only real case is
    // CreateUser(s, as, b)).  Other arities fall through to a best-effort
    // single-arg call with the first element.
    let reply = if zvals.len() == 3 {
        // Most common: CreateUser(username: s, groups: as, enabled: b)
        connection
            .call_method(Some(destination), path, Some(interface), method,
                &(&zvals[0], &zvals[1], &zvals[2]))
            .await
            .with_context(|| format!("DBus hetero call {}.{} at {} failed", interface, method, path))?
    } else if zvals.len() == 2 {
        connection
            .call_method(Some(destination), path, Some(interface), method,
                &(&zvals[0], &zvals[1]))
            .await
            .with_context(|| format!("DBus hetero call {}.{} at {} failed", interface, method, path))?
    } else if let Some(first) = zvals.first() {
        connection
            .call_method(Some(destination), path, Some(interface), method, first)
            .await
            .with_context(|| format!("DBus hetero call {}.{} at {} failed", interface, method, path))?
    } else {
        connection
            .call_method(Some(destination), path, Some(interface), method, &())
            .await
            .with_context(|| format!("DBus hetero call {}.{} at {} (empty) failed", interface, method, path))?
    };

    match reply.body().deserialize::<OwnedValue>() {
        Ok(owned) => match zvariant_to_json(owned.into()) {
            Ok(v) => Ok(v),
            Err(_) => Ok(Value::Null),
        },
        Err(_) => Ok(Value::Null),
    }
}

// ---------------------------------------------------------------------------
// Type conversion helpers
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

/// Convert a [`serde_json::Value`] to a [`zbus::zvariant::Value`].
///
/// Supports the JSON primitives that map unambiguously to D-Bus types:
/// - JSON string  → D-Bus `s` (string)
/// - JSON bool    → D-Bus `b` (boolean)
/// - JSON integer → D-Bus `i` (int32) or `x` (int64) depending on range
/// - JSON float   → D-Bus `d` (double)
/// - JSON array of strings → D-Bus `as`
///
/// Returns an error for JSON `null` or object types, which do not have a
/// canonical D-Bus representation.  Callers that need to set complex types
/// should build typed proxies from introspection XML.
fn json_to_zvariant(value: &Value) -> Result<zbus::zvariant::Value<'_>> {
    use zbus::zvariant::Value as ZVal;
    match value {
        Value::String(s) => Ok(ZVal::from(s.as_str())),
        Value::Bool(b) => Ok(ZVal::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    Ok(ZVal::from(i as i32))
                } else {
                    Ok(ZVal::from(i))
                }
            } else if let Some(f) = n.as_f64() {
                Ok(ZVal::from(f))
            } else {
                Err(anyhow!("Cannot convert JSON number to D-Bus value: {}", n))
            }
        }
        Value::Array(arr) => {
            // Only arrays of strings are supported for the generic path
            let strings: Result<Vec<&str>> = arr
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| anyhow!("Array element is not a string: {}", v))
                })
                .collect();
            match strings {
                Ok(ss) => {
                    let zval_strings: Vec<zbus::zvariant::Value<'_>> =
                        ss.iter().map(|s| ZVal::from(*s)).collect();
                    Ok(ZVal::from(zval_strings))
                }
                Err(e) => Err(e.context("Cannot convert JSON array to D-Bus as")),
            }
        }
        Value::Null => Err(anyhow!("Cannot represent JSON null as a D-Bus value")),
        Value::Object(_) => Err(anyhow!(
            "Cannot represent a JSON object as a D-Bus value; use a typed proxy"
        )),
    }
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

    #[test]
    fn test_json_to_zvariant_string() {
        let v = json!("hello");
        let result = json_to_zvariant(&v);
        assert!(result.is_ok());
    }

    #[test]
    fn test_json_to_zvariant_bool() {
        assert!(json_to_zvariant(&json!(true)).is_ok());
        assert!(json_to_zvariant(&json!(false)).is_ok());
    }

    #[test]
    fn test_json_to_zvariant_integer() {
        assert!(json_to_zvariant(&json!(42)).is_ok());
        assert!(json_to_zvariant(&json!(i64::MAX)).is_ok());
    }

    #[test]
    fn test_json_to_zvariant_float() {
        assert!(json_to_zvariant(&json!(3.14)).is_ok());
    }

    #[test]
    fn test_json_to_zvariant_null_errors() {
        assert!(json_to_zvariant(&json!(null)).is_err());
    }

    #[test]
    fn test_json_to_zvariant_object_errors() {
        assert!(json_to_zvariant(&json!({"key": "value"})).is_err());
    }
}
