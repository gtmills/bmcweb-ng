# bmcweb-ng Development Status

## Overview
This document tracks the development progress of bmcweb-ng, a Rust rewrite of the OpenBMC bmcweb server.

**Last Updated:** 2026-07-11

## Project Structure

```
bmcweb-ng/
├── src/
│   ├── main.rs              ✅ Main entry point with config loading, DBus init, HTTP server
│   ├── lib.rs               ✅ Core library with AppState
│   ├── persistent_data.rs   ✅ UUID and session persistence (atomic JSON writes)
│   ├── config/
│   │   └── mod.rs           ✅ Configuration management (TOML-based)
│   ├── protocol/
│   │   ├── mod.rs           ✅ Protocol layer exports
│   │   └── http.rs          ✅ HTTP/HTTPS server (axum/hyper, rustls TLS)
│   ├── api/
│   │   ├── mod.rs           ✅ API layer
│   │   ├── redfish/
│   │   │   ├── mod.rs           ✅ Redfish router (full route table)
│   │   │   ├── service_root.rs  ✅ ServiceRoot (v1.17.0 / v1.15.0 type)
│   │   │   ├── systems.rs       ✅ Systems + sub-resources (Processors, Memory, etc.)
│   │   │   ├── chassis.rs       ✅ Chassis + Power/Thermal/Sensors/NetworkAdapters
│   │   │   ├── managers.rs      ✅ Managers + NetworkProtocol/EthernetInterfaces/LogServices
│   │   │   ├── sessions.rs      ✅ SessionService + Sessions (full login flow)
│   │   │   ├── accounts.rs      ✅ AccountService + Accounts + Roles
│   │   │   ├── event_service.rs      ✅ EventService + Subscriptions + SubmitTestEvent + SSE
│   │   │   ├── task_service.rs       ✅ TaskService + Tasks
│   │   │   ├── update_service.rs     ✅ UpdateService + FirmwareInventory + SimpleUpdate
│   │   │   ├── certificate_service.rs ✅ CertificateService + CertificateLocations
│   │   │   └── telemetry_service.rs  ✅ TelemetryService + MetricDefinitions/Reports/ReportDefinitions
│   │   └── websocket/
│   │       └── mod.rs       ✅ Serial console (/console0), KVM stub (/kvm/0)
│   ├── auth/
│   │   ├── mod.rs           ✅ Authentication module (exports all auth types)
│   │   ├── basic.rs         ✅ HTTP Basic authentication with PAM
│   │   ├── session.rs       ✅ Session management (create, lookup, expire, delete)
│   │   ├── middleware.rs    ✅ Auth middleware + extract_client_ip()
│   │   └── privilege.rs     ✅ Redfish RBAC (5 privileges, 4 roles, check_privilege)
│   ├── dbus/
│   │   └── mod.rs           ✅ DBus trait + ZBusClient (production) + MockDbusClient (tests)
│   ├── services/
│   │   ├── mod.rs           ✅ Service layer exports
│   │   ├── event.rs         ✅ Event Service (subscriptions, async dispatch via reqwest)
│   │   ├── task.rs          ✅ Task Service (state machine, progress, messages)
│   │   └── update.rs        ✅ Update Service (firmware inventory, update operations)
│   └── observability/
│       ├── mod.rs           ✅ Metrics handler
│       └── metrics.rs       ✅ Prometheus metrics (HTTP, auth, Redfish, DBus counters)
├── bmcweb-ng.service        ✅ Systemd service file (security hardening)
├── bmcweb-ng.socket         ✅ Systemd socket activation file
├── Cargo.toml               ✅ Dependencies configured
├── config.toml              ✅ Default configuration
└── README.md                ✅ Project documentation
```

## Implementation Status

### ✅ Completed Features

1. **Project Infrastructure**
   - Cargo.toml with all necessary dependencies
   - Basic project structure following Rust best practices
   - Comprehensive documentation (README, ARCHITECTURE, BUILDING, CONTRIBUTING)

2. **Configuration Management** (`src/config/mod.rs`)
   - TOML-based configuration
   - Server, auth, features, logging, and metrics config sections
   - Default configuration with sensible values
   - File-based config loading with fallback to defaults

3. **Persistent Data** (`src/persistent_data.rs`)
   - System UUID persistence across restarts
   - Atomic JSON writes to `/var/lib/bmcweb/config.json`
   - Session state persistence scaffolding
   - Versioned schema (v1)

4. **Application State** (`src/lib.rs`)
   - Shared state structure with Arc for thread-safety
   - Configuration, DBus connection, session store, metrics, services

5. **HTTP/HTTPS Server** (`src/protocol/http.rs`)
   - Axum-based server with compression middleware and request tracing
   - Structured JSON health endpoint (`/health`) with per-component dbus/sessions/metrics status
   - TLS with rustls: loads PEM cert/key, self-signed generation stub
   - TLS accept loop with per-connection tokio::spawn
   - Auth middleware applied to Redfish routes

6. **Redfish API — Core Resources**
   - ServiceRoot (`/redfish/v1`) — Redfish v1.17.0 compliant
   - Systems collection + instance + ComputerSystem.Reset action
   - Systems sub-resources: Processors, Memory, Storage, EthernetInterfaces, LogServices
   - Chassis collection + instance
   - Chassis sub-resources: Power, Thermal, Sensors, NetworkAdapters
   - Managers collection + instance + Manager.Reset action
   - Managers sub-resources: NetworkProtocol, EthernetInterfaces, LogServices

7. **Redfish API — Services**
   - SessionService + Sessions (full login flow, PAM auth, X-Auth-Token); SessionTimeout persisted via AtomicI64
   - AccountService + Accounts + Roles (four built-in Redfish roles)
   - EventService + Subscriptions + SubmitTestEvent action; PATCH settings persisted
   - TaskService + Tasks collection
   - UpdateService + FirmwareInventory + SimpleUpdate action (202 + Location)

8. **Authentication** (`src/auth/`)
   - HTTP Basic authentication (RFC 7617) with PAM
   - Session management (create, lookup by ID/token, expiry, delete)
   - Cookie-based session auth (BMCWEB-SESSION cookie)
   - X-Auth-Token header auth
   - Authentication middleware (optional + mandatory variants)
   - Redfish PrivilegeRegistry RBAC (5 privileges, 4 roles)

9. **DBus Layer** (`src/dbus/mod.rs`)
   - `DbusClient` trait: get/set property, get_all_properties, call_method, get_managed_objects
   - `ZBusClient`: production implementation using zbus fdo proxies
   - `ZBusClient::set_property()` fully implemented with `json_to_zvariant()` converter
   - `ZBusClient::call_method()` fully implemented: dispatches on JSON arg shape (None/String/scalar/array); `call_method_hetero_array` helper for heterogeneous `(s as b)` signatures
   - `MockDbusClient`: in-memory mock for unit testing
   - `zvariant_to_json` and `json_to_zvariant` type conversion helpers

10. **WebSocket Support** (`src/api/websocket/mod.rs`)
    - Serial console `/console0`: full bidirectional proxy to obmc-console UNIX socket
    - KVM `/kvm/0`: bidirectional TCP proxy to `obmc-ikvm` at `127.0.0.1:5900`

11. **Observability** (`src/observability/`)
    - Prometheus metrics (HTTP, auth, Redfish, DBus counters/histograms)
    - `GET /metrics` endpoint on configurable port

12. **Systemd Integration**
    - Service file with security hardening (NoNewPrivileges, PrivateTmp, etc.)
    - Socket activation support

### ✅ Completed DBus wiring — Systems and Managers

1. **Live PowerState** — `GET /Systems/system` reads `CurrentHostState` from DBus
2. **Live FirmwareVersion** — `GET /Managers/bmc` reads `Version` from BMC image object
3. **Live hostname + NTP** — `GET /Managers/bmc/NetworkProtocol` reads from `Network.SystemConfiguration`
4. **Live NIC properties** — `GET /Managers/bmc/EthernetInterfaces/eth0` reads MAC + IP from DBus
5. **Role-aware sessions** — `UserSession.role` set from DBus `GetUserInfo` at login
6. **RBAC uses real role** — `session_role()` returns stored role, not hard-coded "ReadOnly"
7. **LogServices/EventLog** — `GET /Systems/system/LogServices/EventLog` endpoint added
8. **`set_property()` working** — `ZBusClient` can now write string/bool/int/float/string-array DBus properties
9. **DBus chassis enumeration** — `GET /Chassis` and `GET /Chassis/{id}` enumerate from inventory
10. **Processor + Memory instances** — `GET /Systems/system/Processors/{id}` and `/Memory/{id}` with DBus data

### ✅ Completed DBus wiring — Chassis inventory and power

1. **FirmwareInventory from DBus** — `GET /UpdateService/FirmwareInventory` enumerates live software objects from `xyz.openbmc_project.Software.BMC.Updater` via `GetManagedObjects`; deduplicates with in-memory firmware
2. **System AssetTag/SerialNumber/Model from DBus** — `GET /Systems/system` reads `AssetTag` from `Inventory.Decorator.AssetTag`, and `SerialNumber`, `PartNumber`, `Model` from `Inventory.Decorator.Asset` on the chassis inventory object
3. **PATCH /Systems/system AssetTag** — Writes `AssetTag` via `set_property` on `xyz.openbmc_project.Inventory.Decorator.AssetTag`
4. **Chassis live data from DBus** — `GET /Chassis/{id}` reads `Name`, `Model`, `SerialNumber`, `PartNumber` and `IndicatorLED` from DBus inventory and LED physical state
5. **PATCH /Chassis/{id} IndicatorLED** — Writes `Asserted` bool on `xyz.openbmc_project.Led.Group` at `/led/groups/front_id`
6. **PowerControl total wattage** — `PowerConsumedWatts` on `GET /Chassis/{id}/Power` reads live value from `/sensors/power/total_power`
7. **Dynamic @odata.id** — Chassis sub-resource links now use the dynamic `chassis_id` rather than hard-coded `"chassis"`

### ✅ Completed DBus wiring — Storage, EthernetInterface, boot

1. **Storage collection from DBus** — `GET /Systems/system/Storage` enumerates `Inventory.Item.StorageController` objects; synthesises a "Storage/1" entry if only `Item.Drive` objects are present
2. **PATCH EthernetInterface** — `PATCH /Managers/bmc/EthernetInterfaces/{nic_id}` handles `DHCPv4.DHCPEnabled`, `MACAddress`, `IPv4StaticAddresses` via `set_property` and `call_method`
3. **Dynamic NIC validation** — `GET /Managers/bmc/EthernetInterfaces/{nic_id}` validates NIC id against live DBus NIC list instead of hard-coded `eth0`

### ✅ Completed DBus wiring — Boot override, EventLog, NetworkProtocol

1. **Boot override settings from DBus** — `GET /Systems/system` now returns live `BootSourceOverrideTarget/Enabled/Mode` from `xyz.openbmc_project.Control.Boot.Source` at `/control/host0/boot` and `/control/host0/boot/one_time`
2. **PATCH /Systems/system** — Sets `BootSource` and one-time boot via `set_property`; returns updated resource
3. **EventLog Entries collection** — `GET /EventLog/Entries` reads all entries from `xyz.openbmc_project.Logging` via `GetManagedObjects`, sorted newest-first
4. **EventLog Entry instance** — `GET /EventLog/Entries/{id}` reads a single entry via `get_all_properties`
5. **ClearLog action** — `POST /EventLog/Actions/LogService.ClearLog` calls `DeleteAll` on logging service
6. **PATCH NetworkProtocol fully wired** — `HostName` and `NTP.NTPServers` applied via `set_property` on `Network.SystemConfiguration`

### ✅ Completed DBus wiring — AccountService, sensors, resets, NIC enumeration

1. **AccountService full DBus wiring** — `GET /AccountService/Accounts` lists real users via `ListUsers`; `GET /Accounts/{id}` fetches live user info via `GetUserInfo`; `POST /Accounts` calls `CreateUser`; `PATCH /Accounts/{id}` writes `UserPrivilege`/`UserEnabled` via `set_property`; `DELETE /Accounts/{id}` calls `DeleteUser`
2. **Chassis Power sensors** — `GET /Chassis/{id}/Power` enumerates power-supply and voltage sensors from DBus inventory + `xyz.openbmc_project.Sensor` paths
3. **Chassis Thermal sensors** — `GET /Chassis/{id}/Thermal` enumerates temperature and fan sensors from DBus
4. **Chassis Sensors collection** — `GET /Chassis/{id}/Sensors` returns the full merged sensor list with `ReadingType`, `Reading`, and `Status`
5. **BMC reset via DBus** — `POST /Managers/bmc/Actions/Manager.Reset` writes `RequestedBMCTransition` on `xyz.openbmc_project.State.BMC`
6. **System reset via DBus** — `POST /Systems/system/Actions/ComputerSystem.Reset` maps all Redfish `ResetType` values to `xyz.openbmc_project.State.Host.Transition` enum strings
7. **NIC enumeration from DBus** — `GET /Managers/bmc/EthernetInterfaces` dynamically lists all NICs via `GetManagedObjects` filtering on `EthernetInterface` interface

### ⚠️ Partially Implemented

1. **TLS**
   - Certificate loading fully implemented
   - Self-signed generation requires `rcgen` dependency (documented TODO)
   - TLS accept loop implemented but uses placeholder for per-stream serving

2. **RBAC Enforcement**
   - Privilege infrastructure in place; session role populated at login
   - Per-route `check_privilege()` calls can now be added trivially

### ❌ Not Yet Implemented

1. **Additional Redfish Resources**
   - Registries / JsonSchemas

2. **Additional Authentication**
   - Mutual TLS (mTLS) certificate authentication
   - LDAP/Active Directory integration

3. **WebSocket — Additional Endpoints**
   - KVM (Remote Frame Buffer) full implementation
   - Virtual Media (`/vm/0/0`)
   - NBD virtual media (`/nbd/0`)

4. **DBus REST API** (`/api/v1`)
   - Direct DBus object tree access (upstream feature)

## Comparison with Original bmcweb

### Architecture Differences

| Feature | bmcweb (C++) | bmcweb-ng (Rust) |
|---------|--------------|------------------|
| Language | C++23 | Rust 2021 |
| Build System | Meson | Cargo |
| HTTP Library | Boost.Beast | axum/hyper |
| Async Runtime | Boost.Asio | tokio |
| DBus Library | sdbusplus | zbus |
| JSON Library | nlohmann/json | serde_json |
| Logging | Custom | tracing |
| TLS | OpenSSL | rustls |

### Feature Parity Status

| Feature | bmcweb | bmcweb-ng | Notes |
|---------|--------|-----------|-------|
| Redfish ServiceRoot | ✅ | ✅ | v1.17.0 compliant |
| Redfish Systems | ✅ | ✅ | GET+PATCH, live PowerState/Boot/AssetTag/SerialNumber; Reset via DBus |
| Redfish Systems/Processors | ✅ | ✅ | Collection + individual instance from DBus inventory |
| Redfish Systems/Memory | ✅ | ✅ | Collection + individual instance from DBus inventory |
| Redfish Systems/Storage | ✅ | ✅ | Collection enumerated from Inventory.Item.StorageController |
| Redfish Systems/LogServices | ✅ | ✅ | EventLog instance + Entries collection + ClearLog |
| Redfish Chassis | ✅ | ✅ | GET+PATCH, live name/model/serial/LED; Power/Thermal/Sensors |
| Redfish Managers | ✅ | ✅ | GET+PATCH NIC; live FirmwareVersion/hostname/NTP; Reset via DBus |
| SessionService | ✅ | ✅ | Full login flow, X-Auth-Token, role fetched from DBus |
| AccountService | ✅ | ✅ | Full CRUD + PATCH lockout policy + PrivilegeMap |
| EventService | ✅ | ✅ | Subscriptions + SubmitTestEvent + SSE stream + persisted PATCH settings + AtomicI64 timeout |
| TaskService | ✅ | ✅ | Collection + instance management |
| UpdateService | ✅ | ✅ | FirmwareInventory from DBus + SimpleUpdate |
| CertificateService | ✅ | ✅ | GET + CertificateLocations |
| TelemetryService | ✅ | ✅ | GET + MetricDefinitions/Reports/ReportDefinitions |
| Registries/JsonSchemas | ✅ | ✅ | Collection stubs |
| DBus set_property | ✅ | ✅ | String/bool/int/float/string-array types |
| DBus REST API | ✅ | ✅ | /bus/, /list/, /xyz/*, /org/* with GET+PUT |
| KVM WebSocket | ✅ | ✅ | TCP proxy to obmc-ikvm on :5900 |
| Serial Console | ✅ | ✅ | Full bidirectional proxy |
| Virtual Media | ✅ | ✅ | UNIX socket proxy to nbd-proxy (/run/media-proxy/slot_0) |
| Authentication | ✅ | ✅ | Basic + Session + Middleware |
| RBAC | ✅ | ✅ | Full; role from DBus at login, per-session storage |
| TLS/HTTPS | ✅ | ✅ | rustls with PEM loading |
| Static File Serving | ✅ | ✅ | ServeDir from /usr/share/www at /ui |
| Systemd Integration | ✅ | ✅ | Service + socket files |
| Persistent UUID | ✅ | ✅ | Atomic JSON persistence |
| Prometheus Metrics | ❌ | ✅ | Additional capability |

### Performance Measurements (QEMU, July 2026)

Measured on OpenBMC `qemuarm` (emulated Cortex-A15, 256 MB RAM). Binary:
`bmcwebd-ng v0.2.1`, `opt-level="z"`, LTO, stripped, `arm-unknown-linux-gnueabihf`.

| Metric | Target | Measured | Status |
|--------|--------|----------|--------|
| Binary Size | <1MB | 4.75 MB | ⚠️ Over (musl static needed for <5 MB) |
| Memory RSS (idle) | <10MB | **5.7 MB** | ✅ Met |
| Startup Time | <1s | ~1.6s | ⚠️ Over on QEMU (~5-10× slower than bare metal) |
| Request Latency (p99) | <100ms | **7ms** | ✅ Met |
| Concurrent 20 GETs | — | 20/20 ✅ | ✅ All successful |

## Development Roadmap

### Phase 1: Core Infrastructure ✅ Complete
- [x] Project setup
- [x] Configuration management
- [x] HTTP server (HTTP + HTTPS)
- [x] Basic Redfish ServiceRoot
- [x] Systemd integration

### Phase 2: Essential Features ✅ Complete
- [x] Authentication (Basic, Session)
- [x] Session Management
- [x] Event Service foundation and API
- [x] Task Service foundation and API
- [x] Update Service foundation and API
- [x] Redfish Systems resource (collection + sub-resources)
- [x] Redfish Chassis resource (collection + Power/Thermal/Sensors)
- [x] Redfish Managers resource (collection + NetworkProtocol/NICs)
- [x] AccountService and Roles
- [x] SessionService (login flow)
- [x] DBus client trait with ZBusClient and MockDbusClient
- [x] TLS/SSL support with rustls
- [x] Persistent UUID storage
- [x] RBAC privilege system

### Phase 3: DBus Integration ✅ Complete
- [x] Wire ZBusClient to Redfish resource handlers
- [x] Power state from xyz.openbmc_project.State.Host
- [x] Boot settings from xyz.openbmc_project.Control.Boot
- [x] Processor/DIMM inventory from xyz.openbmc_project.Inventory
- [x] Sensor data from xyz.openbmc_project.Sensor.Value
- [x] Firmware version from xyz.openbmc_project.Software.Version
- [x] Network config from xyz.openbmc_project.Network
- [x] User management from xyz.openbmc_project.User.Manager
- [x] BMC reset via xyz.openbmc_project.State.BMC
- [x] Host reset via xyz.openbmc_project.State.Host (all ResetType variants)
- [x] Chassis sensors (Power, Thermal, full Sensors collection)
- [x] Boot settings (xyz.openbmc_project.Control.Boot.Source — GET + PATCH)
- [x] Log entries (EventLog/Entries + instance + ClearLog via DBus)
- [x] PATCH NetworkProtocol (HostName + NTPServers via set_property)
- [x] Chassis LED (xyz.openbmc_project.Led.Group/Physical — GET + PATCH)
- [x] Chassis live inventory (AssetTag, SerialNumber, Model, PartNumber)
- [x] FirmwareInventory from DBus (xyz.openbmc_project.Software.Version)
- [x] Storage collection (Inventory.Item.StorageController enumeration)
- [x] PATCH EthernetInterface (DHCPEnabled, MACAddress, static IPs)
- [x] AccountService lockout policy from DBus (MaxLoginAttemptBeforeLockout)
- [x] CertificateService + TelemetryService endpoints

### Phase 4: Advanced Features
- [x] WebSocket KVM (TCP proxy to obmc-ikvm :5900)
- [x] Virtual Media (/vm/0/0 and /nbd/0 UNIX-socket proxy)
- [x] DBus REST API (/bus/, /list/, /xyz/*, /org/* GET + PUT)
- [ ] mTLS authentication
- [ ] LDAP integration

### Phase 5: Production Readiness
- [ ] Comprehensive integration testing
- [ ] Performance benchmarking vs bmcweb
- [ ] Security audit
- [ ] Yocto recipe integration
- [ ] Documentation completion

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## References

- Original bmcweb: https://github.com/openbmc/bmcweb
- bmcweb-ng (public): https://github.com/gtmills/bmcweb-ng
- Redfish Specification: https://www.dmtf.org/standards/redfish
- OpenBMC Project: https://github.com/openbmc/openbmc
