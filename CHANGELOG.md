# Changelog

All notable changes to bmcweb-ng will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-07-11

### Added

- **Chassis live inventory from DBus** (`chassis.rs`) —
  `GET /Chassis/{id}` reads `Name`, `Model`, `SerialNumber`, `PartNumber` from
  `xyz.openbmc_project.Inventory.Decorator.Asset` and `Inventory.Item.PrettyName`.
  `IndicatorLED` reads from `xyz.openbmc_project.Led.Physical.State`.
  All `@odata.id` sub-resource links now use the dynamic `chassis_id`.

- **PATCH /Chassis/{id} IndicatorLED** (`chassis.rs`) —
  `PATCH /Chassis/{id}` writes `Asserted` (bool) on
  `xyz.openbmc_project.Led.Group` at `/xyz/openbmc_project/led/groups/front_id`.

- **PowerControl total wattage** (`chassis.rs`) —
  `PowerConsumedWatts` in the `PowerControl` array reads live value from
  `/xyz/openbmc_project/sensors/power/total_power` via `get_property`.

- **System AssetTag/SerialNumber/Model from DBus** (`systems.rs`) —
  `GET /Systems/system` reads `AssetTag` from `Inventory.Decorator.AssetTag`,
  and `SerialNumber`, `PartNumber`, `Model` from `Inventory.Decorator.Asset`.
  `PATCH /Systems/system` applies `AssetTag` changes via `set_property`.

- **FirmwareInventory enriched from DBus** (`update_service.rs`) —
  `GET /UpdateService/FirmwareInventory` enumerates live software images from
  `xyz.openbmc_project.Software.BMC.Updater` using `GetManagedObjects`.
  Results are deduplicated with the in-memory firmware list from upload operations.

- **Storage collection from DBus** (`systems.rs`) —
  `GET /Systems/system/Storage` now enumerates storage controller objects via
  `GetManagedObjects` filtering on `Inventory.Item.StorageController`.
  When no explicit controller objects exist, synthesises one "Storage/1" entry
  if any `Inventory.Item.Drive` objects are present.

- **PATCH EthernetInterface** (`managers.rs`) —
  `PATCH /Managers/bmc/EthernetInterfaces/{nic_id}` handles `DHCPv4.DHCPEnabled`,
  `MACAddress`, and `IPv4StaticAddresses`. Static IPs call
  `xyz.openbmc_project.Network.IP.Create / IP` via `call_method`.
  Returns the updated NIC resource after applying changes.

- **Dynamic NIC validation** (`managers.rs`) —
  `GET /Managers/bmc/EthernetInterfaces/{nic_id}` now validates the NIC id
  against the live DBus NIC list (via `GetManagedObjects`) rather than
  hard-coding `eth0`.

- **Boot override settings** (`systems.rs`) — `GET /Systems/system` now reads
  `BootSource` from `xyz.openbmc_project.Control.Boot.Source` at
  `/xyz/openbmc_project/control/host0/boot` and the one-time-boot path.
  `BootSourceOverrideEnabled`, `BootSourceOverrideTarget`, and `BootSourceOverrideMode`
  are populated from live DBus values instead of static defaults.

- **PATCH /Systems/system** (`systems.rs`) — New handler allows setting
  `Boot.BootSourceOverrideTarget` and `Boot.BootSourceOverrideEnabled` via DBus
  `set_property` on the persistent boot path and one-time-boot path.

- **EventLog Entries collection** (`systems.rs`) —
  `GET /Systems/system/LogServices/EventLog/Entries` reads all log entries from
  `xyz.openbmc_project.Logging` via `GetManagedObjects`, returning them newest-first
  with `Severity`, `Created`, and `Message` fields.

- **EventLog Entry instance** (`systems.rs`) —
  `GET /Systems/system/LogServices/EventLog/Entries/{entry_id}` reads a single log
  entry from `/xyz/openbmc_project/logging/entry/{N}` via `get_all_properties`.

- **ClearLog action** (`systems.rs`) —
  `POST /Systems/system/LogServices/EventLog/Actions/LogService.ClearLog` calls
  `xyz.openbmc_project.Collection.DeleteAll / DeleteAll` to flush all log entries.

- **PATCH NetworkProtocol fully wired** (`managers.rs`) —
  `PATCH /Managers/bmc/NetworkProtocol` now applies `HostName` and `NTP.NTPServers`
  changes via `set_property` on `xyz.openbmc_project.Network.SystemConfiguration`.
  Returns the updated resource after applying changes.

- **AccountService DBus wiring** (`accounts.rs`) —
  - `GET /AccountService/Accounts` now calls `xyz.openbmc_project.User.Manager / ListUsers`
    to enumerate all BMC users dynamically. Falls back to static `root` entry when DBus is
    unavailable.
  - `POST /AccountService/Accounts` calls `CreateUser(username, [priv-group, ssh], enabled)`
    to create a real system user. Returns HTTP 500 if DBus call fails.
  - `GET /AccountService/Accounts/{id}` calls `GetUserInfo(username)` to read `UserPrivilege`,
    `UserEnabled`, and `UserLockedForFailedAttempt`. Falls back to static `root` data for
    unknown users when DBus is unavailable.
  - `PATCH /AccountService/Accounts/{id}` applies `RoleId` and `Enabled` changes via
    `set_property` on `xyz.openbmc_project.User.Attributes`.
  - `DELETE /AccountService/Accounts/{id}` calls `DeleteUser(username)`. Deleting `root`
    is still forbidden (returns HTTP 403).
  - Added `openbmc_priv_to_role()` helper mapping OpenBMC priv groups to Redfish roles.

- **Chassis Power DBus wiring** (`chassis.rs`) — `GET /Chassis/chassis/Power` now queries
  `GetManagedObjects` for objects under `/xyz/openbmc_project/sensors/voltage/` (voltage
  sensors, `Sensor.Value` interface) and inventory objects with
  `Inventory.Item.PowerSupply`. `Voltages` and `PowerSupplies` arrays are populated from
  live DBus data; fall back to empty arrays when DBus is unavailable.

- **Chassis Thermal DBus wiring** (`chassis.rs`) — `GET /Chassis/chassis/Thermal` now
  queries `GetManagedObjects` on the sensor service for temperature sensors
  (`/sensors/temperature/`) and fan sensors (`/sensors/fan*`). `Temperatures` and `Fans`
  arrays include `ReadingCelsius`/`Reading` values, threshold properties, and `ReadingUnits`.
  Falls back to empty arrays when DBus is unavailable.

- **Chassis Sensors DBus enumeration** (`chassis.rs`) — `GET /Chassis/chassis/Sensors` now
  enumerates all objects with `Sensor.Value` interface under `/xyz/openbmc_project/sensors/`
  and returns a `SensorCollection` with `Members@odata.count` reflecting the live count.
  Sensor IDs are derived from the last two path segments (e.g. `temperature_ambient`).

- **BMC reset DBus wiring** (`managers.rs`) — `POST /Managers/bmc/Actions/Manager.Reset`
  now writes `RequestedBMCTransition` on `xyz.openbmc_project.State.BMC` at
  `/xyz/openbmc_project/state/bmc0`. `GracefulRestart` maps to `Transition.Reboot`;
  `ForceRestart` maps to `Transition.HardReboot`. Returns 204 even if DBus is unavailable
  (logs a warning).

- **System reset DBus wiring** (`systems.rs`) — `POST /Systems/system/Actions/ComputerSystem.Reset`
  now writes `RequestedHostTransition` or `RequestedPowerTransition` to the appropriate
  DBus state object. Full ResetType → DBus transition mapping:
  `On`/`ForceOn` → `Host.Transition.On`, `ForceOff` → `Chassis.Transition.Off`,
  `GracefulShutdown` → `Host.Transition.Off`, `GracefulRestart` → `Host.Transition.Reboot`,
  `ForceRestart` → `Host.Transition.ForceWarmReboot`, `Nmi` → `Host.Transition.DiagnosticMode`.

- **BMC NIC enumeration** (`managers.rs`) — `GET /Managers/bmc/EthernetInterfaces`
  now calls `GetManagedObjects` on `xyz.openbmc_project.Network` to enumerate all
  interfaces implementing `xyz.openbmc_project.Network.EthernetInterface`. Falls back to
  a single `eth0` entry when DBus is unavailable or returns an empty set.

### Changed

- `accounts.rs`: removed hard-coded `root`-only restriction from `get_account()`; now
  any account present in DBus can be retrieved.
- `chassis.rs`: Power, Thermal, and Sensors endpoints now use dynamic `@odata.id` based on
  `chassis_id` parameter instead of hard-coded `"/redfish/v1/Chassis/chassis/..."`.
- `managers.rs`: EthernetInterfaces collection count is now dynamic (reflects live NIC count).

### Tests

- 115 unit tests passing (up from 112); new tests for account operations (no-DBus fallback,
  delete-root-forbidden, delete-unknown, `openbmc_priv_to_role` mapping).

---

- **DBus PowerState wiring** (`systems.rs`) — `GET /redfish/v1/Systems/system`
  now queries `xyz.openbmc_project.State.Host / CurrentHostState` at
  `/xyz/openbmc_project/state/host0`. Maps `HostState.Running`, `Quiesced`, and
  `DiagnosticMode` to `"On"`; `HostState.Off` to `"Off"`; unknown states to
  `"Unknown"`. Gracefully falls back to `"Unknown"` when DBus is unavailable.

- **DBus FirmwareVersion wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc`
  now queries `xyz.openbmc_project.Software.Version / Version` on the active BMC
  image object at `/xyz/openbmc_project/software/active`. Falls back to `"Unknown"`.

- **DBus hostname + NTP wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc/NetworkProtocol`
  now queries `xyz.openbmc_project.Network.SystemConfiguration / HostName` and
  `NTPServers` from `/xyz/openbmc_project/network/config`. `HostName` and `FQDN`
  fields reflect the live hostname; `NTP.NTPServers` contains the current NTP list.

- **DBus MAC/IP wiring** (`managers.rs`) — `GET /redfish/v1/Managers/bmc/EthernetInterfaces/eth0`
  now queries `xyz.openbmc_project.Network.EthernetInterface / MACAddress`,
  `IPv4Addresses`, and `IPv6Addresses` from `/xyz/openbmc_project/network/eth0`.
  Falls back to `00:00:00:00:00:00` / empty lists when DBus is unavailable.

- **Role-based session support** (`auth/session.rs`, `auth/privilege.rs`) — `UserSession`
  now carries a `role: String` field (default `"ReadOnly"`), set at session-creation
  time via `set_role()`. `session_role()` in `privilege.rs` reads the stored role
  instead of hard-coding `"ReadOnly"`. `SessionStore::set_session_role()` allows
  updating the persisted role in-store after creation. New tests confirm Administrator
  can configure, ReadOnly cannot, and that the stored role is correctly used.

- **DBus role lookup on login** (`sessions.rs`) — `POST /redfish/v1/SessionService/Sessions`
  now calls `xyz.openbmc_project.User.Manager / GetUserInfo` after PAM authentication
  to look up the user's `priv-admin`, `priv-operator`, `priv-user`, or `priv-noaccess`
  group, mapping it to a Redfish role (`Administrator`, `Operator`, `ReadOnly`,
  `NoAccess`). Falls back to `"ReadOnly"` if DBus is unavailable or call fails.

- **LogServices/EventLog instance endpoint** (`systems.rs`, `mod.rs`) —
  `GET /redfish/v1/Systems/system/LogServices/EventLog` now returns a fully-formed
  `LogService.v1_4_0.LogService` resource with `Entries` link and `ClearLog` action
  target. Route registered in `mod.rs`.

- **`ZBusClient::set_property()` implementation** (`dbus/mod.rs`) — Previously
  returned an immediate error. Now builds a `PropertiesProxy`, converts the
  `serde_json::Value` to a `zbus::zvariant::Value` via `json_to_zvariant()`, and
  calls `org.freedesktop.DBus.Properties.Set`. Supports string, bool, integer,
  double, and string-array JSON types. Object and null values return a descriptive
  error. Six new unit tests cover all conversion paths.

- **DBus chassis enumeration** (`chassis.rs`) — `GET /redfish/v1/Chassis` now queries
  `xyz.openbmc_project.Inventory.Manager / GetManagedObjects` and returns all objects
  that implement `xyz.openbmc_project.Inventory.Item.Chassis`, ordered lexicographically.
  Falls back to a single `"chassis"` member when DBus is unavailable or the inventory
  is empty. `GET /redfish/v1/Chassis/{id}` validates the chassis ID against the same
  inventory, also accepting the default `"chassis"` ID as a fallback.

- **Processor and Memory instance endpoints** (`systems.rs`, `mod.rs`) —
  `GET /redfish/v1/Systems/system/Processors/{id}` returns a `Processor.v1_16_0.Processor`
  resource with `Model`, `TotalCores`, and `TotalThreads` pulled from
  `xyz.openbmc_project.Inventory.Item.Cpu` in DBus. Returns 404 if the ID is not found
  in the inventory.
  `GET /redfish/v1/Systems/system/Memory/{id}` returns a `Memory.v1_18_0.Memory`
  resource with `CapacityMiB`, `OperatingSpeedMhz`, and `MemoryType` pulled from
  `xyz.openbmc_project.Inventory.Item.Dimm`. Both routes registered in `mod.rs`.
  The Processors and Memory collections are updated to enumerate live DBus inventory.

### Changed

- `systems.rs` Processors and Memory collections now dynamically enumerate DBus
  inventory objects rather than returning hard-coded empty collections.
- `privilege.rs` `session_role()` now reads `session.role` rather than returning
  a hard-coded `"ReadOnly"` for all sessions.
- All DBus-querying handlers use the `ZBusClient::from_connection()` constructor
  with graceful fallback when `AppState.dbus_connection` is `None`.

### Tests

- 112 unit tests passing (up from ~80); all new DBus-integration paths covered with
  no-DBus fallback tests. New coverage for privilege checks with specific roles,
  `json_to_zvariant` conversion, EventLog endpoint, PowerState mapping.

---

- **SessionService API** — Full Redfish SessionService resource family
  (GET/PATCH /redfish/v1/SessionService, GET/POST /redfish/v1/SessionService/Sessions,
  GET/DELETE /redfish/v1/SessionService/Sessions/{id}). Session creation via PAM,
  returns `X-Auth-Token` header. Includes /Members alias per DSP0266.

- **AccountService API** — Complete AccountService resource family including
  Accounts collection (GET/POST), individual account management (GET/PATCH/DELETE),
  Roles collection, and individual Role resources. All four Redfish built-in roles
  (Administrator, Operator, ReadOnly, NoAccess) with correct privilege mappings.

- **EventService API** — EventService resource family wired to the EventService
  business layer: GET/PATCH EventService, SubmitTestEvent action, Subscriptions
  collection (GET/POST), individual subscription management (GET/DELETE).

- **TaskService API** — TaskService resource family: GET TaskService, Tasks
  collection (GET), individual task (GET/DELETE). Task state/progress/messages
  exposed per Redfish TaskService schema v1.2.0.

- **UpdateService API** — UpdateService with FirmwareInventory collection (GET),
  individual firmware items (GET), and SimpleUpdate action (POST). SimpleUpdate
  creates a task and returns 202 Accepted with Location header pointing at the task.

- **Privilege/RBAC system** — Redfish PrivilegeRegistry-conformant RBAC per
  DSP0272. Five standard privileges, four built-in roles with correct assignments,
  `check_privilege()` helper for route handlers, route-level privilege constants.

- **ZBusClient** — Production DBus client using `org.freedesktop.DBus.Properties`
  and `ObjectManager` proxies. `MockDbusClient` for unit testing with pre-populated
  property maps. `zvariant_to_json` type conversion helper.

- **TLS/HTTPS support** — `rustls`-backed HTTPS server with PEM certificate loading,
  automatic fallback to self-signed generation (documents `rcgen` TODO), HTTP/2 via
  ALPN, TLS-based accept loop with per-connection `tokio::spawn`.

- **Chassis sub-resources** — Power (PSUs, voltage sensors), Thermal (temperatures,
  fans), Sensors collection, NetworkAdapters collection. All document OpenBMC DBus
  interface paths for future integration.

- **Systems sub-resources** — Processors collection, Memory collection, Storage
  collection, EthernetInterfaces collection, LogServices collection. Each endpoint
  documents the relevant OpenBMC DBus inventory interface.

- **Managers sub-resources** — NetworkProtocol (GET/PATCH with NTP/SSH/HTTPS config),
  EthernetInterfaces collection and individual NIC, LogServices collection. NIC
  endpoint includes full DHCPv4/v6 and IPv4/v6 fields.

- **WebSocket foundation** — Serial console handler (`/console0`) that proxies
  bidirectional byte streams between WebSocket and the obmc-console UNIX socket at
  `/run/obmc-console/default`. KVM handler stub at `/kvm/0` with detailed TODO.

- **Authentication middleware integration** — `optional_auth_middleware` applied
  globally to Redfish and WebSocket routes. Session creation endpoints intentionally
  receive unauthenticated requests. WebSocket auth checked post-upgrade.

- **Persistent data store** — `PersistentStore` reads/writes `/var/lib/bmcweb/config.json`
  with atomic rename-based writes. System UUID is generated once and persisted across
  restarts. Format is versioned JSON matching upstream bmcweb's persistent_data convention.

- **`extract_client_ip()`** — Public helper in auth middleware for consistent IP
  extraction across session creation and middleware layers.

### Changed

- Updated all repository URLs from internal to public GitHub (`https://github.com/gtmills/bmcweb-ng`)
- Improved `managers.rs`: added NetworkProtocol, EthernetInterfaces, LogServices
  sub-resource links, expanded SerialConsole/CommandShell/GraphicalConsole fields
- Improved `systems.rs`: added HealthRollup, AllowableValues for ResetType,
  expanded sub-resource link list
- Improved `chassis.rs`: added HealthRollup, Assembly link
- `protocol/mod.rs`: removed stale TODO comments, re-exported `HttpServer`
- `auth/mod.rs`: exported `extract_client_ip`, `check_privilege`, `privileges_for_role`

### Fixed

- Service root test assertion now matches actual `RedfishVersion` value
- `get_client_ip()` made private; public `extract_client_ip()` wrapper added

### Security

- RBAC privilege checking infrastructure in place
- Session tokens use UUID v4 (119-bit entropy, above OWASP minimum of 64 bits)
- Redfish role stored per-session and read at privilege check time

## [0.1.0] - 2026-04-28

### Added
- Initial project setup
- Basic Rust project structure
- README with project overview and roadmap
- Apache 2.0 license
- Git repository initialization

[Unreleased]: https://github.com/gtmills/bmcweb-ng/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/gtmills/bmcweb-ng/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gtmills/bmcweb-ng/releases/tag/v0.1.0
