# Contributing to bmcweb-ng

Thank you for your interest in contributing to bmcweb-ng! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Code Style](#code-style)
- [Documentation](#documentation)

## Code of Conduct

This project follows the OpenBMC Code of Conduct. Please be respectful and professional in all interactions.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.ibm.com/YOUR-USERNAME/bmcweb-ng
   cd bmcweb-ng
   ```
3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.ibm.com/gmills/bmcweb-ng
   ```

## Development Setup

### Prerequisites

- Rust 1.75 or later
- Linux development environment (native, WSL2, or Docker)
- OpenSSL development libraries
- DBus development libraries

### Installing Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    libssl-dev \
    libdbus-1-dev \
    pkg-config
```

**Fedora/RHEL:**
```bash
sudo dnf install -y \
    gcc \
    openssl-devel \
    dbus-devel \
    pkg-config
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

## Making Changes

### Branch Naming

Use descriptive branch names:
- `feature/add-xyz` - New features
- `fix/issue-123` - Bug fixes
- `docs/update-readme` - Documentation updates
- `refactor/cleanup-auth` - Code refactoring

### Commit Messages

Follow conventional commit format:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

**Example:**
```
feat(auth): Add mTLS authentication support

Implement mutual TLS authentication for enhanced security.
This allows clients to authenticate using X.509 certificates.

Closes #42
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_service_root

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test '*'
```

### Writing Tests

- Write unit tests in the same file as the code
- Write integration tests in `tests/` directory
- Use descriptive test names
- Test both success and failure cases
- Mock external dependencies (DBus, etc.)

**Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_root_returns_valid_json() {
        let result = get_service_root();
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["@odata.type"], "#ServiceRoot.v1_15_0.ServiceRoot");
    }

    #[tokio::test]
    async fn test_async_operation() {
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

## Submitting Changes

### Pull Request Process

1. **Update your fork**:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Create a feature branch**:
   ```bash
   git checkout -b feature/my-feature
   ```

3. **Make your changes** and commit them

4. **Run tests and linting**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

5. **Push to your fork**:
   ```bash
   git push origin feature/my-feature
   ```

6. **Create a Pull Request** on GitHub

### Pull Request Guidelines

- Provide a clear description of the changes
- Reference related issues (e.g., "Fixes #123")
- Ensure all tests pass
- Update documentation if needed
- Keep PRs focused on a single feature/fix
- Respond to review feedback promptly

## Code Style

### Rust Style Guide

We follow the official Rust style guide with these additions:

1. **Formatting**: Use `rustfmt` with default settings
   ```bash
   cargo fmt
   ```

2. **Linting**: Address all `clippy` warnings
   ```bash
   cargo clippy -- -D warnings
   ```

3. **Naming Conventions**:
   - `snake_case` for functions, variables, modules
   - `PascalCase` for types, traits, enums
   - `SCREAMING_SNAKE_CASE` for constants
   - Descriptive names over abbreviations

4. **Error Handling**:
   - Use `Result<T, E>` for fallible operations
   - Use `anyhow::Result` for application errors
   - Use `thiserror` for library errors
   - Avoid `unwrap()` in production code

5. **Documentation**:
   - Document all public APIs
   - Use `///` for doc comments
   - Include examples in doc comments
   - Document panics, errors, and safety

**Example:**
```rust
/// Retrieves the Redfish service root resource.
///
/// # Returns
///
/// Returns a JSON object containing the service root information
/// including API version, UUID, and links to major resource collections.
///
/// # Errors
///
/// Returns an error if the service root cannot be generated or
/// if required system information is unavailable.
///
/// # Example
///
/// ```
/// use bmcweb_ng::api::redfish::get_service_root;
///
/// let service_root = get_service_root()?;
/// println!("Redfish Version: {}", service_root["RedfishVersion"]);
/// ```
pub fn get_service_root() -> Result<serde_json::Value> {
    // Implementation
}
```

## Documentation

### Types of Documentation

1. **Code Documentation**: Inline comments and doc comments
2. **API Documentation**: Generated from doc comments (`cargo doc`)
3. **User Documentation**: README, guides, tutorials
4. **Architecture Documentation**: Design decisions, diagrams

### Updating Documentation

When making changes, update:
- Inline code comments for complex logic
- Doc comments for public APIs
- README.md for user-facing changes
- Architecture docs for design changes
- CHANGELOG.md for notable changes

### Generating Documentation

```bash
# Generate and open documentation
cargo doc --open

# Generate documentation for all dependencies
cargo doc --no-deps --open
```

## Questions?

If you have questions or need help:
- Open an issue on GitHub
- Contact the maintainers
- Check existing documentation and issues

Thank you for contributing to bmcweb-ng!