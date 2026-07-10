# Changelog

All notable changes to bmcweb-ng will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

- RBAC privilege checking infrastructure in place (enforcement TODO per route)
- Session tokens use UUID v4 (119-bit entropy, above OWASP minimum of 64 bits)

## [0.1.0] - 2026-04-28

### Added
- Initial project setup
- Basic Rust project structure
- README with project overview and roadmap
- Apache 2.0 license
- Git repository initialization

[Unreleased]: https://github.com/gtmills/bmcweb-ng/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gtmills/bmcweb-ng/releases/tag/v0.1.0
