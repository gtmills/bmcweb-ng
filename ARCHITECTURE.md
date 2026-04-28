# bmcweb-ng Architecture

This document describes the architecture and design decisions of bmcweb-ng, a Rust rewrite of the OpenBMC webserver.

## Table of Contents

- [Overview](#overview)
- [Design Principles](#design-principles)
- [Architecture Layers](#architecture-layers)
- [Component Details](#component-details)
- [Data Flow](#data-flow)
- [Concurrency Model](#concurrency-model)
- [Error Handling](#error-handling)
- [Security](#security)
- [Performance Considerations](#performance-considerations)

## Overview

bmcweb-ng is designed as a high-performance, memory-safe BMC webserver that implements the Redfish API specification. The architecture follows a layered approach with clear separation of concerns.

```
┌─────────────────────────────────────────────────────────────┐
│                     Client Applications                      │
│              (Web UI, CLI tools, Management Software)        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      Protocol Layer                          │
│         HTTP/1.1, HTTP/2, HTTPS, WebSocket, TLS             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Authentication Layer                      │
│        Basic, Session, Cookie, mTLS, XToken                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                        API Layer                             │
│         Redfish Resources, REST Endpoints, WebSocket        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      Service Layer                           │
│    System, Chassis, Manager, Session, Event Management      │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                       DBus Layer                             │
│         Abstraction over OpenBMC DBus Services              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    OpenBMC Services                          │
│    phosphor-*, xyz.openbmc_project.* DBus Services          │
└─────────────────────────────────────────────────────────────┘
```

## Design Principles

### 1. Layered Architecture
Each layer has a specific responsibility and communicates only with adjacent layers. This promotes:
- **Separation of concerns**: Each layer focuses on one aspect
- **Testability**: Layers can be tested independently with mocks
- **Maintainability**: Changes in one layer don't affect others
- **Flexibility**: Layers can be swapped or extended

### 2. Dependency Injection
Components receive their dependencies through constructor injection:
```rust
pub struct RedfishService {
    dbus_client: Arc<dyn DBusClient>,
    config: Arc<Config>,
}

impl RedfishService {
    pub fn new(dbus_client: Arc<dyn DBusClient>, config: Arc<Config>) -> Self {
        Self { dbus_client, config }
    }
}
```

Benefits:
- Easy to mock dependencies for testing
- Clear dependency graph
- Supports multiple implementations

### 3. Trait-Based Abstractions
Key interfaces are defined as traits:
```rust
#[async_trait]
pub trait DBusClient: Send + Sync {
    async fn get_property(&self, path: &str, interface: &str, property: &str) 
        -> Result<Value>;
    async fn set_property(&self, path: &str, interface: &str, property: &str, value: Value) 
        -> Result<()>;
    async fn call_method(&self, path: &str, interface: &str, method: &str, args: &[Value]) 
        -> Result<Value>;
}
```

This enables:
- Multiple implementations (real, mock, test)
- Runtime polymorphism
- Clear contracts between components

### 4. Async/Await
All I/O operations use async/await for efficient concurrency:
- Non-blocking I/O
- Efficient resource utilization
- Scalable to many concurrent connections
- Clean, readable code compared to callbacks

### 5. Type Safety
Leverage Rust's type system for correctness:
- Strong typing prevents many bugs at compile time
- `Result<T, E>` for error handling
- `Option<T>` for nullable values
- Newtype pattern for domain types

## Architecture Layers

### 1. Protocol Layer (`src/protocol/`)

**Responsibility**: Handle low-level network protocols

**Components**:
- HTTP server (HTTP/1.1, HTTP/2)
- TLS configuration and certificate management
- WebSocket protocol handling
- Connection management

**Technologies**:
- `tokio` - Async runtime
- `hyper` - HTTP implementation
- `axum` - Web framework
- `tokio-rustls` - TLS support
- `tokio-tungstenite` - WebSocket support

**Key Features**:
- HTTP/2 with ALPN negotiation
- TLS 1.3 support
- Automatic certificate generation
- Connection pooling and keep-alive
- Request/response compression

### 2. Authentication Layer (`src/auth/`)

**Responsibility**: Authenticate and authorize requests

**Components**:
- Basic authentication (RFC 7617)
- Session-based authentication
- Cookie authentication
- Mutual TLS (mTLS)
- XToken authentication (Redfish)
- Privilege checking

**Flow**:
```
Request → Extract Credentials → Validate → Check Privileges → Allow/Deny
```

**Session Management**:
- In-memory session store
- Configurable timeout
- Session token generation
- Concurrent session limits

### 3. API Layer (`src/api/`)

**Responsibility**: Expose HTTP endpoints and handle routing

**Components**:
- Redfish resource handlers (`src/api/redfish/`)
- WebSocket handlers (`src/api/websocket/`)
- REST API endpoints
- Request validation
- Response formatting

**Routing**:
```rust
Router::new()
    .route("/redfish/v1", get(service_root))
    .route("/redfish/v1/Systems", get(systems_collection))
    .route("/redfish/v1/Systems/:id", get(system_instance))
    .route("/redfish/v1/Chassis", get(chassis_collection))
    // ... more routes
```

### 4. Service Layer (`src/services/`)

**Responsibility**: Business logic and resource management

**Components**:
- System management
- Chassis management
- Manager resources
- Session management
- Event service
- Task service
- Update service

**Pattern**:
```rust
pub struct SystemService {
    dbus: Arc<dyn DBusClient>,
}

impl SystemService {
    pub async fn get_system(&self, id: &str) -> Result<System> {
        // 1. Validate input
        // 2. Query DBus for system information
        // 3. Transform to Redfish format
        // 4. Return result
    }
}
```

### 5. DBus Layer (`src/dbus/`)

**Responsibility**: Abstract DBus communication

**Components**:
- DBus client trait
- zbus implementation
- Mock implementation for testing
- Connection pooling
- Error mapping

**Abstraction**:
```rust
#[async_trait]
pub trait DBusClient: Send + Sync {
    async fn get_property(&self, path: &str, interface: &str, property: &str) 
        -> Result<Value>;
    // ... other methods
}

pub struct ZBusClient {
    connection: Connection,
}

#[async_trait]
impl DBusClient for ZBusClient {
    async fn get_property(&self, path: &str, interface: &str, property: &str) 
        -> Result<Value> {
        // Implementation using zbus
    }
}
```

### 6. Configuration Layer (`src/config/`)

**Responsibility**: Application configuration management

**Features**:
- TOML-based configuration
- Environment variable overrides
- Command-line argument parsing
- Configuration validation
- Default values

**Structure**:
```rust
#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub features: FeatureConfig,
    pub logging: LoggingConfig,
    pub metrics: MetricsConfig,
}
```

### 7. Observability Layer (`src/observability/`)

**Responsibility**: Logging, metrics, and tracing

**Components**:
- Structured logging (tracing)
- Prometheus metrics
- OpenTelemetry tracing
- Health checks

**Metrics**:
- Request count and latency
- Active connections
- Authentication attempts
- DBus call statistics
- Error rates

## Data Flow

### Typical Request Flow

```
1. Client Request
   ↓
2. TLS Termination (if HTTPS)
   ↓
3. HTTP Parsing
   ↓
4. Authentication Middleware
   ↓
5. Authorization Check
   ↓
6. Route Matching
   ↓
7. Handler Execution
   ↓
8. Service Layer Call
   ↓
9. DBus Query
   ↓
10. Response Formatting
    ↓
11. Compression (if supported)
    ↓
12. Send Response
```

### WebSocket Flow

```
1. HTTP Upgrade Request
   ↓
2. Authentication
   ↓
3. WebSocket Handshake
   ↓
4. Persistent Connection
   ↓
5. Bidirectional Messages
   ↓
6. Event Streaming / KVM / Serial
```

## Concurrency Model

### Tokio Runtime

bmcweb-ng uses the Tokio async runtime:
- Multi-threaded work-stealing scheduler
- Efficient task scheduling
- Non-blocking I/O
- Cooperative multitasking

### Shared State

Shared state is managed using:
- `Arc<T>` for shared ownership
- `RwLock<T>` for read-write access
- `Mutex<T>` for exclusive access
- Atomic types for simple counters

**Example**:
```rust
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<String, Session>>>,
    pub dbus: Arc<dyn DBusClient>,
    pub config: Arc<Config>,
}
```

### Task Spawning

Long-running operations are spawned as separate tasks:
```rust
tokio::spawn(async move {
    // Long-running operation
    process_event_subscription(subscription).await;
});
```

## Error Handling

### Error Types

1. **Application Errors** (`anyhow::Error`):
   - Used in application code
   - Provides context and backtraces
   - Easy error propagation with `?`

2. **Library Errors** (`thiserror`):
   - Used in library code
   - Structured error types
   - Implements `std::error::Error`

**Example**:
```rust
#[derive(Debug, thiserror::Error)]
pub enum DBusError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Property not found: {path}:{interface}:{property}")]
    PropertyNotFound {
        path: String,
        interface: String,
        property: String,
    },
    
    #[error("Method call failed: {0}")]
    MethodCallFailed(#[from] zbus::Error),
}
```

### Error Propagation

```rust
pub async fn get_system_info(id: &str) -> Result<SystemInfo> {
    let power_state = dbus.get_property(
        &format!("/xyz/openbmc_project/state/host{}", id),
        "xyz.openbmc_project.State.Host",
        "CurrentHostState"
    ).await?;  // Propagate error with ?
    
    Ok(SystemInfo {
        power_state: parse_power_state(&power_state)?,
        // ...
    })
}
```

## Security

### Authentication

Multiple authentication methods supported:
1. **Basic Auth**: Username/password via HTTP Basic
2. **Session Auth**: Token-based sessions
3. **Cookie Auth**: Browser-friendly cookies
4. **mTLS**: Certificate-based authentication
5. **XToken**: Redfish session tokens

### Authorization

Role-based access control (RBAC):
- Administrator
- Operator
- ReadOnly
- NoAccess

Privilege checking per endpoint:
```rust
#[derive(Debug, Clone, Copy)]
pub enum Privilege {
    Login,
    ConfigureManager,
    ConfigureUsers,
    ConfigureSelf,
    ConfigureComponents,
}

pub fn check_privilege(session: &Session, required: Privilege) -> Result<()> {
    if session.has_privilege(required) {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
```

### TLS

- TLS 1.3 preferred
- Strong cipher suites only
- Certificate validation
- Automatic certificate generation for development

### Input Validation

All inputs are validated:
- Request body parsing with serde
- Path parameter validation
- Query parameter validation
- Size limits on requests

## Performance Considerations

### Optimization Strategies

1. **Connection Pooling**:
   - Reuse DBus connections
   - HTTP keep-alive
   - WebSocket connection reuse

2. **Caching**:
   - Cache frequently accessed data
   - Invalidate on changes
   - TTL-based expiration

3. **Async I/O**:
   - Non-blocking operations
   - Concurrent request handling
   - Efficient resource utilization

4. **Zero-Copy**:
   - Use `Bytes` for buffer management
   - Avoid unnecessary allocations
   - Stream large responses

5. **Compression**:
   - gzip and zstd support
   - Compress responses > 1KB
   - Negotiate with client

### Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Binary Size | <1MB | Stripped release build |
| Memory Usage | <10MB | Idle state |
| Startup Time | <1s | Cold start |
| Request Latency (p99) | <100ms | Simple GET requests |
| Concurrent Connections | 100+ | Sustained load |
| Throughput | 1000+ req/s | Simple endpoints |

### Profiling

Tools for performance analysis:
- `cargo flamegraph` - CPU profiling
- `valgrind` - Memory profiling
- `perf` - Linux performance analysis
- `tokio-console` - Async runtime inspection

## Future Enhancements

### Planned Features

1. **HTTP/3 Support**: QUIC-based HTTP
2. **GraphQL API**: Alternative to REST
3. **gRPC Support**: For internal services
4. **Distributed Tracing**: Full request tracing
5. **Advanced Caching**: Redis integration
6. **Rate Limiting**: Per-user/IP rate limits
7. **API Versioning**: Multiple API versions
8. **Plugin System**: Extensible architecture

### Scalability

Future scalability improvements:
- Horizontal scaling with load balancer
- Distributed session storage
- Event streaming with Kafka
- Microservices architecture option

## References

- [Redfish Specification](https://www.dmtf.org/standards/redfish)
- [OpenBMC Documentation](https://github.com/openbmc/docs)
- [Tokio Documentation](https://tokio.rs/)
- [Axum Documentation](https://docs.rs/axum/)
- [zbus Documentation](https://docs.rs/zbus/)