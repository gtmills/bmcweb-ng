# bmcweb-ng Development Status

## Overview
This document tracks the development progress of bmcweb-ng, a Rust rewrite of the OpenBMC bmcweb server.

**Last Updated:** 2026-05-01

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
│   │   │   ├── event_service.rs ✅ EventService + Subscriptions + SubmitTestEvent
│   │   │   ├── task_service.rs  ✅ TaskService + Tasks
│   │   │   └── update_service.rs ✅ UpdateService + FirmwareInventory + SimpleUpdate
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
   - Health check endpoint
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
   - SessionService + Sessions (full login flow, PAM auth, X-Auth-Token)
   - AccountService + Accounts + Roles (four built-in Redfish roles)
   - EventService + Subscriptions + SubmitTestEvent action
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
   - `MockDbusClient`: in-memory mock for unit testing
   - `zvariant_to_json` conversion helper

10. **WebSocket Support** (`src/api/websocket/mod.rs`)
    - Serial console `/console0`: full bidirectional proxy to obmc-console UNIX socket
    - KVM `/kvm/0`: stub with RFB protocol implementation guide

11. **Observability** (`src/observability/`)
    - Prometheus metrics (HTTP, auth, Redfish, DBus counters/histograms)
    - `GET /metrics` endpoint on configurable port

12. **Systemd Integration**
    - Service file with security hardening (NoNewPrivileges, PrivateTmp, etc.)
    - Socket activation support

### ⚠️ Partially Implemented

1. **DBus Integration** (all Redfish handlers)
   - Trait and production client implemented
   - Each handler documents the OpenBMC DBus path/interface with TODO markers
   - Actual property reading/writing pending typed proxy generation

2. **TLS**
   - Certificate loading fully implemented
   - Self-signed generation requires `rcgen` dependency (documented TODO)
   - TLS accept loop implemented but uses placeholder for per-stream serving

3. **RBAC Enforcement**
   - Privilege infrastructure in place
   - Per-route enforcement (calling `check_privilege()` in handlers) is TODO
   - Session role lookup from DBus is TODO

### ❌ Not Yet Implemented

1. **Additional Redfish Resources**
   - TelemetryService
   - CertificateService
   - Registries / JsonSchemas
   - Individual Processor, Memory, Storage instances
   - Log entries (individual log event access)

2. **Additional Authentication**
   - Mutual TLS (mTLS) certificate authentication
   - LDAP/Active Directory integration

3. **WebSocket — Additional Endpoints**
   - KVM (Remote Frame Buffer) full implementation
   - Virtual Media (`/vm/0/0`)
   - NBD virtual media (`/nbd/0`)
   - Server-Sent Events for EventService

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
| Redfish Systems | ✅ | ✅ | Full collection + sub-resources |
| Redfish Chassis | ✅ | ✅ | Full collection + Power/Thermal/Sensors |
| Redfish Managers | ✅ | ✅ | Full collection + NetworkProtocol/NICs |
| SessionService | ✅ | ✅ | Full login flow, X-Auth-Token |
| AccountService | ✅ | ✅ | Accounts + Roles |
| EventService | ✅ | ✅ | Subscriptions + SubmitTestEvent |
| TaskService | ✅ | ✅ | Collection + instance management |
| UpdateService | ✅ | ✅ | FirmwareInventory + SimpleUpdate |
| DBus REST API | ✅ | ❌ | TODO |
| KVM WebSocket | ✅ | ⚠️ | Stub |
| Serial Console | ✅ | ✅ | Full bidirectional proxy |
| Virtual Media | ✅ | ❌ | TODO |
| Host Console | ✅ | ✅ | /console0 via obmc-console socket |
| Authentication | ✅ | ✅ | Basic + Session + Middleware |
| RBAC | ✅ | ✅ | Infrastructure + privilege constants |
| TLS/HTTPS | ✅ | ✅ | rustls with PEM loading |
| Static File Serving | ✅ | ❌ | TODO |
| Systemd Integration | ✅ | ✅ | Service + socket files |
| Persistent UUID | ✅ | ✅ | Atomic JSON persistence |
| Prometheus Metrics | ❌ | ✅ | Additional capability |

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

### Phase 3: DBus Integration (In Progress)
- [ ] Wire ZBusClient to Redfish resource handlers
- [ ] Power state from xyz.openbmc_project.State.Host
- [ ] Boot settings from xyz.openbmc_project.Control.Boot
- [ ] Processor/DIMM inventory from xyz.openbmc_project.Inventory
- [ ] Sensor data from xyz.openbmc_project.Sensor.Value
- [ ] Firmware version from xyz.openbmc_project.Software.Version
- [ ] Network config from xyz.openbmc_project.Network
- [ ] User management from xyz.openbmc_project.User.Manager

### Phase 4: Advanced Features
- [ ] WebSocket KVM (RFB protocol)
- [ ] Virtual Media
- [ ] DBus REST API
- [ ] TelemetryService
- [ ] CertificateService
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
