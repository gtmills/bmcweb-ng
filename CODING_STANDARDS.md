# Coding Standards

This document defines the coding standards for bmcweb-ng. All contributions are
expected to follow these guidelines so that the codebase reads as if written by a
single developer.

---

## Table of Contents

- [Consistency](#consistency)
- [Readability](#readability)
- [Naming Conventions](#naming-conventions)
- [Structure and Formatting](#structure-and-formatting)
- [Documentation and Comments](#documentation-and-comments)
- [Error Handling](#error-handling)
- [Imports](#imports)
- [Tests](#tests)
- [DBus Conventions](#dbus-conventions)
- [Enforcement](#enforcement)

---

## Consistency

The goal is a codebase that looks like a single developer wrote every file.
Before submitting a change, read the surrounding code and match its style —
do not introduce a new pattern when an existing one already covers the case.

Run the following before every commit:

```bash
cargo fmt          # enforces formatting automatically
cargo clippy       # catches common mistakes and style divergence
```

Both must produce zero warnings on the changed code.

---

## Readability

- Prefer explicit over clever. A slightly longer expression that names its intent
  clearly is better than a compact one that requires a comment to decode.
- Limit function and method bodies to what fits on one screen (~40 lines).
  Split larger functions into named helpers.
- Avoid deeply nested control flow. Flatten with early returns (`return`,
  `continue`, `?`) rather than stacking `if`/`match` arms.
- Each function should do one thing. If its name requires "and" to describe it,
  split it.

---

## Naming Conventions

| Kind | Style | Examples |
|------|-------|---------|
| Functions and methods | `snake_case` | `get_property`, `host_state_to_power_state` |
| Variables and parameters | `snake_case` | `session_id`, `dbus_path`, `retry_count` |
| Types, structs, enums, traits | `PascalCase` | `DbusClient`, `UserSession`, `EventType` |
| Enum variants | `PascalCase` | `SessionType::Basic`, `TaskState::Running` |
| Constants and statics | `SCREAMING_SNAKE_CASE` | `MAX_SESSIONS`, `DEFAULT_TIMEOUT_SECS` |
| Modules and crate names | `snake_case` | `event_service`, `persistent_data` |
| Test functions | `snake_case` prefixed with `test_` | `test_session_creation`, `test_get_chassis` |

Names should be **descriptive** — a reader should understand the purpose without
reading the body. Avoid single-letter names except for short-lived loop indices
(`i`, `j`) and well-known math variables.

---

## Structure and Formatting

### Indentation and whitespace

- Use **4 spaces** per indent level. Tabs are not used anywhere in the project.
- Separate logical sections of a function body with a single blank line.
- Do not add trailing whitespace.
- End every file with a single newline.

`cargo fmt` enforces these rules automatically.

### Line length

Keep lines under **100 characters**. Break long expressions at operator or
argument boundaries and indent the continuation by one level:

```rust
// Good
let result = some_long_function_name(
    first_argument,
    second_argument,
    third_argument,
)?;

// Avoid
let result = some_long_function_name(first_argument, second_argument, third_argument)?;
```

### Braces

Opening braces go on the **same line** as the statement that introduces the
block (Rust standard):

```rust
// Good
if condition {
    do_something();
}

// Avoid
if condition
{
    do_something();
}
```

### Function and method size

- Target **fewer than 40 lines** per function, not counting doc comments and
  blank lines.
- Extract named helpers for repeated logic. A helper with a clear name is
  self-documenting and easy to unit-test independently.

### Module organisation

Each source file should contain one primary abstraction (a struct, a trait, or a
closely related group of free functions). Use the module hierarchy to communicate
intent:

```
src/
  api/redfish/       ← HTTP request handlers
  auth/              ← authentication and authorisation
  dbus/              ← DBus abstraction layer
  services/          ← business logic
  protocol/          ← HTTP/TLS server
  observability/     ← metrics and health
  config/            ← configuration loading
```

---

## Documentation and Comments

### Module-level documentation

Every `.rs` file starts with a `//!` module doc comment that describes:

1. **What** the module contains (one sentence).
2. **Key types or entry points** a reader should look for.
3. Any relevant external references (Redfish schemas, OpenBMC DBus service names).

```rust
//! Redfish Systems resource handlers.
//!
//! Covers `GET /redfish/v1/Systems`, `GET /Systems/system`, and all
//! sub-resources (Processors, Memory, Storage, LogServices, …).
//!
//! # DBus sources
//!
//! - Power state: `xyz.openbmc_project.State.Host / CurrentHostState`
//! - Boot settings: `xyz.openbmc_project.Control.Boot.Source`
```

### Public-item documentation

All `pub` functions, methods, structs, enums, and trait definitions carry a `///`
doc comment. The comment explains **why** the item exists and what callers should
know, not merely a restatement of the signature.

Use `# Arguments` and `# Returns` sections when the parameter names alone are not
self-explanatory:

```rust
/// Map an OpenBMC host-state string to a Redfish `PowerState` value.
///
/// Returns `"On"` for running/quiesced/diagnostic states, `"Off"` for
/// the off state, and `"Unknown"` for anything unrecognised or when DBus
/// is unavailable.
fn host_state_to_power_state(state: &str) -> &'static str {
```

### Inline comments

- Explain the **why**, not the **what**.
- Keep comments short and at the same indentation as the code they describe.
- Delete comments that are no longer accurate rather than leaving stale text.
- Do not comment out dead code — remove it. Version control holds the history.

```rust
// Good — explains a non-obvious constraint
// /tmp is tmpfs (~116 MB free); the rootfs is 96% full so /usr/bin is not writable.
let install_path = "/tmp/bmcwebd-ng";

// Avoid — restates what the code already says
// Set install_path to /tmp/bmcwebd-ng
let install_path = "/tmp/bmcwebd-ng";
```

---

## Error Handling

### Fallible functions

Use `anyhow::Result<T>` for internal functions that can fail. Add `.context()`
at every boundary where more information aids debugging:

```rust
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read config file: {}", path))?;
    toml::from_str(&text).context("Failed to parse config TOML")
}
```

### HTTP handlers

Handlers return explicit status codes using the `(StatusCode, Json<Value>)`
tuple pattern. Do not propagate `anyhow::Error` to the HTTP layer — convert it
to an appropriate status and a JSON error body at the handler boundary:

```rust
async fn get_system(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Build response; fall back gracefully when DBus is unavailable.
    (StatusCode::OK, axum::Json(body))
}
```

### `unwrap` and `expect`

- **Never** call `.unwrap()` or `.expect()` in production code paths (handler
  bodies, service methods, DBus calls).
- `.unwrap()` is **acceptable** in two places only:
  1. `RwLock::read()` / `RwLock::write()` guards — these only panic if a thread
     panicked while holding the lock, which is already an unrecoverable state.
  2. Test code inside `#[cfg(test)]` blocks, where a panic is a useful failure
     signal.

---

## Imports

Group `use` declarations in this order, with a blank line between groups:

1. Standard library (`std::`)
2. External crates (alphabetical within the group)
3. Crate-local (`crate::`)

```rust
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::AppState;
use crate::dbus::DbusClient;
```

`cargo fmt` enforces alphabetical ordering within each group automatically.
Do not use wildcard imports (`use foo::*`) except inside `#[cfg(test)]` blocks
where `use super::*` is idiomatic.

---

## Tests

### Location

Unit tests live in a `#[cfg(test)] mod tests` block at the **bottom** of the
file they test. Integration tests (if any) go under `tests/`.

### Naming

Test function names follow the pattern `test_<what>_<condition>`:

```rust
#[test]
fn test_session_creation() { … }

#[test]
fn test_get_chassis_not_found_no_dbus() { … }

#[tokio::test]
fn test_health_handler_returns_json() { … }
```

### Structure

Each test should be independent — no shared mutable state between tests.
Use the `MockDbusClient` for any test that would otherwise require a live DBus
connection.

Structure each test as:

1. **Arrange** — set up inputs and mocks.
2. **Act** — call the function under test.
3. **Assert** — check the result.

```rust
#[tokio::test]
async fn test_get_session_service() {
    // Arrange
    let config = Config::default();
    let state = Arc::new(AppState::new(config));

    // Act
    let result = get_session_service(State(state)).await;

    // Assert
    let (status, body) = result.into_response().into_parts();
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(body.0["Id"], "SessionService");
}
```

---

## DBus Conventions

### Service names, paths, and interfaces

DBus identifiers are string literals embedded in handler functions. Place them
as close as possible to their use site and document the corresponding OpenBMC
service in the enclosing function's doc comment. Use the established OpenBMC
naming scheme:

| Kind | Pattern | Example |
|------|---------|---------|
| Service (well-known name) | `xyz.openbmc_project.<service>` | `xyz.openbmc_project.State.Host` |
| Object path | `/xyz/openbmc_project/<category>/<id>` | `/xyz/openbmc_project/state/host0` |
| Interface | `xyz.openbmc_project.<category>.<Name>` | `xyz.openbmc_project.Control.Boot.Source` |
| Property | `PascalCase` string | `"CurrentHostState"`, `"RequestedHostTransition"` |

### Graceful fallback

Every handler that reads from DBus must fall back to a sensible default when
the DBus connection is absent or the call fails. Use `if let Some(client) = …`
and log a `warn!` at the point of fallback:

```rust
let power_state = if let Some(client) = dbus_client {
    match client.get_property(HOST_PATH, HOST_IFACE, "CurrentHostState").await {
        Ok(v) => host_state_to_power_state(v.as_str().unwrap_or("")),
        Err(e) => {
            warn!("Cannot read PowerState from DBus: {}", e);
            "Unknown"
        }
    }
} else {
    "Unknown"
};
```

---

## Enforcement

| Tool | Purpose | Run |
|------|---------|-----|
| `cargo fmt` | Formatting | `cargo fmt` — auto-fixes |
| `cargo clippy` | Lints and style | `cargo clippy -- -D warnings` |
| `cargo test` | Unit tests | `cargo test` |

All three must pass with zero errors and zero warnings before a pull request is
merged.
