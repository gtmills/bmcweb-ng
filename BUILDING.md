# Building bmcweb-ng

This guide provides detailed instructions for building bmcweb-ng from source.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Platform-Specific Instructions](#platform-specific-instructions)
- [Build Configurations](#build-configurations)
- [Cross-Compilation](#cross-compilation)
- [Docker Build](#docker-build)
- [Troubleshooting](#troubleshooting)

## Prerequisites

### Required

- **Rust**: 1.75 or later
- **Cargo**: Comes with Rust
- **C Compiler**: GCC or Clang
- **pkg-config**: For finding system libraries

### System Libraries

- **OpenSSL**: 3.0+ (for TLS support)
- **DBus**: 1.12+ (for OpenBMC communication)
- **PAM**: For authentication (Linux only)

## Quick Start

```bash
# Clone the repository
git clone https://github.ibm.com/gmills/bmcweb-ng
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
    libpam0g-dev

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
    pkg-config \
    openssl-devel \
    dbus-devel \
    pam-devel

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

### WSL2 (Windows Subsystem for Linux)

```bash
# Use Ubuntu/Debian instructions above
# WSL2 provides a full Linux environment

# Install WSL2 if not already installed (PowerShell as Admin):
# wsl --install -d Ubuntu

# Then follow Ubuntu instructions
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

# Build (note: PAM support may be limited on macOS)
cargo build --release
```

## Build Configurations

### Debug Build

```bash
# Fast compilation, includes debug symbols, no optimizations
cargo build

# Binary location: target/debug/bmcwebd-ng
# Size: ~50-100MB (with debug symbols)
```

### Release Build

```bash
# Optimized for performance and size
cargo build --release

# Binary location: target/release/bmcwebd-ng
# Size: <1MB (stripped)
```

### Minimum Size Build

```bash
# Optimize for binary size
cargo build --release --config profile.release.opt-level='"z"'

# Further reduce size by stripping symbols
strip target/release/bmcwebd-ng

# Expected size: <800KB
```

### Development Build with Fast Compilation

```bash
# Use mold linker for faster linking (Linux only)
sudo apt-get install mold  # or: cargo install mold

# Add to .cargo/config.toml:
# [target.x86_64-unknown-linux-gnu]
# linker = "clang"
# rustflags = ["-C", "link-arg=-fuse-ld=mold"]

cargo build
```

## Build Features

### Feature Flags

bmcweb-ng supports conditional compilation via Cargo features:

```bash
# Build with all features (default)
cargo build --release

# Build with specific features
cargo build --release --no-default-features --features "redfish,websocket"

# Available features:
# - redfish: Redfish API support (default)
# - websocket: WebSocket support (default)
# - kvm: KVM over WebSocket (default)
# - virtual-media: Virtual media support (default)
# - event-service: Event subscription service (default)
# - metrics: Prometheus metrics (default)
# - tracing: OpenTelemetry tracing (default)
```

### Build Profiles

Custom build profiles in `Cargo.toml`:

```toml
[profile.release]
opt-level = "z"          # Optimize for size
lto = true               # Link-time optimization
codegen-units = 1        # Better optimization
strip = true             # Strip symbols
panic = "abort"          # Smaller binary

[profile.dev]
opt-level = 0            # No optimization
debug = true             # Include debug info

[profile.bench]
inherits = "release"
debug = true             # Keep debug info for profiling
```

## Cross-Compilation

### ARM64 (aarch64)

```bash
# Install cross-compilation toolchain
rustup target add aarch64-unknown-linux-gnu
sudo apt-get install gcc-aarch64-linux-gnu

# Configure Cargo for cross-compilation
cat >> ~/.cargo/config.toml << EOF
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

# Build
cargo build --release --target aarch64-unknown-linux-gnu

# Binary location: target/aarch64-unknown-linux-gnu/release/bmcwebd-ng
```

### ARM32 (armv7)

```bash
# Install cross-compilation toolchain
rustup target add armv7-unknown-linux-gnueabihf
sudo apt-get install gcc-arm-linux-gnueabihf

# Configure Cargo
cat >> ~/.cargo/config.toml << EOF
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF

# Build
cargo build --release --target armv7-unknown-linux-gnueabihf
```

### Using cross

```bash
# Install cross (Docker-based cross-compilation)
cargo install cross

# Build for ARM64
cross build --release --target aarch64-unknown-linux-gnu

# Build for ARM32
cross build --release --target armv7-unknown-linux-gnueabihf

# Build for RISC-V
cross build --release --target riscv64gc-unknown-linux-gnu
```

## Docker Build

### Build Container

```dockerfile
# Dockerfile
FROM rust:1.75-slim as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libdbus-1-dev \
    libpam0g-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy source
COPY . .

# Build
RUN cargo build --release

# Runtime container
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libdbus-1-3 \
    libpam0g \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /build/target/release/bmcwebd-ng /usr/local/bin/

# Copy config
COPY config.toml /etc/bmcweb/config.toml

# Expose ports
EXPOSE 443 9090

# Run
CMD ["bmcwebd-ng", "--config", "/etc/bmcweb/config.toml"]
```

### Build and Run

```bash
# Build Docker image
docker build -t bmcweb-ng:latest .

# Run container
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
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_service_root

# Run integration tests
cargo test --test '*'

# Run benchmarks
cargo bench
```

### Check Code Quality

```bash
# Run clippy (linter)
cargo clippy -- -D warnings

# Check formatting
cargo fmt --check

# Generate documentation
cargo doc --no-deps --open
```

### Verify Binary

```bash
# Check binary size
ls -lh target/release/bmcwebd-ng

# Check dependencies
ldd target/release/bmcwebd-ng

# Run binary
./target/release/bmcwebd-ng --version
./target/release/bmcwebd-ng --help
```

## Troubleshooting

### OpenSSL Not Found

```bash
# Ubuntu/Debian
sudo apt-get install libssl-dev pkg-config

# Fedora/RHEL
sudo dnf install openssl-devel pkg-config

# macOS
brew install openssl pkg-config
export PKG_CONFIG_PATH="/usr/local/opt/openssl/lib/pkgconfig"
```

### DBus Not Found

```bash
# Ubuntu/Debian
sudo apt-get install libdbus-1-dev

# Fedora/RHEL
sudo dnf install dbus-devel
```

### PAM Not Found

```bash
# Ubuntu/Debian
sudo apt-get install libpam0g-dev

# Fedora/RHEL
sudo dnf install pam-devel
```

### Linker Errors

```bash
# Install build essentials
sudo apt-get install build-essential

# Or specify linker explicitly
export RUSTFLAGS="-C linker=gcc"
cargo build --release
```

### Out of Memory During Build

```bash
# Reduce parallel jobs
cargo build --release -j 2

# Or use less optimization
cargo build --release --config profile.release.opt-level=2
```

### Slow Compilation

```bash
# Use sccache for caching
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux)
sudo apt-get install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# Incremental compilation (enabled by default in debug)
export CARGO_INCREMENTAL=1
```

### Cross-Compilation Issues

```bash
# Ensure target is installed
rustup target list --installed

# Install missing target
rustup target add <target-triple>

# Use cross for easier cross-compilation
cargo install cross
cross build --target <target-triple>
```

## Build Environment Variables

```bash
# Rust compiler flags
export RUSTFLAGS="-C target-cpu=native"

# Cargo build jobs
export CARGO_BUILD_JOBS=4

# Enable incremental compilation
export CARGO_INCREMENTAL=1

# Use sccache
export RUSTC_WRAPPER=sccache

# OpenSSL location (if not in standard path)
export OPENSSL_DIR=/usr/local/opt/openssl
export PKG_CONFIG_PATH="/usr/local/opt/openssl/lib/pkgconfig"
```

## CI/CD Integration

### GitHub Actions

```yaml
name: Build

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
        run: cargo build --release
      - name: Test
        run: cargo test
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
# Run typical workload...
killall bmcwebd-ng

# Step 3: Merge profile data
llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# Step 4: Build with profile data
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" cargo build --release
```

### Link-Time Optimization (LTO)

Already enabled in release profile:
```toml
[profile.release]
lto = true  # Enable LTO
```

## Additional Resources

- [Rust Installation Guide](https://www.rust-lang.org/tools/install)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Cross-Compilation Guide](https://rust-lang.github.io/rustup/cross-compilation.html)
- [OpenBMC Development](https://github.com/openbmc/docs)