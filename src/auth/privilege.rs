//! Redfish Privilege System
//!
//! Implements Redfish role-based access control per the Redfish PrivilegeRegistry
//! (DSP0272). Every HTTP method on every Redfish endpoint has a required set of
//! privileges. A request is allowed only if the session's role grants at least
//! one of the required privileges for that method.
//!
//! Privilege model
//! ---------------
//! Redfish defines five standard privileges:
//!   - Login              – read-only access to the service
//!   - ConfigureManager   – change BMC-level settings
//!   - ConfigureUsers     – manage user accounts
//!   - ConfigureSelf      – change only the caller's own account
//!   - ConfigureComponents – control managed components (power, boot, etc.)
//!
//! The four built-in roles map to these privileges:
//!   - Administrator: all five
//!   - Operator:      Login + ConfigureSelf + ConfigureComponents
//!   - ReadOnly:      Login + ConfigureSelf
//!   - NoAccess:      (none)

use std::collections::HashSet;
use std::fmt;

/// The five Redfish standard privileges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Privilege {
    Login,
    ConfigureManager,
    ConfigureUsers,
    ConfigureSelf,
    ConfigureComponents,
}

impl fmt::Display for Privilege {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Privilege::Login => "Login",
            Privilege::ConfigureManager => "ConfigureManager",
            Privilege::ConfigureUsers => "ConfigureUsers",
            Privilege::ConfigureSelf => "ConfigureSelf",
            Privilege::ConfigureComponents => "ConfigureComponents",
        };
        write!(f, "{}", s)
    }
}

impl Privilege {
    /// Parse a privilege name string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Login" => Some(Privilege::Login),
            "ConfigureManager" => Some(Privilege::ConfigureManager),
            "ConfigureUsers" => Some(Privilege::ConfigureUsers),
            "ConfigureSelf" => Some(Privilege::ConfigureSelf),
            "ConfigureComponents" => Some(Privilege::ConfigureComponents),
            _ => None,
        }
    }
}

/// The set of privileges held by a user.
#[derive(Debug, Clone)]
pub struct PrivilegeSet(HashSet<Privilege>);

impl PrivilegeSet {
    /// Create an empty privilege set.
    pub fn empty() -> Self {
        PrivilegeSet(HashSet::new())
    }

    /// Check whether this set contains all of the given privileges.
    pub fn is_superset_of<'a>(&self, required: impl IntoIterator<Item = &'a Privilege>) -> bool {
        required.into_iter().all(|p| self.0.contains(p))
    }

    /// Check whether this set contains at least one privilege from the list.
    pub fn satisfies_any(&self, options: &[Privilege]) -> bool {
        options.iter().any(|p| self.0.contains(p))
    }

    /// Return the set of privilege names as strings.
    pub fn as_strings(&self) -> Vec<String> {
        self.0.iter().map(|p| p.to_string()).collect()
    }
}

/// The built-in Redfish roles and their privilege assignments.
///
/// Returns the [`PrivilegeSet`] corresponding to the named role.
pub fn privileges_for_role(role_id: &str) -> PrivilegeSet {
    let privs: &[Privilege] = match role_id {
        "Administrator" => &[
            Privilege::Login,
            Privilege::ConfigureManager,
            Privilege::ConfigureUsers,
            Privilege::ConfigureSelf,
            Privilege::ConfigureComponents,
        ],
        "Operator" => &[
            Privilege::Login,
            Privilege::ConfigureSelf,
            Privilege::ConfigureComponents,
        ],
        "ReadOnly" => &[Privilege::Login, Privilege::ConfigureSelf],
        // NoAccess and unknown roles get no privileges
        _ => &[],
    };

    PrivilegeSet(privs.iter().cloned().collect())
}

// ---------------------------------------------------------------------------
// Route-level privilege requirements
//
// These constants mirror the Redfish PrivilegeRegistry (DSP0272) for the
// routes that bmcweb-ng currently implements.
// ---------------------------------------------------------------------------

/// Privileges required to GET (read) most Redfish resources.
pub const PRIVILEGE_GET: &[Privilege] = &[Privilege::Login];

/// Privileges required to POST (create) most Redfish resources.
pub const PRIVILEGE_POST: &[Privilege] = &[Privilege::ConfigureManager];

/// Privileges required to PATCH (modify) most Redfish resources.
pub const PRIVILEGE_PATCH: &[Privilege] = &[Privilege::ConfigureManager];

/// Privileges required to DELETE most Redfish resources.
pub const PRIVILEGE_DELETE: &[Privilege] = &[Privilege::ConfigureManager];

/// Privileges required to perform system/chassis/manager actions.
pub const PRIVILEGE_ACTION: &[Privilege] = &[Privilege::ConfigureComponents];

/// Privileges required to create a session (login — intentionally empty).
pub const PRIVILEGE_CREATE_SESSION: &[Privilege] = &[];

/// Privileges required to delete any session (own session or all sessions).
pub const PRIVILEGE_DELETE_SESSION: &[Privilege] = &[Privilege::ConfigureSelf];

/// Privileges required to manage user accounts.
pub const PRIVILEGE_CONFIGURE_USERS: &[Privilege] = &[Privilege::ConfigureUsers];

// ---------------------------------------------------------------------------
// Axum extractor / check helper
// ---------------------------------------------------------------------------

use axum::http::StatusCode;

use crate::auth::session::UserSession;

/// Check whether the given session holds at least one of the required
/// privileges and return the appropriate HTTP status code.
///
/// Returns `Ok(())` if access is permitted, `Err(StatusCode::FORBIDDEN)` if
/// the session exists but lacks the required privilege, and
/// `Err(StatusCode::UNAUTHORIZED)` if there is no session at all.
pub fn check_privilege(
    session: Option<&UserSession>,
    required: &[Privilege],
) -> Result<(), StatusCode> {
    // Open endpoints (empty required list) are always permitted.
    if required.is_empty() {
        return Ok(());
    }

    let session = session.ok_or(StatusCode::UNAUTHORIZED)?;
    let priv_set = privileges_for_role(&session_role(session));

    if priv_set.satisfies_any(required) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Extract the role string from a session, defaulting to "ReadOnly" for
/// sessions that were created via Basic auth and have no explicit role set.
fn session_role(_session: &UserSession) -> String {
    // In the current implementation, the role is not yet stored in the session.
    // TODO: Store the role in UserSession after fetching it from DBus
    //       xyz.openbmc_project.User.Manager.GetUserInfo during session creation.
    // For now, default to ReadOnly so that Basic-auth sessions can at least
    // read Redfish resources.
    "ReadOnly".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use crate::auth::session::{SessionType, UserSession};

    fn make_session(role: &str) -> UserSession {
        let mut s = UserSession::new(
            "testuser".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );
        s
    }

    #[test]
    fn test_administrator_has_all_privileges() {
        let priv_set = privileges_for_role("Administrator");
        assert!(priv_set.satisfies_any(&[Privilege::Login]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureManager]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureUsers]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureSelf]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureComponents]));
    }

    #[test]
    fn test_readonly_only_has_login_and_configure_self() {
        let priv_set = privileges_for_role("ReadOnly");
        assert!(priv_set.satisfies_any(&[Privilege::Login]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureSelf]));
        assert!(!priv_set.satisfies_any(&[Privilege::ConfigureManager]));
        assert!(!priv_set.satisfies_any(&[Privilege::ConfigureComponents]));
    }

    #[test]
    fn test_no_access_has_no_privileges() {
        let priv_set = privileges_for_role("NoAccess");
        assert!(!priv_set.satisfies_any(&[Privilege::Login]));
    }

    #[test]
    fn test_check_privilege_open_endpoint() {
        // Empty required list = always allowed even without a session
        assert!(check_privilege(None, PRIVILEGE_CREATE_SESSION).is_ok());
    }

    #[test]
    fn test_check_privilege_no_session() {
        assert_eq!(
            check_privilege(None, PRIVILEGE_GET),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn test_privilege_from_str() {
        assert_eq!(Privilege::from_str("Login"), Some(Privilege::Login));
        assert_eq!(Privilege::from_str("ConfigureManager"), Some(Privilege::ConfigureManager));
        assert_eq!(Privilege::from_str("Unknown"), None);
    }

    #[test]
    fn test_operator_privileges() {
        let priv_set = privileges_for_role("Operator");
        assert!(priv_set.satisfies_any(&[Privilege::Login]));
        assert!(priv_set.satisfies_any(&[Privilege::ConfigureComponents]));
        assert!(!priv_set.satisfies_any(&[Privilege::ConfigureManager]));
        assert!(!priv_set.satisfies_any(&[Privilege::ConfigureUsers]));
    }
}
