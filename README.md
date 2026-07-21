# bmcweb-ng

**Next-generation BMC webserver for OpenBMC, written in Rust**

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

## Overview

`bmcweb-ng` is a modern rewrite of [bmcweb](https://github.com/openbmc/bmcweb), the embedded webserver for OpenBMC. Built from the ground up in Rust, it provides a high-performance, memory-safe implementation of the Redfish API and other BMC management interfaces.

### Why a Rewrite?

The original bmcweb is a mature, production-ready C++ application. This rewrite aims to:

- **Improve Maintainability** - Simpler architecture, less template metaprogramming
- **Enhance Safety** - Memory safety guarantees from Rust
- **Better Testing** - Mockable interfaces, comprehensive test coverage
- **Developer Experience** - Faster compile times, better tooling, clearer error messages
- **Modern Async** - Clean async/await syntax instead of callback chains

### Key Features

- ✅ **Redfish API** - Broad Redfish coverage from ServiceRoot through TelemetryService, plus OData, Fabrics, and Virtual Media routes
- ✅ **Multiple Protocols** - HTTP/1.1, HTTP/2, HTTPS with TLS 1.3
- ✅ **Authentication** - Basic auth with PAM, Session management, Token-based auth
- ✅ **Event Service** - Event subscriptions, async notifications, SSE endpoint, and PATCH-configurable retry settings
- ✅ **Task Service** - Long-running operation tracking and management
- ✅ **Update Service** - Firmware update management and live DBus inventory
- ✅ **DBus Integration** - Comprehensive async DBus wiring to OpenBMC services
- ✅ **WebSocket Support** - Serial console, KVM proxy, and Virtual Media/NBD UNIX-socket proxies
- ✅ **Performance** - ~5MB binary, <10MB memory (idle), fast request latency on QEMU and real hardware
- ✅ **Observability** - Structured logging, Prometheus metrics support

## Architecture

```
┌─────────────────────────────────────┐
│   API Layer (Redfish/REST/WS)       │  ← HTTP handlers, routing
├─────────────────────────────────────┤
│   Business Logic (Resources)        │  ← Redfish resource handlers
├─────────────────────────────────────┤
│   Service Layer (DBus)              │  ← DBus abstraction, caching
├─────────────────────────────────────┤
│   Protocol Layer (HTTP/WS/TLS)      │  ← axum, hyper, tokio-tungstenite
├─────────────────────────────────────┤
│   Runtime (Async I/O)               │  ← tokio runtime
└─────────────────────────────────────┘
```

### Design Principles

1. **Layered Architecture** - Clear separation of concerns
2. **Dependency Injection** - Testable components via traits
3. **Schema-Driven** - Generate types from Redfish schemas
4. **Configuration as Code** - Runtime TOML configuration
5. **Fail Fast** - Validate at startup, not at runtime
6. **Observability First** - Logging, metrics, and tracing built-in

## Getting Started

### Prerequisites

- Rust 1.75 or later
- OpenBMC development environment (for DBus integration)
- OpenSSL 3.0+ (for TLS support)

### Building

```bash
# Clone the repository
git clone https://github.com/gtmills/bmcweb-ng.git
cd bmcweb-ng

# Build in debug mode
cargo build

# Build optimized release
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=info cargo run
```

### Configuration

Create a `config.toml` file:

```toml
[server]
bind_address = "0.0.0.0"
port = 443
tls_cert = "/etc/bmcweb/cert.pem"
tls_key = "/etc/bmcweb/key.pem"
max_connections = 100

[auth]
session_timeout_seconds = 3600
max_sessions = 64

[logging]
level = "info"

[metrics]
enabled = true
port = 9090
```

### Running

```bash
# Run with default config
cargo run --release

# Run with custom config
cargo run --release -- --config /path/to/config.toml

# Run in development mode with hot reload
cargo watch -x run
```

## Project Structure

```
bmcweb-ng/
├── src/
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Core library with AppState
│   ├── persistent_data.rs   # UUID and session persistence
│   ├── config/
│   │   └── mod.rs           # Configuration management (TOML)
│   ├── api/
│   │   ├── mod.rs           # API layer
│   │   └── redfish/         # Redfish endpoints
│   │       ├── mod.rs                # Router + route table
│   │       ├── service_root.rs       # ServiceRoot
│   │       ├── systems.rs            # Systems + sub-resources
│   │       ├── chassis.rs            # Chassis + sub-resources
│   │       ├── managers.rs           # Managers + sub-resources
│   │       ├── sessions.rs           # SessionService + Sessions
│   │       ├── accounts.rs           # AccountService + Accounts + Roles
│   │       ├── event_service.rs      # EventService + Subscriptions
│   │       ├── task_service.rs       # TaskService + Tasks
│   │       ├── update_service.rs     # UpdateService + FirmwareInventory
│   │       ├── certificate_service.rs # CertificateService
│   │       └── telemetry_service.rs  # TelemetryService
│   ├── auth/                # Authentication & authorization
│   │   ├── mod.rs           # Auth exports
│   │   ├── basic.rs         # Basic auth with PAM
│   │   ├── session.rs       # Session management
│   │   ├── middleware.rs    # Auth middleware
│   │   └── privilege.rs     # RBAC privilege checking
│   ├── dbus/
│   │   └── mod.rs           # DbusClient trait + ZBusClient + MockDbusClient
│   ├── services/            # Business logic services
│   │   ├── mod.rs
│   │   ├── event.rs         # Event Service
│   │   ├── task.rs          # Task Service
│   │   └── update.rs        # Update Service
│   ├── protocol/            # Protocol layer
│   │   ├── mod.rs
│   │   └── http.rs          # HTTP/HTTPS server (axum + rustls)
│   └── observability/       # Prometheus metrics
│       ├── mod.rs
│       └── metrics.rs
├── Cargo.toml               # Rust dependencies
├── config.toml              # Default configuration
├── bmcweb-ng.service        # Systemd service file
├── bmcweb-ng.socket         # Systemd socket activation
└── README.md                # This file
```

## Development

### Code Style

We follow the Rust standard style guide. Format your code before committing:

```bash
cargo fmt
```

### Linting

Run clippy to catch common mistakes:

```bash
cargo clippy -- -D warnings
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test '*'

# Run benchmarks
cargo bench
```

### Documentation

Generate and view documentation:

```bash
cargo doc --open
```

## Performance Targets

Measured on OpenBMC `qemuarm` (Cortex-A15, 256 MB RAM, 4 cores) — July 2026.

| Metric | Target | Current | Notes |
|--------|--------|---------|-------|
| Binary Size | <1MB | **4.75 MB** | ARM dynamically-linked release build; musl static target would be smaller — see note below |
| Memory RSS (idle) | <10MB | **5.7 MB** | Measured via `/proc/<pid>/status` after cold start, no active requests |
| Startup Time | <1s | **~1.6s** | Cold start on emulated ARM; <200ms expected on real hardware |
| Request Latency (p99) | <100ms | **<10ms** | p50=4ms p95=5ms p99=7ms — 30 sequential GETs to `/redfish/v1` on QEMU |
| Concurrent Connections | 100+ | **20/20** ✅ | 20 simultaneous GETs all succeeded; full 100+ load test pending on real hardware |

> **Binary size note**: The `<1MB` target assumed a musl static build. The current dynamically-linked
> ARM EABI release build is 4.75 MB stripped. This is because Tokio, hyper, rustls, zbus, and serde_json
> together contribute significant code. Switching to `aarch64-unknown-linux-musl` with LTO and
> `opt-level = "z"` (already set) typically yields 3–5 MB — still larger than the original target
> which was aspirational. The `<10MB` memory target is **met** at 5.7 MB RSS.

> **Startup time note**: 1.6s is measured on QEMU's emulated Cortex-A15. On a real BMC SoC
> (e.g. AST2600 at 800 MHz) startup is expected to be under 500ms. The `<1s` target is realistic
> for production hardware.

## Compatibility

### API Compatibility
- **Redfish API**: High parity with upstream bmcweb for the currently implemented route set
- **DBus Interface**: Same DBus calls and object model intent as bmcweb for implemented resources
- **Configuration**: New TOML format defined in [`config.toml`](config.toml)

### Migration from bmcweb
No standalone migration guide is checked into this repository yet. Use [`BUILDING.md`](BUILDING.md), [`ARCHITECTURE.md`](ARCHITECTURE.md), and [`DEVELOPMENT_STATUS.md`](DEVELOPMENT_STATUS.md) for current setup and parity details.

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Format code (`cargo fmt`)
6. Run linter (`cargo clippy`)
7. Commit your changes (`git commit -m 'Add amazing feature'`)
8. Push to the branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Original [bmcweb](https://github.com/openbmc/bmcweb) project and contributors
- OpenBMC community
- DMTF for the Redfish specification
- Rust community for excellent async ecosystem

## Contact

- **Project Lead**: Gunnar Mills
- **Repository**: https://github.com/gtmills/bmcweb-ng
- **Issues**: https://github.com/gtmills/bmcweb-ng/issues
- **IBM fork of upstream**: https://github.com/ibm-openbmc/bmcweb

## Related Projects

- [bmcweb](https://github.com/openbmc/bmcweb) - Original C++ implementation
- [OpenBMC](https://github.com/openbmc/openbmc) - Open source BMC firmware
- [Redfish](https://www.dmtf.org/standards/redfish) - DMTF Redfish specification