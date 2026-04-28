# bmcweb-ng Development Status

## Overview
This document tracks the development progress of bmcweb-ng, a Rust rewrite of the OpenBMC bmcweb server.

**Last Updated:** 2026-04-28

## Project Structure

```
bmcweb-ng/
├── src/
│   ├── main.rs              ✅ Main entry point with config loading, DBus init, HTTP server
│   ├── lib.rs               ✅ Core library with AppState
│   ├── config/
│   │   └── mod.rs           ✅ Configuration management (TOML-based)
│   ├── protocol/
│   │   ├── mod.rs           ✅ Protocol layer exports
│   │   └── http.rs          ✅ HTTP server implementation (axum/hyper)
│   ├── api/
│   │   ├── mod.rs           ⚠️  API layer (basic structure)
│   │   ├── redfish/
│   │   │   ├── mod.rs       ✅ Redfish router
│   │   │   └── service_root.rs ✅ ServiceRoot endpoint (v1.17.0 compliant)
│   │   └── websocket/
│   │       └── mod.rs       ❌ WebSocket handlers (TODO)
│   ├── auth/
│   │   └── mod.rs           ❌ Authentication (TODO)
│   ├── dbus/
│   │   └── mod.rs           ⚠️  DBus interface (basic structure)
│   ├── services/
│   │   └── mod.rs           ❌ Service modules (TODO)
│   └── observability/
│       └── mod.rs           ❌ Metrics/logging (TODO)
├── bmcweb-ng.service        ✅ Systemd service file
├── bmcweb-ng.socket         ✅ Systemd socket file
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

3. **Application State** (`src/lib.rs`)
   - Shared state structure with Arc for thread-safety
   - Configuration storage
   - Optional DBus connection support
   - System UUID management

4. **HTTP Server** (`src/protocol/http.rs`)
   - Axum-based HTTP server
   - Compression middleware (gzip, br, deflate)
   - Request tracing
   - Health check endpoint
   - Graceful shutdown support
   - TLS placeholder (TODO: implement rustls)

5. **Main Application** (`src/main.rs`)
   - Command-line argument parsing (clap)
   - Logging initialization (tracing/tracing-subscriber)
   - Configuration loading
   - DBus connection initialization (with graceful fallback)
   - HTTP server startup
   - Signal handling (Ctrl+C)
   - Graceful shutdown

6. **Redfish API**
   - ServiceRoot endpoint (`/redfish/v1`)
   - Redfish v1.17.0 compliant response
   - Proper @odata annotations
   - Protocol features supported declaration
   - Links to all major Redfish services

7. **Systemd Integration**
   - Service file with security hardening
   - Socket activation support
   - Proper user/group isolation

### ⚠️ Partially Implemented

1. **DBus Integration** (`src/dbus/mod.rs`)
   - Basic module structure
   - Connection initialization in main.rs
   - TODO: Implement actual DBus method calls
   - TODO: Add DBus object introspection
   - TODO: Implement property monitoring

2. **API Layer** (`src/api/mod.rs`)
   - Basic module structure
   - Redfish router configured
   - TODO: Add DBus REST API
   - TODO: Add authentication middleware

### ❌ Not Yet Implemented

1. **Authentication** (`src/auth/mod.rs`)
   - HTTP Basic authentication
   - Session-based authentication
   - PAM integration
   - JWT token support
   - LDAP/Active Directory integration

2. **Redfish Resources**
   - Systems collection and resources
   - Chassis collection and resources
   - Managers collection and resources
   - AccountService
   - SessionService
   - EventService
   - TaskService
   - UpdateService
   - TelemetryService
   - CertificateService

3. **WebSocket Support** (`src/api/websocket/mod.rs`)
   - KVM (Remote Frame Buffer)
   - Virtual Media
   - Host Serial Console

4. **Observability** (`src/observability/mod.rs`)
   - Prometheus metrics endpoint
   - OpenTelemetry integration
   - Structured logging
   - Performance monitoring

5. **Services** (`src/services/mod.rs`)
   - System management
   - Chassis management
   - Manager management
   - Event subscriptions
   - Task management

6. **TLS/SSL**
   - Certificate loading
   - rustls integration
   - Certificate management API
   - Auto-renewal support

7. **Persistent Data**
   - UUID persistence
   - Session storage
   - Configuration cache
   - Event log storage

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
| TLS | OpenSSL | rustls (planned) |

### Feature Parity Status

| Feature | bmcweb | bmcweb-ng | Notes |
|---------|--------|-----------|-------|
| Redfish ServiceRoot | ✅ | ✅ | v1.17.0 compliant |
| Redfish Systems | ✅ | ❌ | TODO |
| Redfish Chassis | ✅ | ❌ | TODO |
| Redfish Managers | ✅ | ❌ | TODO |
| DBus REST API | ✅ | ❌ | TODO |
| KVM WebSocket | ✅ | ❌ | TODO |
| Virtual Media | ✅ | ❌ | TODO |
| Host Console | ✅ | ❌ | TODO |
| Authentication | ✅ | ❌ | TODO |
| Session Management | ✅ | ❌ | TODO |
| Event Service | ✅ | ❌ | TODO |
| Task Service | ✅ | ❌ | TODO |
| Update Service | ✅ | ❌ | TODO |
| Static File Serving | ✅ | ❌ | TODO |
| Systemd Integration | ✅ | ✅ | Service files created |

## Build Status

### Current Issues
- **Cannot build on Windows**: Rust toolchain not installed
- **Requires Linux**: zbus dependency requires Linux DBus
- **Untested**: Code has not been compiled or tested yet

### Next Steps for Building
1. Set up Linux development environment (or WSL2)
2. Install Rust toolchain (rustup)
3. Run `cargo check` to verify compilation
4. Run `cargo test` to execute unit tests
5. Run `cargo build --release` for production binary

## Testing Strategy

### Unit Tests
- ✅ Configuration loading tests
- ✅ ServiceRoot response tests
- ❌ HTTP server tests (TODO)
- ❌ DBus integration tests (TODO)
- ❌ Authentication tests (TODO)

### Integration Tests
- ❌ End-to-end Redfish API tests (TODO)
- ❌ WebSocket connection tests (TODO)
- ❌ DBus method call tests (TODO)
- ❌ Performance benchmarks (TODO)

### Test Coverage Goals
- Target: 80% code coverage
- Current: Unknown (not yet measured)

## Performance Considerations

### Memory Safety
- ✅ Rust's ownership system prevents memory leaks
- ✅ No unsafe code in current implementation
- ✅ Thread-safe shared state with Arc

### Async Performance
- ✅ tokio runtime for efficient async I/O
- ✅ Connection pooling via axum
- ⚠️ TODO: Benchmark against original bmcweb

### Resource Usage
- Target: < 50MB memory footprint
- Target: < 5% CPU usage at idle
- Target: Handle 100+ concurrent connections

## Security Features

### Implemented
- ✅ Systemd security hardening (NoNewPrivileges, PrivateTmp, etc.)
- ✅ User/group isolation
- ✅ Resource limits

### TODO
- ❌ TLS/SSL encryption
- ❌ Authentication and authorization
- ❌ Rate limiting
- ❌ Input validation
- ❌ CSRF protection
- ❌ Security headers

## Deployment

### Installation (Planned)
```bash
# Build release binary
cargo build --release

# Install binary
sudo install -m 755 target/release/bmcwebd-ng /usr/bin/

# Install systemd files
sudo install -m 644 bmcweb-ng.service /etc/systemd/system/
sudo install -m 644 bmcweb-ng.socket /etc/systemd/system/

# Create user and directories
sudo useradd -r -s /sbin/nologin bmcweb-ng
sudo mkdir -p /etc/bmcweb /var/lib/bmcweb
sudo chown bmcweb-ng:bmcweb-ng /var/lib/bmcweb

# Install configuration
sudo install -m 644 config.toml /etc/bmcweb/

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable bmcweb-ng.socket
sudo systemctl start bmcweb-ng.socket
```

## Development Roadmap

### Phase 1: Core Infrastructure (Current)
- [x] Project setup
- [x] Configuration management
- [x] HTTP server
- [x] Basic Redfish ServiceRoot
- [x] Systemd integration
- [ ] Build and test on Linux

### Phase 2: Essential Features
- [ ] Authentication (Basic, Session)
- [ ] Redfish Systems resource
- [ ] Redfish Chassis resource
- [ ] Redfish Managers resource
- [ ] DBus integration
- [ ] TLS/SSL support

### Phase 3: Advanced Features
- [ ] WebSocket support (KVM, Console)
- [ ] Event Service
- [ ] Task Service
- [ ] Update Service
- [ ] Metrics and observability

### Phase 4: Production Readiness
- [ ] Comprehensive testing
- [ ] Performance optimization
- [ ] Security audit
- [ ] Documentation completion
- [ ] Yocto recipe integration

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## References

- Original bmcweb: https://github.com/openbmc/bmcweb
- IBM fork: https://github.com/ibm-openbmc/bmcweb
- Redfish Specification: https://www.dmtf.org/standards/redfish
- OpenBMC Project: https://github.com/openbmc