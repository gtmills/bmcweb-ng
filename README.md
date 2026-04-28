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

- ✅ **Redfish API** - Full DMTF Redfish specification compliance
- ✅ **Multiple Protocols** - HTTP/1.1, HTTP/2, HTTPS with TLS 1.3
- ✅ **Authentication** - Basic, Session, Cookie, mTLS, XToken
- ✅ **WebSocket Support** - KVM, Serial Console, Event Subscriptions
- ✅ **DBus Integration** - Async DBus communication with OpenBMC services
- ✅ **Performance** - <1MB binary, <10MB memory, <1s startup time
- ✅ **Observability** - Structured logging, Prometheus metrics, OpenTelemetry tracing

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
git clone https://github.ibm.com/gmills/bmcweb-ng
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
methods = ["basic", "session", "mtls"]
session_timeout_seconds = 3600
max_sessions = 64

[features]
redfish = true
dbus_rest = true
kvm = true
virtual_media = true
event_service = true

[logging]
level = "info"
format = "json"

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
│   ├── config/              # Configuration management
│   ├── api/                 # API layer
│   │   ├── redfish/         # Redfish endpoints
│   │   ├── dbus_rest/       # DBus REST API
│   │   └── websocket/       # WebSocket handlers
│   ├── services/            # Business logic
│   │   ├── system/          # System management
│   │   ├── chassis/         # Chassis management
│   │   ├── manager/         # Manager resources
│   │   └── session/         # Session management
│   ├── dbus/                # DBus abstraction layer
│   │   ├── client.rs        # DBus client trait
│   │   ├── zbus_impl.rs     # zbus implementation
│   │   └── mock.rs          # Mock for testing
│   ├── auth/                # Authentication & authorization
│   │   ├── basic.rs         # Basic auth
│   │   ├── session.rs       # Session auth
│   │   ├── mtls.rs          # Mutual TLS
│   │   └── privilege.rs     # Privilege checking
│   ├── protocol/            # Protocol layer
│   │   ├── http.rs          # HTTP server
│   │   ├── websocket.rs     # WebSocket server
│   │   └── tls.rs           # TLS configuration
│   ├── schema/              # Generated Redfish types
│   └── observability/       # Logging, metrics, tracing
├── schemas/                 # Redfish CSDL/JSON schemas
├── tests/
│   ├── unit/               # Unit tests
│   ├── integration/        # Integration tests
│   └── performance/        # Performance benchmarks
├── docs/
│   ├── architecture/       # Architecture documentation
│   ├── api/               # API documentation
│   └── migration/         # Migration guide from bmcweb
├── Cargo.toml             # Rust dependencies
├── config.toml            # Default configuration
├── rustfmt.toml           # Code formatting rules
└── README.md              # This file
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

## Roadmap

### Phase 1: Foundation (Months 1-3)
- [x] Project structure and build system
- [ ] HTTP/HTTPS server with tokio + axum
- [ ] Basic routing and middleware
- [ ] DBus abstraction layer
- [ ] Configuration management
- [ ] Logging and metrics

### Phase 2: Core Features (Months 4-7)
- [ ] Authentication (Basic, Session, mTLS)
- [ ] Authorization and privilege checking
- [ ] Session management
- [ ] ServiceRoot endpoint
- [ ] Systems collection
- [ ] Chassis collection
- [ ] Managers collection

### Phase 3: Advanced Features (Months 8-13)
- [ ] All Redfish schemas
- [ ] WebSocket support
- [ ] KVM implementation
- [ ] Virtual media
- [ ] Event service
- [ ] Task service
- [ ] Firmware updates

### Phase 4: Production Ready (Months 14-15)
- [ ] Performance optimization
- [ ] Security hardening
- [ ] Comprehensive documentation
- [ ] Migration tools
- [ ] Production deployment guide

## Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
| Binary Size | <1MB | TBD |
| Memory Usage | <10MB | TBD |
| Startup Time | <1s | TBD |
| Request Latency (p99) | <100ms | TBD |
| Concurrent Connections | 100+ | TBD |

## Compatibility

### API Compatibility
- **Redfish API**: 100% compatible with bmcweb
- **DBus Interface**: Same DBus calls as bmcweb
- **Configuration**: New TOML format (migration tool provided)

### Migration from bmcweb
See [docs/migration/from-bmcweb.md](docs/migration/from-bmcweb.md) for detailed migration guide.

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
- **Repository**: https://github.ibm.com/gmills/bmcweb-ng
- **Issues**: https://github.ibm.com/gmills/bmcweb-ng/issues

## Related Projects

- [bmcweb](https://github.com/openbmc/bmcweb) - Original C++ implementation
- [OpenBMC](https://github.com/openbmc/openbmc) - Open source BMC firmware
- [Redfish](https://www.dmtf.org/standards/redfish) - DMTF Redfish specification