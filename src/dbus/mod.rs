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

    /// Resolve which service(s) own a D-Bus object path.
    ///
    /// Calls `xyz.openbmc_project.ObjectMapper.GetObject` with the given
    /// `path` and optional interface filter list.  Returns a map of
    /// `service_name → [interface, ...]` for every service that implements
    /// the object.
    ///
    /// # Arguments
    /// * `path`       – D-Bus object path to resolve
    /// * `interfaces` – If non-empty, only return services that implement at
    ///                  least one of these interfaces.  Pass an empty slice to
    ///                  match any service.
    async fn get_object(
        &self,
        path: &str,
        interfaces: &[&str],
    ) -> Result<HashMap<String, Vec<String>>>;

    /// Enumerate D-Bus objects that implement specified interfaces under a
    /// subtree of the object hierarchy.
    ///
    /// Calls `xyz.openbmc_project.ObjectMapper.GetSubTree`.  Returns a map of
    /// `object_path → { service_name → [interface, ...] }`.
    ///
    /// # Arguments
    /// * `subtree`    – Root path to search under (e.g. `/xyz/openbmc_project`)
    /// * `depth`      – How many path components below `subtree` to recurse.
    ///                  `0` means unlimited depth.
    /// * `interfaces` – Only return objects that implement at least one of
    ///                  these interfaces.  Pass an empty slice to return all.
    async fn get_subtree(
        &self,
        subtree: &str,
        depth: i32,
        interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>>;

    /// Follow a named association link and return associated object paths.
    ///
    /// OpenBMC models relationships between objects via the
    /// `xyz.openbmc_project.Association.Definitions` interface.  The
    /// ObjectMapper exposes resolved association endpoints at synthesised
    /// paths of the form `<object_path>/<association_name>`.
    ///
    /// This method calls `xyz.openbmc_project.ObjectMapper.GetAssociatedSubTree`
    /// to retrieve all objects reachable through the association.  The result
    /// is a map of `object_path → { service_name → [interface, ...] }`,
    /// identical in shape to [`get_subtree`].
    ///
    /// # Arguments
    /// * `association_path` – The synthesised association endpoint, e.g.
    ///   `/xyz/openbmc_project/inventory/system/chassis/sensors` (the chassis
    ///   object path with `/sensors` appended for the "sensors" association).
    /// * `subtree`          – Root path to restrict the search to.
    /// * `depth`            – Recursion depth; `0` means unlimited.
    /// * `interfaces`       – Interface filter; empty slice means no filter.
    async fn get_associated(
        &self,
        association_path: &str,
        subtree: &str,
        depth: i32,
        interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>>;
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
        let json_value = zvariant_to_json(value)
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
            match zvariant_to_json(value) {
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
        if method == "GetUserInfo" {
            if let Ok(map) = reply_msg.body().deserialize::<std::collections::HashMap<String, OwnedValue>>() {
                let mut result = serde_json::Map::new();
                for (key, value) in map {
                    result.insert(key, zvariant_to_json(value)?);
                }
                return Ok(Value::Object(result));
            }
        }

        // We attempt to decode as OwnedValue first, then fall back to null.
        match reply_msg.body().deserialize::<OwnedValue>() {
            Ok(owned) => {
                match zvariant_to_json(owned) {
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
                    match zvariant_to_json(prop_value) {
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

    async fn get_object(
        &self,
        path: &str,
        interfaces: &[&str],
    ) -> Result<HashMap<String, Vec<String>>> {
        debug!("DBus GetObject: path={} interfaces={:?}", path, interfaces);

        // ObjectMapper.GetObject(path: s, interfaces: as) →
        //   dict<service_name: s, interfaces: as>
        let ifaces: Vec<&str> = interfaces.to_vec();
        let reply = self.connection
            .call_method(
                Some("xyz.openbmc_project.ObjectMapper"),
                "/xyz/openbmc_project/object_mapper",
                Some("xyz.openbmc_project.ObjectMapper"),
                "GetObject",
                &(path, ifaces.as_slice()),
            )
            .await
            .with_context(|| format!("ObjectMapper.GetObject failed for path '{}'", path))?;

        // Reply is a(sas) — array of (service, [interface, ...]) pairs
        // which zvariant deserialises as a HashMap<String, Vec<String>>.
        let result: HashMap<String, Vec<String>> = reply
            .body()
            .deserialize()
            .context("Failed to deserialise GetObject reply")?;

        Ok(result)
    }

    async fn get_subtree(
        &self,
        subtree: &str,
        depth: i32,
        interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>> {
        debug!(
            "DBus GetSubTree: subtree={} depth={} interfaces={:?}",
            subtree, depth, interfaces
        );

        // ObjectMapper.GetSubTree(subtree: s, depth: i, interfaces: as) →
        //   dict<path: s, dict<service: s, interfaces: as>>
        let ifaces: Vec<&str> = interfaces.to_vec();
        let reply = self.connection
            .call_method(
                Some("xyz.openbmc_project.ObjectMapper"),
                "/xyz/openbmc_project/object_mapper",
                Some("xyz.openbmc_project.ObjectMapper"),
                "GetSubTree",
                &(subtree, depth, ifaces.as_slice()),
            )
            .await
            .with_context(|| format!("ObjectMapper.GetSubTree failed for subtree '{}'", subtree))?;

        let result: HashMap<String, HashMap<String, Vec<String>>> = reply
            .body()
            .deserialize()
            .context("Failed to deserialise GetSubTree reply")?;

        Ok(result)
    }

    async fn get_associated(
        &self,
        association_path: &str,
        subtree: &str,
        depth: i32,
        interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>> {
        debug!(
            "DBus GetAssociatedSubTree: association_path={} subtree={} depth={} interfaces={:?}",
            association_path, subtree, depth, interfaces
        );

        // ObjectMapper.GetAssociatedSubTree(
        //   association_path: s, subtree: s, depth: i, interfaces: as
        // ) → dict<path: s, dict<service: s, interfaces: as>>
        let ifaces: Vec<&str> = interfaces.to_vec();
        let reply = self.connection
            .call_method(
                Some("xyz.openbmc_project.ObjectMapper"),
                "/xyz/openbmc_project/object_mapper",
                Some("xyz.openbmc_project.ObjectMapper"),
                "GetAssociatedSubTree",
                &(association_path, subtree, depth, ifaces.as_slice()),
            )
            .await
            .with_context(|| format!(
                "ObjectMapper.GetAssociatedSubTree failed for association_path '{}'",
                association_path
            ))?;

        let result: HashMap<String, HashMap<String, Vec<String>>> = reply
            .body()
            .deserialize()
            .context("Failed to deserialise GetAssociatedSubTree reply")?;

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
        Ok(owned) => match zvariant_to_json(owned) {
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
    /// Stores GetObject results keyed by object path.
    objects: Arc<RwLock<HashMap<String, HashMap<String, Vec<String>>>>>,
    /// Stores GetSubTree results keyed by subtree path.
    subtrees: Arc<RwLock<HashMap<String, HashMap<String, HashMap<String, Vec<String>>>>>>,
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


    /// Pre-populate the result returned by [`get_object`] for `path`.
    ///
    /// `service_ifaces` maps service name → list of interfaces, e.g.
    /// `[("xyz.openbmc_project.Inventory.Manager", vec!["xyz.openbmc_project.Inventory.Item"])]`.
    pub fn set_mock_object(&self, path: &str, service_ifaces: HashMap<String, Vec<String>>) {
        self.objects.write().unwrap().insert(path.to_string(), service_ifaces);
    }

    /// Pre-populate the result returned by [`get_subtree`] and
    /// [`get_associated`] for a given subtree/association path.
    ///
    /// `tree` maps object path → service → interfaces.
    pub fn set_mock_subtree(
        &self,
        subtree: &str,
        tree: HashMap<String, HashMap<String, Vec<String>>>,
    ) {
        self.subtrees.write().unwrap().insert(subtree.to_string(), tree);
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

    async fn get_object(
        &self,
        path: &str,
        _interfaces: &[&str],
    ) -> Result<HashMap<String, Vec<String>>> {
        self.objects
            .read()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow!("Mock GetObject not found for path: {}", path))
    }

    async fn get_subtree(
        &self,
        subtree: &str,
        _depth: i32,
        _interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>> {
        Ok(self
            .subtrees
            .read()
            .unwrap()
            .get(subtree)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_associated(
        &self,
        association_path: &str,
        _subtree: &str,
        _depth: i32,
        _interfaces: &[&str],
    ) -> Result<HashMap<String, HashMap<String, Vec<String>>>> {
        // Association endpoints are stored in the same subtrees map, keyed by
        // the association path (e.g. "/chassis/sensors").
        Ok(self
            .subtrees
            .read()
            .unwrap()
            .get(association_path)
            .cloned()
            .unwrap_or_default())
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

    // -----------------------------------------------------------------------
    // get_object
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_get_object_found() {
        let mock = MockDbusClient::new();
        let mut svc_map = HashMap::new();
        svc_map.insert(
            "xyz.openbmc_project.Inventory.Manager".to_string(),
            vec!["xyz.openbmc_project.Inventory.Item.Cpu".to_string()],
        );
        mock.set_mock_object("/xyz/openbmc_project/inventory/cpu0", svc_map.clone());

        let result = mock
            .get_object("/xyz/openbmc_project/inventory/cpu0", &[])
            .await
            .unwrap();

        assert_eq!(result, svc_map);
    }

    #[tokio::test]
    async fn test_mock_get_object_not_found() {
        let mock = MockDbusClient::new();
        let result = mock
            .get_object("/xyz/openbmc_project/inventory/missing", &[])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_get_object_multiple_services() {
        let mock = MockDbusClient::new();
        let mut svc_map = HashMap::new();
        svc_map.insert(
            "xyz.openbmc_project.Inventory.Manager".to_string(),
            vec!["xyz.openbmc_project.Inventory.Item".to_string()],
        );
        svc_map.insert(
            "xyz.openbmc_project.Sensor.Manager".to_string(),
            vec!["xyz.openbmc_project.Sensor.Value".to_string()],
        );
        mock.set_mock_object("/xyz/openbmc_project/inventory/sensor0", svc_map.clone());

        let result = mock
            .get_object("/xyz/openbmc_project/inventory/sensor0", &["xyz.openbmc_project.Sensor.Value"])
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("xyz.openbmc_project.Inventory.Manager"));
        assert!(result.contains_key("xyz.openbmc_project.Sensor.Manager"));
    }

    // -----------------------------------------------------------------------
    // get_subtree
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_get_subtree_found() {
        let mock = MockDbusClient::new();
        let mut tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        let mut svc = HashMap::new();
        svc.insert(
            "xyz.openbmc_project.Inventory.Manager".to_string(),
            vec!["xyz.openbmc_project.Inventory.Item.Drive".to_string()],
        );
        tree.insert("/xyz/openbmc_project/inventory/drive0".to_string(), svc);
        mock.set_mock_subtree("/xyz/openbmc_project/inventory", tree.clone());

        let result = mock
            .get_subtree(
                "/xyz/openbmc_project/inventory",
                0,
                &["xyz.openbmc_project.Inventory.Item.Drive"],
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("/xyz/openbmc_project/inventory/drive0"));
    }

    #[tokio::test]
    async fn test_mock_get_subtree_empty_when_not_configured() {
        let mock = MockDbusClient::new();
        let result = mock
            .get_subtree("/xyz/openbmc_project/inventory", 0, &[])
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_get_subtree_multiple_objects() {
        let mock = MockDbusClient::new();
        let mut tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        for i in 0..3 {
            let mut svc = HashMap::new();
            svc.insert(
                "xyz.openbmc_project.Inventory.Manager".to_string(),
                vec!["xyz.openbmc_project.Inventory.Item.Cpu".to_string()],
            );
            tree.insert(
                format!("/xyz/openbmc_project/inventory/cpu{}", i),
                svc,
            );
        }
        mock.set_mock_subtree("/xyz/openbmc_project/inventory", tree);

        let result = mock
            .get_subtree("/xyz/openbmc_project/inventory", 0, &[])
            .await
            .unwrap();
        assert_eq!(result.len(), 3);
    }

    // -----------------------------------------------------------------------
    // get_associated
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_get_associated_found() {
        let mock = MockDbusClient::new();
        let mut tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        let mut svc = HashMap::new();
        svc.insert(
            "xyz.openbmc_project.Sensor.Manager".to_string(),
            vec!["xyz.openbmc_project.Sensor.Value".to_string()],
        );
        tree.insert("/xyz/openbmc_project/sensors/temperature/cpu0".to_string(), svc);
        // Association path is the chassis object path + "/sensors"
        mock.set_mock_subtree(
            "/xyz/openbmc_project/inventory/system/chassis/sensors",
            tree.clone(),
        );

        let result = mock
            .get_associated(
                "/xyz/openbmc_project/inventory/system/chassis/sensors",
                "/xyz/openbmc_project",
                0,
                &["xyz.openbmc_project.Sensor.Value"],
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("/xyz/openbmc_project/sensors/temperature/cpu0"));
    }

    #[tokio::test]
    async fn test_mock_get_associated_empty_when_not_configured() {
        let mock = MockDbusClient::new();
        let result = mock
            .get_associated(
                "/xyz/openbmc_project/inventory/system/chassis/powered_by",
                "/xyz/openbmc_project",
                0,
                &[],
            )
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_mock_get_associated_independent_of_subtree_param() {
        // get_associated looks up by association_path, not subtree arg — verify
        // two different association names on the same object are independent.
        let mock = MockDbusClient::new();
        let base = "/xyz/openbmc_project/inventory/system/chassis";

        let mut sensors_tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        let mut s1 = HashMap::new();
        s1.insert("svc.Sensor".to_string(), vec!["iface.Sensor".to_string()]);
        sensors_tree.insert("/sensors/temp0".to_string(), s1);
        mock.set_mock_subtree(&format!("{}/sensors", base), sensors_tree);

        let mut fans_tree: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
        let mut s2 = HashMap::new();
        s2.insert("svc.Fan".to_string(), vec!["iface.Fan".to_string()]);
        fans_tree.insert("/sensors/fan0".to_string(), s2);
        mock.set_mock_subtree(&format!("{}/fans", base), fans_tree);

        let sensors = mock
            .get_associated(&format!("{}/sensors", base), base, 0, &[])
            .await
            .unwrap();
        let fans = mock
            .get_associated(&format!("{}/fans", base), base, 0, &[])
            .await
            .unwrap();

        assert_eq!(sensors.len(), 1);
        assert!(sensors.contains_key("/sensors/temp0"));
        assert_eq!(fans.len(), 1);
        assert!(fans.contains_key("/sensors/fan0"));
    }
}
