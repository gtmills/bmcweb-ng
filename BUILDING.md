# Building bmcweb-ng

This guide covers everything needed to build, install, and cross-compile bmcweb-ng.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Platform-Specific Instructions](#platform-specific-instructions)
- [Build Configurations](#build-configurations)
- [Build Features](#build-features)
- [Cross-Compilation](#cross-compilation)
- [Installation](#installation)
- [Development Workflow](#development-workflow)
- [Docker Build](#docker-build)
- [Build Verification](#build-verification)
- [Troubleshooting](#troubleshooting)
- [Build Environment Variables](#build-environment-variables)
- [CI/CD Integration](#cicd-integration)
- [Performance Optimization](#performance-optimization)

## Prerequisites

### System Requirements

- **Operating System**: Linux (Ubuntu 22.04+, Fedora 38+, Arch, or similar)
- **Architecture**: x86_64 or ARM64 for building; cross-compiles to ARM32
- **Memory**: 2 GB RAM minimum for building
- **Disk Space**: 1 GB free space

### Required Software

- **Rust**: 1.75 or later (install via [rustup](https://rustup.rs/))
- **Cargo**: Comes with Rust
- **C Compiler**: GCC or Clang
- **pkg-config**: For finding system libraries

### System Libraries

- **OpenSSL**: 3.0+ (for TLS support)
- **DBus**: 1.12+ (for OpenBMC communication)
- **PAM**: For authentication (Linux only; optional — see [Feature Flags](#feature-flags))

## Quick Start

```bash
# Clone the repository
git clone https://github.com/gtmills/bmcweb-ng
cd bmcweb-ng

# Build in debug mode
cargo build

# Build in release mode (optimized)
cargo build --release

# Run the binary
./target/release/bmcwebd-ng --config config.toml
```

## Platform-Specific Instructions

### Ubuntu/Debian

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install system dependencies
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libdbus-1-dev \
    libpam0g-dev \
    libsystemd-dev

# Build
cargo build --release
```

### Fedora/RHEL/CentOS

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install system dependencies
sudo dnf install -y \
    gcc \
    gcc-c++ \
    pkg-config \
    openssl-devel \
    dbus-devel \
    pam-devel \
    systemd-devel

# Build
cargo build --release
```

### Arch Linux

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install system dependencies
sudo pacman -S \
    base-devel \
    pkg-config \
    openssl \
    pam \
    dbus \
    systemd

# Build
cargo build --release
```

### OpenBMC (Yocto)

```bash
# Add to your Yocto recipe
DEPENDS += "openssl dbus pam"

# In your recipe
do_compile() {
    cargo build --release --target=${RUST_TARGET_SYS}
}

do_install() {
    install -d ${D}${bindir}
    install -m 0755 ${B}/target/${RUST_TARGET_SYS}/release/bmcwebd-ng ${D}${bindir}/
}
```

The Yocto BitBake recipe is located at:

```
meta-phosphor/recipes-phosphor/interfaces/bmcweb-ng_git.bb
```

To build with Yocto:

```bash
# In your OpenBMC build directory
bitbake bmcweb-ng

# Clean and rebuild
bitbake -c clean bmcweb-ng && bitbake bmcweb-ng

# Deploy to target
bitbake bmcweb-ng -c deploy
```

### WSL2 (Windows Subsystem for Linux)

```bash
# Use Ubuntu/Debian instructions above.
# WSL2 provides a full Linux environment.

# Install WSL2 if not already installed (PowerShell as Admin):
# wsl --install -d Ubuntu
# Then open Ubuntu and follow the Ubuntu instructions above.
```

### macOS

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install dependencies via Homebrew
brew install pkg-config openssl dbus

# Set environment variables for OpenSSL
export PKG_CONFIG_PATH="/usr/local/opt/openssl/lib/pkgconfig"

# Build without PAM (macOS does not have libpam in the same location)
cargo build --release --no-default-features
```

## Build Configurations

### Debug Build

```bash
# Fast compilation, includes debug symbols, no optimizations
cargo build

# Binary location: target/debug/bmcwebd-ng
# Size: ~50–100 MB (with debug symbols)
```

### Release Build

```bash
# Optimized for performance and size
cargo build --release

# Binary location: target/release/bmcwebd-ng
# Size: ~5 MB (dynamically-linked ARM EABI release)
```

### Minimum Size Build

The `Cargo.toml` release profile already sets `opt-level = "z"` and strips symbols.
You can squeeze a bit more by stripping manually after build:

```bash
cargo build --release
strip target/release/bmcwebd-ng
# Expected size: ~4–5 MB (dynamically-linked); ~3–4 MB with musl static target
```

### Development Build with Fast Compilation

```bash
# Use mold linker for faster linking (Linux only)
sudo apt-get install mold

# Add to .cargo/config.toml:
# [target.x86_64-unknown-linux-gnu]
# linker = "clang"
# rustflags = ["-C", "link-arg=-fuse-ld=mold"]

cargo build
```

## Build Features

### Feature Flags

bmcweb-ng has one conditional feature flag:

```bash
# Build with PAM authentication enabled (Linux production default)
cargo build --release --features pam

# Build without PAM — auth stubs accept all logins
# (used for QEMU/ARM cross-compilation where a PAM ARM sysroot is unavailable)
cargo build --release

# Available features:
# - pam: real PAM-based authentication (requires libpam headers at build time)
```

> **Note**: `pam` is **not** in the `default` feature set so that
> `cargo build --release --target arm-unknown-linux-gnueabihf` works without
> an ARM PAM sysroot. Enable it explicitly for production Linux x86_64 builds.

### Build Profiles

The release profile is already tuned for size in `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"      # Optimize for size
lto = true           # Link-time optimization
codegen-units = 1    # Better optimization
strip = true         # Strip symbols
panic = "abort"      # Smaller binary
```

## Cross-Compilation

### ARM32 (arm-unknown-linux-gnueabihf) — Primary OpenBMC Target

This is the required target for OpenBMC `qemuarm` and production AST2600/AST2700 hardware.
The repository's `.cargo/config.toml` already configures the linker for this target.

```bash
# Install cross-compilation toolchain
rustup target add arm-unknown-linux-gnueabihf
sudo apt-get install gcc-arm-linux-gnueabihf

# Build (pam feature omitted — no ARM PAM sysroot needed)
# Note: the release profile enables full LTO, which requires the linker env var
# to avoid the host x86_64 LLD being injected by rustc on LTO-linked binaries.
CC=arm-linux-gnueabihf-gcc \
CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
cargo build --release --target arm-unknown-linux-gnueabihf

# Binary location: target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng
```

### ARM64 (aarch64-unknown-linux-gnu)

```bash
# Install cross-compilation toolchain
rustup target add aarch64-unknown-linux-gnu
sudo apt-get install gcc-aarch64-linux-gnu

# Configure Cargo for cross-compilation (if not already set)
cat >> ~/.cargo/config.toml << EOF
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

# Build
cargo build --release --target aarch64-unknown-linux-gnu

# Binary location: target/aarch64-unknown-linux-gnu/release/bmcwebd-ng
```

### Using `cross` (Docker-based)

```bash
# Install cross
cargo install cross

# Build for ARM32 (OpenBMC target)
cross build --release --target arm-unknown-linux-gnueabihf

# Build for ARM64
cross build --release --target aarch64-unknown-linux-gnu

# Build for RISC-V
cross build --release --target riscv64gc-unknown-linux-gnu
```

## Installation

### System-wide Installation

```bash
# Build release binary
cargo build --release

# Install binary
sudo install -m 755 target/release/bmcwebd-ng /usr/bin/

# Create service user and groups
sudo useradd -r -s /sbin/nologin bmcweb-ng
sudo groupadd -r web
sudo groupadd -r redfish
sudo groupadd -r hostconsole
sudo usermod -a -G web,redfish,hostconsole bmcweb-ng

# Create directories
sudo mkdir -p /etc/bmcweb /var/lib/bmcweb /var/log/bmcweb
sudo chown bmcweb-ng:bmcweb-ng /var/lib/bmcweb /var/log/bmcweb

# Install configuration and systemd files
sudo install -m 644 config.toml /etc/bmcweb/
sudo install -m 644 bmcweb-ng.service /etc/systemd/system/
sudo install -m 644 bmcweb-ng.socket  /etc/systemd/system/

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable --now bmcweb-ng.socket

# Check status
sudo systemctl status bmcweb-ng.socket bmcweb-ng.service
```

### Verify Installation

```bash
# Check service is active
systemctl is-active bmcweb-ng.service

# Follow logs
journalctl -u bmcweb-ng.service -f

# Test endpoints
curl http://localhost/health
curl -u root:0penBmc http://localhost/redfish/v1
```

## Development Workflow

### Iterative Development

```bash
# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

### Auto-Rebuild on File Changes

```bash
# Install cargo-watch
cargo install cargo-watch

# Auto-rebuild and run on file changes
cargo watch -x run

# Auto-test on file changes
cargo watch -x test
```

### Debugging

```bash
# Build with debug symbols
cargo build

# Run with GDB
rust-gdb target/debug/bmcwebd-ng

# Run with LLDB
rust-lldb target/debug/bmcwebd-ng

# Enable backtrace on panic
RUST_BACKTRACE=1 cargo run
RUST_BACKTRACE=full cargo run
```

### Logging

```bash
# Info-level logging (default)
RUST_LOG=info cargo run

# Debug-level logging
RUST_LOG=debug cargo run

# Trace-level (very verbose)
RUST_LOG=trace cargo run

# Per-module filtering
RUST_LOG=bmcweb_ng::dbus=debug,bmcweb_ng::auth=info cargo run
```

## Docker Build

### Build Container

```dockerfile
FROM rust:1.75-slim as builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libdbus-1-dev libpam0g-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN cargo build --release --features pam

# Runtime container
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 libdbus-1-3 libpam0g ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/bmcwebd-ng /usr/local/bin/
COPY config.toml /etc/bmcweb/config.toml

EXPOSE 443 9090
CMD ["bmcwebd-ng", "--config", "/etc/bmcweb/config.toml"]
```

### Build and Run

```bash
docker build -t bmcweb-ng:latest .

docker run -d \
    --name bmcweb-ng \
    -p 443:443 \
    -p 9090:9090 \
    -v /etc/bmcweb:/etc/bmcweb \
    bmcweb-ng:latest
```

## Build Verification

### Run Tests

```bash
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_service_root

# Run all integration tests
cargo test --test '*'
```

### Check Code Quality

```bash
# Linter
cargo clippy -- -D warnings

# Formatting check
cargo fmt --check

# Generate documentation
cargo doc --no-deps --open
```

### Benchmarks

```bash
# Install cargo-criterion
cargo install cargo-criterion

# Run benchmarks
cargo bench

# Generate benchmark report
cargo criterion --message-format=json > benchmark.json
```

### Verify Binary

```bash
# Check binary size
ls -lh target/release/bmcwebd-ng

# Check runtime library dependencies
ldd target/release/bmcwebd-ng

# Smoke-test the binary
./target/release/bmcwebd-ng --version
./target/release/bmcwebd-ng --help
```

## Troubleshooting

### Build Errors

#### `error: linker 'cc' not found`
```bash
sudo apt-get install build-essential       # Ubuntu/Debian
sudo dnf install gcc gcc-c++              # Fedora/RHEL
```

#### `error: failed to run custom build command for 'openssl-sys'`
```bash
sudo apt-get install libssl-dev pkg-config    # Ubuntu/Debian
sudo dnf install openssl-devel pkg-config     # Fedora/RHEL
brew install openssl pkg-config               # macOS
export PKG_CONFIG_PATH="/usr/local/opt/openssl/lib/pkgconfig"  # macOS
```

#### `error: failed to run custom build command for 'pam-sys'`
```bash
# Either install libpam headers, or build without the pam feature:
sudo apt-get install libpam0g-dev    # Ubuntu/Debian
sudo dnf install pam-devel           # Fedora/RHEL
# -- OR --
cargo build --release                # (pam feature is off by default)
```

#### `error: failed to run custom build command for 'zbus'`
```bash
sudo apt-get install libdbus-1-dev    # Ubuntu/Debian
sudo dnf install dbus-devel           # Fedora/RHEL
```

### Runtime Errors

#### `Failed to establish DBus connection`
```bash
systemctl status dbus
sudo systemctl start dbus
ls -la /var/run/dbus/system_bus_socket
```

#### `Permission denied` when binding to port 443
```bash
# Option 1: Use systemd socket activation (recommended)
sudo systemctl start bmcweb-ng.socket

# Option 2: Grant capability to bind privileged ports
sudo setcap 'cap_net_bind_service=+ep' target/release/bmcwebd-ng

# Option 3: Run as root (not recommended for production)
sudo ./target/release/bmcwebd-ng
```

#### `Address already in use`
```bash
sudo lsof -i :443
sudo lsof -i :80
sudo systemctl stop apache2   # or nginx, or whatever is conflicting
```

### Build Performance

#### Out of Memory During Build
```bash
cargo build --release -j 2    # Reduce parallel jobs
```

#### Slow Compilation
```bash
# Use sccache for build caching
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux only)
sudo apt-get install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# Incremental compilation (on by default in debug)
export CARGO_INCREMENTAL=1
```

#### Cross-Compilation Issues
```bash
# List installed targets
rustup target list --installed

# Install a missing target
rustup target add arm-unknown-linux-gnueabihf

# ARM build fails with "incompatible with elf64-x86-64" linker error
# This happens when rustc injects the host LLD into the ARM link command
# (occurs when lto = true in the release profile and a gcc-ld wrapper is
# installed on the host).  Pass the cross-compiler via env vars:
CC=arm-linux-gnueabihf-gcc \
CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
cargo build --release --target arm-unknown-linux-gnueabihf

# Use cross for Docker-based cross-compilation (avoids the LTO linker issue)
cargo install cross
cross build --target arm-unknown-linux-gnueabihf
```

## Build Environment Variables

```bash
# Rust compiler flags
export RUSTFLAGS="-C target-cpu=native"

# Parallelism
export CARGO_BUILD_JOBS=4

# Incremental compilation
export CARGO_INCREMENTAL=1

# Build cache
export RUSTC_WRAPPER=sccache

# OpenSSL location (if not in standard path)
export OPENSSL_DIR=/usr/local/opt/openssl
export PKG_CONFIG_PATH="/usr/local/opt/openssl/lib/pkgconfig"
```

## CI/CD Integration

### GitHub Actions

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
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libssl-dev libdbus-1-dev libpam0g-dev
      - name: Build
        run: cargo build --release --features pam
      - name: Test
        run: cargo test
      - name: Lint
        run: cargo clippy -- -D warnings
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: bmcwebd-ng
          path: target/release/bmcwebd-ng
```

## Performance Optimization

### Profile-Guided Optimization (PGO)

```bash
# Step 1: Build with instrumentation
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo build --release

# Step 2: Run workload to generate profile data
./target/release/bmcwebd-ng --config config.toml &
# Run typical workload against the server...
killall bmcwebd-ng

# Step 3: Merge profile data
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# Step 4: Build with profile data
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" cargo build --release
```

### Link-Time Optimization (LTO)

Already enabled in the release profile:

```toml
[profile.release]
lto = true  # Full LTO for maximum optimization
```

## Additional Resources

- [Rust Installation Guide](https://www.rust-lang.org/tools/install)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Cross-Compilation Guide](https://rust-lang.github.io/rustup/cross-compilation.html)
- [OpenBMC Development](https://github.com/openbmc/docs)
- [Tokio Documentation](https://tokio.rs/)
- [axum Documentation](https://docs.rs/axum/)
