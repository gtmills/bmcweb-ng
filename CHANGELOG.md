# Changelog

All notable changes to bmcweb-ng will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

[Unreleased]: https://github.com/gtmills/bmcweb-ng/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gtmills/bmcweb-ng/releases/tag/v0.1.0
