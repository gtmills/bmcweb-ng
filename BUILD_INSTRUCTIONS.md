# Build Instructions for bmcweb-ng

## Prerequisites

### System Requirements
- **Operating System**: Linux (Ubuntu 22.04+, Fedora 38+, or similar)
- **Architecture**: x86_64 or ARM64
- **Memory**: 2GB RAM minimum for building
- **Disk Space**: 1GB free space

### Required Software

#### 1. Rust Toolchain
```bash
# Install rustup (Rust installer)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Source the cargo environment
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version

# Should show Rust 1.75 or later
```

#### 2. System Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpam0g-dev \
    libdbus-1-dev \
    libsystemd-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install -y \
    gcc \
    gcc-c++ \
    pkg-config \
    openssl-devel \
    pam-devel \
    dbus-devel \
    systemd-devel
```

**Arch Linux:**
```bash
sudo pacman -S \
    base-devel \
    pkg-config \
    openssl \
    pam \
    dbus \
    systemd
```

## Building from Source

### 1. Clone the Repository
```bash
cd /path/to/workspace
git clone https://github.ibm.com/gmills/bmcweb-ng.git
cd bmcweb-ng
```

### 2. Check Dependencies
```bash
# Verify all dependencies are available
cargo check
```

### 3. Build Debug Version
```bash
# Build with debug symbols (faster compilation)
cargo build

# Binary will be at: target/debug/bmcwebd-ng
```

### 4. Build Release Version
```bash
# Build optimized release binary
cargo build --release

# Binary will be at: target/release/bmcwebd-ng
```

### 5. Run Tests
```bash
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_service_root
```

### 6. Run the Server (Development)
```bash
# Run directly with cargo
cargo run

# Run with custom config
cargo run -- --config /path/to/config.toml

# Run with debug logging
cargo run -- --log-level debug

# Run with JSON logs
cargo run -- --json-logs
```

## Installation

### System-wide Installation

```bash
# Build release binary
cargo build --release

# Install binary
sudo install -m 755 target/release/bmcwebd-ng /usr/bin/

# Create user and group
sudo useradd -r -s /sbin/nologin bmcweb-ng
sudo groupadd -r web
sudo groupadd -r redfish
sudo groupadd -r hostconsole
sudo usermod -a -G web,redfish,hostconsole bmcweb-ng

# Create directories
sudo mkdir -p /etc/bmcweb
sudo mkdir -p /var/lib/bmcweb
sudo mkdir -p /var/log/bmcweb

# Set permissions
sudo chown bmcweb-ng:bmcweb-ng /var/lib/bmcweb
sudo chown bmcweb-ng:bmcweb-ng /var/log/bmcweb

# Install configuration
sudo install -m 644 config.toml /etc/bmcweb/

# Install systemd files
sudo install -m 644 bmcweb-ng.service /etc/systemd/system/
sudo install -m 644 bmcweb-ng.socket /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable and start service
sudo systemctl enable bmcweb-ng.socket
sudo systemctl start bmcweb-ng.socket

# Check status
sudo systemctl status bmcweb-ng.socket
sudo systemctl status bmcweb-ng.service
```

### Verify Installation

```bash
# Check if service is running
systemctl is-active bmcweb-ng.service

# View logs
journalctl -u bmcweb-ng.service -f

# Test HTTP endpoint
curl http://localhost/health

# Test Redfish endpoint
curl http://localhost/redfish/v1
```

## Development Workflow

### 1. Code Changes
```bash
# Make your changes in src/

# Format code
cargo fmt

# Check for issues
cargo clippy

# Run tests
cargo test
```

### 2. Continuous Development
```bash
# Install cargo-watch for auto-rebuild
cargo install cargo-watch

# Auto-rebuild and run on file changes
cargo watch -x run

# Auto-test on file changes
cargo watch -x test
```

### 3. Debugging
```bash
# Build with debug symbols
cargo build

# Run with debugger (gdb)
rust-gdb target/debug/bmcwebd-ng

# Run with debugger (lldb)
rust-lldb target/debug/bmcwebd-ng

# Enable backtrace on panic
RUST_BACKTRACE=1 cargo run
RUST_BACKTRACE=full cargo run
```

## Cross-Compilation

### For ARM64 (aarch64)
```bash
# Add target
rustup target add aarch64-unknown-linux-gnu

# Install cross-compiler
sudo apt-get install gcc-aarch64-linux-gnu

# Build
cargo build --release --target aarch64-unknown-linux-gnu
```

### For ARMv7 (armv7)
```bash
# Add target
rustup target add armv7-unknown-linux-gnueabihf

# Install cross-compiler
sudo apt-get install gcc-arm-linux-gnueabihf

# Build
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## Yocto Integration

### BitBake Recipe
The Yocto recipe is located at:
```
meta-phosphor/recipes-phosphor/interfaces/bmcweb-ng_git.bb
```

### Building with Yocto
```bash
# In your OpenBMC build directory
bitbake bmcweb-ng

# Clean and rebuild
bitbake -c clean bmcweb-ng
bitbake bmcweb-ng

# Deploy to target
bitbake bmcweb-ng -c deploy
```

## Troubleshooting

### Build Errors

#### "error: linker `cc` not found"
```bash
# Install build tools
sudo apt-get install build-essential
```

#### "error: failed to run custom build command for `openssl-sys`"
```bash
# Install OpenSSL development files
sudo apt-get install libssl-dev pkg-config
```

#### "error: failed to run custom build command for `pam-sys`"
```bash
# Install PAM development files
sudo apt-get install libpam0g-dev
```

#### "error: failed to run custom build command for `zbus`"
```bash
# Install DBus development files
sudo apt-get install libdbus-1-dev
```

### Runtime Errors

#### "Failed to establish DBus connection"
```bash
# Check if DBus is running
systemctl status dbus

# Start DBus if needed
sudo systemctl start dbus

# Check DBus permissions
ls -la /var/run/dbus/system_bus_socket
```

#### "Permission denied" when binding to port 443
```bash
# Option 1: Run as root (not recommended)
sudo ./target/release/bmcwebd-ng

# Option 2: Use systemd socket activation (recommended)
sudo systemctl start bmcweb-ng.socket

# Option 3: Grant capability to bind privileged ports
sudo setcap 'cap_net_bind_service=+ep' target/release/bmcwebd-ng
```

#### "Address already in use"
```bash
# Check what's using the port
sudo lsof -i :443
sudo lsof -i :80

# Stop conflicting service
sudo systemctl stop apache2  # or nginx, or other web server
```

## Performance Optimization

### Release Build Optimizations
The `Cargo.toml` already includes optimizations:
- `opt-level = "z"` - Optimize for size
- `lto = true` - Link-time optimization
- `codegen-units = 1` - Better optimization
- `strip = true` - Strip symbols
- `panic = "abort"` - Smaller binary

### Profile-Guided Optimization (PGO)
```bash
# 1. Build instrumented binary
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo build --release

# 2. Run workload to generate profile data
./target/release/bmcwebd-ng &
# ... run typical workload ...
killall bmcwebd-ng

# 3. Merge profile data
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# 4. Build with PGO
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" cargo build --release
```

## Benchmarking

```bash
# Install criterion for benchmarks
cargo install cargo-criterion

# Run benchmarks
cargo bench

# Generate benchmark report
cargo criterion --message-format=json > benchmark.json
```

## Documentation

### Generate API Documentation
```bash
# Generate and open documentation
cargo doc --open

# Generate documentation for all dependencies
cargo doc --open --document-private-items
```

## Continuous Integration

### GitHub Actions (Example)
```yaml
name: Build and Test

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: cargo build --release
      - run: cargo test
      - run: cargo clippy -- -D warnings
```

## Additional Resources

- Rust Book: https://doc.rust-lang.org/book/
- Cargo Book: https://doc.rust-lang.org/cargo/
- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- tokio Documentation: https://tokio.rs/
- axum Documentation: https://docs.rs/axum/
- zbus Documentation: https://docs.rs/zbus/