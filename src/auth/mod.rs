//! Authentication and authorization
//!
//! Implements various authentication methods and privilege checking

pub mod basic;
pub mod session;
pub mod middleware;

pub use session::{SessionStore, SessionType, UserSession};
pub use middleware::{auth_middleware, optional_auth_middleware, unauthorized_response};

// TODO: Implement additional authentication modules:
// - token.rs - JWT token-based authentication
// - mtls.rs - Mutual TLS authentication
// - privilege.rs - Redfish privilege checking
// - ldap.rs - LDAP/Active Directory integration
