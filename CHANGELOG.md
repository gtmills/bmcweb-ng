# Changelog

All notable changes to bmcweb-ng will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project structure and architecture
- Redfish ServiceRoot endpoint implementation
- Configuration management with TOML support
- Basic authentication framework
- DBus abstraction layer
- Comprehensive documentation (CONTRIBUTING.md, ARCHITECTURE.md, BUILDING.md)
- Cargo.toml with all required dependencies
- Module structure for API, services, auth, config, dbus, protocol, and observability layers

### Changed
- Removed benchmark configuration from Cargo.toml (missing benchmark files)

### Fixed
- N/A

### Security
- N/A

## [0.1.0] - 2026-04-28

### Added
- Initial project setup
- Basic Rust project structure
- README with project overview and roadmap
- Apache 2.0 license
- Git repository initialization

[Unreleased]: https://github.ibm.com/gmills/bmcweb-ng/compare/v0.1.0...HEAD
[0.1.0]: https://github.ibm.com/gmills/bmcweb-ng/releases/tag/v0.1.0