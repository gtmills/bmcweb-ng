//! Authentication and authorization
//!
//! Implements various authentication methods, session management, and
//! Redfish privilege checking per DSP0272 PrivilegeRegistry.
//!
//! # Implemented
//!
//! - `basic.rs`     — HTTP Basic authentication
//! - `middleware.rs` — Axum auth middleware (mandatory + optional)
//! - `privilege.rs` — Redfish privilege model (DSP0272)
//! - `session.rs`   — Session token store with timeout and expiry
//!
//! # Planned
//!
//! - Mutual TLS (X.509 certificate) authentication
//! - LDAP / Active Directory integration

pub mod basic;
pub mod middleware;
pub mod privilege;
pub mod session;

pub use middleware::{auth_middleware, extract_client_ip, optional_auth_middleware, unauthorized_response};
pub use privilege::{check_privilege, privileges_for_role, Privilege, PrivilegeSet};
pub use session::{SessionStore, SessionType, UserSession};
