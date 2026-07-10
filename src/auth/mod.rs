//! Authentication and authorization
//!
//! Implements various authentication methods, session management, and
//! Redfish privilege checking per DSP0272 PrivilegeRegistry.

pub mod basic;
pub mod middleware;
pub mod privilege;
pub mod session;

pub use middleware::{auth_middleware, extract_client_ip, optional_auth_middleware, unauthorized_response};
pub use privilege::{check_privilege, privileges_for_role, Privilege, PrivilegeSet};
pub use session::{SessionStore, SessionType, UserSession};

// TODO: Implement additional authentication modules:
// - mtls.rs - Mutual TLS (X.509 certificate) authentication
// - ldap.rs - LDAP/Active Directory integration
