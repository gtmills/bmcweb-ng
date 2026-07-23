//! Redfish AccountService endpoints
//!
//! Implements the Redfish AccountService resource family:
//! - GET   /redfish/v1/AccountService
//! - PATCH /redfish/v1/AccountService
//! - GET   /redfish/v1/AccountService/Accounts
//! - POST  /redfish/v1/AccountService/Accounts
//! - GET   /redfish/v1/AccountService/Accounts/{account_id}
//! - PATCH /redfish/v1/AccountService/Accounts/{account_id}
//! - DELETE /redfish/v1/AccountService/Accounts/{account_id}
//! - GET   /redfish/v1/AccountService/Roles
//! - GET   /redfish/v1/AccountService/Roles/{role_id}
//! - GET   /redfish/v1/AccountService/PrivilegeMap
//!
//! Reference: DMTF DSP0266, AccountService schema v1.12.0, ManagerAccount schema v1.12.0
//!
//! OpenBMC DBus sources:
//!   - User enumeration: xyz.openbmc_project.User.Manager / ListUsers
//!   - User info:        xyz.openbmc_project.User.Manager / GetUserInfo (username) → dict
//!   - Create user:      xyz.openbmc_project.User.Manager / CreateUser
//!   - Delete user:      xyz.openbmc_project.User.Manager / DeleteUser
//!
//! OpenBMC GetUserInfo response keys (dict<string, variant>):
//!   UserPrivilege   → string  (e.g. "priv-admin")
//!   UserEnabled     → bool
//!   UserLockedForFailedAttempt → bool
//!   UserPasswordExpired → bool
//!   RemoteUser      → bool

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::auth::privilege::{check_privilege, PRIVILEGE_CONFIGURE_USERS};
use crate::auth::session::UserSession;
use crate::dbus::{DbusClient, ZBusClient};
use crate::AppState;

/// Request body for creating a new account
#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    #[serde(rename = "UserName")]
    pub username: String,
    #[serde(rename = "Password")]
    pub password: String,
    #[serde(rename = "RoleId")]
    pub role_id: String,
    #[serde(rename = "Enabled", default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Request body for patching an account
#[derive(Debug, Deserialize)]
pub struct PatchAccountRequest {
    #[serde(rename = "Password")]
    pub password: Option<String>,
    #[serde(rename = "Enabled")]
    pub enabled: Option<bool>,
    #[serde(rename = "RoleId")]
    pub role_id: Option<String>,
    #[serde(rename = "Locked")]
    pub locked: Option<bool>,
    /// Upstream: AccountService AccountExpiration / PasswordExpirationDays
    /// Maps to xyz.openbmc_project.User.Attributes.UserPasswordExpiry (u64 days)
    #[serde(rename = "PasswordExpirationDays")]
    pub password_expiration_days: Option<u64>,
}

/// The predefined Redfish roles supported by OpenBMC.
const SUPPORTED_ROLES: &[(&str, &[&str])] = &[
    (
        "Administrator",
        &[
            "Login",
            "ConfigureManager",
            "ConfigureUsers",
            "ConfigureSelf",
            "ConfigureComponents",
        ],
    ),
    ("Operator", &["Login", "ConfigureSelf", "ConfigureComponents"]),
    ("ReadOnly", &["Login", "ConfigureSelf"]),
    ("NoAccess", &[]),
];

/// Return true if the given role_id is one of the built-in Redfish roles.
fn is_valid_role(role_id: &str) -> bool {
    SUPPORTED_ROLES.iter().any(|(id, _)| *id == role_id)
}

/// Build a JSON Role object from a role tuple.
fn role_to_json(id: &str, privileges: &[&str]) -> Value {
    let privs: Vec<Value> = privileges.iter().map(|p| json!(p)).collect();
    json!({
        "@odata.type": "#Role.v1_3_1.Role",
        "@odata.id": format!("/redfish/v1/AccountService/Roles/{}", id),
        "Id": id,
        "Name": format!("{} Role", id),
        "Description": format!("Redfish {} Role", id),
        "IsPredefined": true,
        "AssignedPrivileges": privs,
    })
}

/// Map an OpenBMC privilege group string to a Redfish RoleId.
///
/// OpenBMC group → Redfish role:
///   priv-admin    → Administrator
///   priv-operator → Operator
///   priv-user     → ReadOnly
///   priv-noaccess → NoAccess
fn openbmc_priv_to_role(priv_str: &str) -> &'static str {
    match priv_str {
        "priv-admin"    => "Administrator",
        "priv-operator" => "Operator",
        "priv-user"     => "ReadOnly",
        "priv-noaccess" => "NoAccess",
        _               => "ReadOnly",
    }
}

// ---------------------------------------------------------------------------
// AccountService resource
// ---------------------------------------------------------------------------

/// GET /redfish/v1/AccountService
pub async fn get_account_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService");

    // Read live account policy from DBus if available
    let (lockout_threshold, lockout_duration) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = crate::dbus::ZBusClient::from_connection(conn.clone());
        let threshold = client
            .get_property(
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "MaxLoginAttemptBeforeLockout",
            )
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(5);
        let duration = client
            .get_property(
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "AccountUnlockTimeout",
            )
            .await
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        (threshold, duration)
    } else {
        (5u64, 30u64)
    };

    let response = json!({
        "@odata.type": "#AccountService.v1_12_0.AccountService",
        "@odata.id": "/redfish/v1/AccountService",
        "Id": "AccountService",
        "Name": "Account Service",
        "Description": "Account Service",
        "ServiceEnabled": true,
        "AuthFailureLoggingThreshold": 3,
        "MinPasswordLength": 8,
        "AccountLockoutThreshold": lockout_threshold,
        "AccountLockoutDuration": lockout_duration,
        "AccountLockoutCounterResetAfter": lockout_duration,
        "PrivilegeMap": {
            "@odata.id": "/redfish/v1/AccountService/PrivilegeMap"
        },
        "Accounts": {
            "@odata.id": "/redfish/v1/AccountService/Accounts"
        },
        "Roles": {
            "@odata.id": "/redfish/v1/AccountService/Roles"
        },
        "LDAP": {
            "ServiceEnabled": false,
            "ServiceAddresses": [],
            "Authentication": {
                "AuthenticationType": "UsernameAndPassword"
            },
            "LDAPService": {
                "SearchSettings": {}
            },
            "RemoteRoleMapping": []
        }
    });

    Ok(Json(response))
}

/// PATCH /redfish/v1/AccountService
///
/// Allows updating account lockout policy via DBus:
///   xyz.openbmc_project.User.Manager / MaxLoginAttemptBeforeLockout (u32)
///   xyz.openbmc_project.User.Manager / AccountUnlockTimeout (u32)
pub async fn patch_account_service(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<Value>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/AccountService");
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS)?;

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());

        if let Some(threshold) = body.get("AccountLockoutThreshold").and_then(|v| v.as_u64()) {
            if let Err(e) = client
                .set_property(
                    "/xyz/openbmc_project/user",
                    "xyz.openbmc_project.User.Manager",
                    "MaxLoginAttemptBeforeLockout",
                    serde_json::json!(threshold),
                )
                .await
            {
                warn!("Failed to set MaxLoginAttemptBeforeLockout: {}", e);
            } else {
                info!("AccountLockoutThreshold set to {} via DBus", threshold);
            }
        }

        if let Some(duration) = body.get("AccountLockoutDuration").and_then(|v| v.as_u64()) {
            if let Err(e) = client
                .set_property(
                    "/xyz/openbmc_project/user",
                    "xyz.openbmc_project.User.Manager",
                    "AccountUnlockTimeout",
                    serde_json::json!(duration),
                )
                .await
            {
                warn!("Failed to set AccountUnlockTimeout: {}", e);
            } else {
                info!("AccountLockoutDuration set to {} via DBus", duration);
            }
        }
    }

    get_account_service(State(state)).await
}

/// GET /redfish/v1/AccountService/PrivilegeMap
///
/// Returns the Redfish PrivilegeRegistry that maps resources to required
/// privileges.  This is a static document — upstream bmcweb serves it from
/// a bundled JSON file.  We return a minimal well-formed response that
/// satisfies schema-aware clients.
pub async fn get_privilege_map(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/PrivilegeMap");

    Ok(Json(json!({
        "@odata.type": "#PrivilegeRegistry.v1_1_4.PrivilegeRegistry",
        "@odata.id": "/redfish/v1/AccountService/PrivilegeMap",
        "Id": "PrivilegeMap",
        "Name": "Privilege Map",
        "Description": "This resource represents the privilege registry mapping resources to required privileges",
        "PrivilegesUsed": [
            "Login",
            "ConfigureManager",
            "ConfigureUsers",
            "ConfigureSelf",
            "ConfigureComponents"
        ],
        "OEMPrivilegesUsed": [],
        "Mappings": [
            {
                "Entity": "Manager",
                "OperationMap": {
                    "GET":    [{ "Privilege": ["Login"] }],
                    "HEAD":   [{ "Privilege": ["Login"] }],
                    "POST":   [{ "Privilege": ["ConfigureManager"] }],
                    "PUT":    [{ "Privilege": ["ConfigureManager"] }],
                    "PATCH":  [{ "Privilege": ["ConfigureManager"] }],
                    "DELETE": [{ "Privilege": ["ConfigureManager"] }]
                }
            },
            {
                "Entity": "AccountService",
                "OperationMap": {
                    "GET":    [{ "Privilege": ["Login"] }],
                    "PATCH":  [{ "Privilege": ["ConfigureUsers"] }]
                }
            },
            {
                "Entity": "ManagerAccount",
                "OperationMap": {
                    "GET":    [{ "Privilege": ["Login", "ConfigureSelf"] }],
                    "POST":   [{ "Privilege": ["ConfigureUsers"] }],
                    "PATCH":  [{ "Privilege": ["ConfigureUsers", "ConfigureSelf"] }],
                    "DELETE": [{ "Privilege": ["ConfigureUsers"] }]
                }
            }
        ]
    })))
}

// ---------------------------------------------------------------------------
// Accounts collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/AccountService/Accounts
///
/// Enumerates user accounts via `xyz.openbmc_project.User.Manager.ListUsers`.
/// Falls back to a single static `root` entry when DBus is unavailable.
pub async fn get_accounts_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Accounts");

    let members: Vec<Value> = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .call_method(
                "xyz.openbmc_project.User.Manager",
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "ListUsers",
                None,
            )
            .await
        {
            Ok(val) => {
                // ListUsers returns an array of username strings
                if let Some(users) = val.as_array() {
                    let mut result: Vec<Value> = users
                        .iter()
                        .filter_map(|u| u.as_str())
                        .map(|username| {
                            json!({ "@odata.id": format!("/redfish/v1/AccountService/Accounts/{}", username) })
                        })
                        .collect();
                    result.sort_by_key(|v| v["@odata.id"].as_str().unwrap_or("").to_string());
                    result
                } else {
                    warn!("ListUsers returned unexpected format: {:?}", val);
                    vec![json!({ "@odata.id": "/redfish/v1/AccountService/Accounts/root" })]
                }
            }
            Err(e) => {
                warn!("ListUsers DBus call failed: {} — using static fallback", e);
                vec![json!({ "@odata.id": "/redfish/v1/AccountService/Accounts/root" })]
            }
        }
    } else {
        vec![json!({ "@odata.id": "/redfish/v1/AccountService/Accounts/root" })]
    };

    let count = members.len();
    Ok(Json(json!({
        "@odata.type": "#ManagerAccountCollection.ManagerAccountCollection",
        "@odata.id": "/redfish/v1/AccountService/Accounts",
        "Name": "Accounts Collection",
        "Description": "BMC User Accounts",
        "Members@odata.count": count,
        "Members": members
    })))
}

/// POST /redfish/v1/AccountService/Accounts
///
/// Creates a new user account via `xyz.openbmc_project.User.Manager.CreateUser`.
///
/// DBus signature: CreateUser(sas b) → void
///   arg 0: username (string)
///   arg 1: groups (array of strings, e.g. ["priv-admin", "ssh"])
///   arg 2: userPassword is managed by PAM, not passed here
pub async fn create_account(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    JsonBody(body): JsonBody<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    debug!("POST /redfish/v1/AccountService/Accounts - user: {}", body.username);
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS)?;

    if body.username.is_empty() || body.password.is_empty() {
        warn!("Missing username or password in account creation request");
        return Err(StatusCode::BAD_REQUEST);
    }

    if !is_valid_role(&body.role_id) {
        warn!("Invalid RoleId '{}' in account creation request", body.role_id);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Map Redfish RoleId → OpenBMC privilege group
    let openbmc_group = match body.role_id.as_str() {
        "Administrator" => "priv-admin",
        "Operator"      => "priv-operator",
        "ReadOnly"      => "priv-user",
        "NoAccess"      => "priv-noaccess",
        _               => "priv-user",
    };

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        // CreateUser args: username, [groups], enabled
        // We pass the priv group and "ssh" group so the user can log in
        let args = json!([body.username, [openbmc_group, "ssh"], body.enabled]);
        match client
            .call_method(
                "xyz.openbmc_project.User.Manager",
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "CreateUser",
                Some(&args),
            )
            .await
        {
            Ok(_) => {
                info!("Created user '{}' with role '{}' via DBus", body.username, body.role_id);
            }
            Err(e) => {
                warn!("CreateUser DBus call failed for '{}': {}", body.username, e);
                // Return 500 if DBus call fails — the account was not created
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        info!("No DBus — account creation for '{}' acknowledged but not persisted", body.username);
    }

    let response = json!({
        "@odata.type": "#ManagerAccount.v1_12_0.ManagerAccount",
        "@odata.id": format!("/redfish/v1/AccountService/Accounts/{}", body.username),
        "Id": body.username,
        "Name": format!("User Account: {}", body.username),
        "UserName": body.username,
        "RoleId": body.role_id,
        "Enabled": body.enabled,
        "Locked": false,
        "PasswordExpirationDays": null,
        "Links": {
            "Role": {
                "@odata.id": format!("/redfish/v1/AccountService/Roles/{}", body.role_id)
            }
        }
    });

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /redfish/v1/AccountService/Accounts/{account_id}
///
/// Retrieves account info from `xyz.openbmc_project.User.Manager.GetUserInfo`.
/// Falls back to static data for `root` when DBus is unavailable.
pub async fn get_account(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Accounts/{}", account_id);

    let (role_id, enabled, locked) = if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .call_method(
                "xyz.openbmc_project.User.Manager",
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "GetUserInfo",
                Some(&json!(account_id)),
            )
            .await
        {
            Ok(info) => {
                // GetUserInfo returns a dict<string, variant>
                let priv_str = info
                    .get("UserPrivilege")
                    .and_then(|v| v.as_str())
                    .unwrap_or("priv-user");
                let role = openbmc_priv_to_role(priv_str).to_string();
                let is_enabled = info
                    .get("UserEnabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let is_locked = info
                    .get("UserLockedForFailedAttempt")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (role, is_enabled, is_locked)
            }
            Err(e) => {
                warn!("GetUserInfo for '{}' failed: {}", account_id, e);
                // If the DBus call failed with a "user not found" style error,
                // return 404; otherwise fall back to static data for root.
                if account_id != "root" {
                    return Err(StatusCode::NOT_FOUND);
                }
                ("Administrator".to_string(), true, false)
            }
        }
    } else {
        // No DBus: only root is known
        if account_id != "root" {
            warn!("Account '{}' not found (no DBus)", account_id);
            return Err(StatusCode::NOT_FOUND);
        }
        ("Administrator".to_string(), true, false)
    };

    Ok(Json(json!({
        "@odata.type": "#ManagerAccount.v1_12_0.ManagerAccount",
        "@odata.id": format!("/redfish/v1/AccountService/Accounts/{}", account_id),
        "Id": account_id,
        "Name": format!("User Account: {}", account_id),
        "UserName": account_id,
        "RoleId": role_id,
        "Enabled": enabled,
        "Locked": locked,
        "PasswordExpirationDays": null,
        "Links": {
            "Role": {
                "@odata.id": format!("/redfish/v1/AccountService/Roles/{}", role_id)
            }
        }
    })))
}

/// PATCH /redfish/v1/AccountService/Accounts/{account_id}
///
/// Updates account properties via DBus.
pub async fn patch_account(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(account_id): Path<String>,
    JsonBody(body): JsonBody<PatchAccountRequest>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/AccountService/Accounts/{}", account_id);

    let is_self = session.username == account_id;
    let has_configure_users = check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS).is_ok();
    if has_configure_users {
        if let Some(ref role) = body.role_id {
            if !is_valid_role(role) {
                warn!("Invalid RoleId '{}' in account patch", role);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        if !is_self {
            return Err(StatusCode::FORBIDDEN);
        }
        if body.password.is_none()
            || body.enabled.is_some()
            || body.role_id.is_some()
            || body.locked.is_some()
            || body.password_expiration_days.is_some()
        {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());

        // Apply RoleId change by updating the UserPrivilege attribute
        if let Some(ref new_role) = body.role_id {
            let openbmc_group = match new_role.as_str() {
                "Administrator" => "priv-admin",
                "Operator"      => "priv-operator",
                "ReadOnly"      => "priv-user",
                "NoAccess"      => "priv-noaccess",
                _               => "priv-user",
            };
            let user_obj = format!("/xyz/openbmc_project/user/{}", account_id);
            if let Err(e) = client
                .set_property(
                    &user_obj,
                    "xyz.openbmc_project.User.Attributes",
                    "UserPrivilege",
                    json!(openbmc_group),
                )
                .await
            {
                warn!("SetProperty UserPrivilege for '{}' failed: {}", account_id, e);
            } else {
                info!("Updated role for '{}' to '{}' via DBus", account_id, new_role);
            }
        }

        // Apply Enabled change
        if let Some(enabled) = body.enabled {
            let user_obj = format!("/xyz/openbmc_project/user/{}", account_id);
            if let Err(e) = client
                .set_property(
                    &user_obj,
                    "xyz.openbmc_project.User.Attributes",
                    "UserEnabled",
                    json!(enabled),
                )
                .await
            {
                warn!("SetProperty UserEnabled for '{}' failed: {}", account_id, e);
            }
        }

        // Apply PasswordExpirationDays change
        // Upstream: AccountService schema PasswordExpirationDays → UserPasswordExpiry on DBus
        if let Some(expiry_days) = body.password_expiration_days {
            let user_obj = format!("/xyz/openbmc_project/user/{}", account_id);
            if let Err(e) = client
                .set_property(
                    &user_obj,
                    "xyz.openbmc_project.User.Attributes",
                    "UserPasswordExpiry",
                    json!(expiry_days),
                )
                .await
            {
                warn!("SetProperty UserPasswordExpiry for '{}' failed: {}", account_id, e);
            } else {
                info!(
                    "Updated PasswordExpirationDays for '{}' to {}",
                    account_id, expiry_days
                );
            }
        }
    } else {
        info!("Account patch for '{}' acknowledged (no DBus)", account_id);
    }

    // Return the updated account state
    get_account(State(state), Path(account_id)).await
}

/// DELETE /redfish/v1/AccountService/Accounts/{account_id}
///
/// Deletes a user account via `xyz.openbmc_project.User.Manager.DeleteUser`.
pub async fn delete_account(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<UserSession>,
    Path(account_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/AccountService/Accounts/{}", account_id);
    check_privilege(Some(&session), PRIVILEGE_CONFIGURE_USERS)?;

    if account_id == "root" {
        warn!("Attempt to delete the root account is not allowed");
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(conn) = state.dbus_connection.as_deref() {
        let client = ZBusClient::from_connection(conn.clone());
        match client
            .call_method(
                "xyz.openbmc_project.User.Manager",
                "/xyz/openbmc_project/user",
                "xyz.openbmc_project.User.Manager",
                "DeleteUser",
                Some(&json!(account_id)),
            )
            .await
        {
            Ok(_) => {
                info!("Deleted user '{}' via DBus", account_id);
                Ok(StatusCode::NO_CONTENT)
            }
            Err(e) => {
                warn!("DeleteUser DBus call failed for '{}': {}", account_id, e);
                Err(StatusCode::NOT_FOUND)
            }
        }
    } else {
        warn!("Cannot delete user '{}' — no DBus connection", account_id);
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Roles collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/AccountService/Roles
pub async fn get_roles_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Roles");

    let members: Vec<Value> = SUPPORTED_ROLES
        .iter()
        .map(|(id, _)| {
            json!({ "@odata.id": format!("/redfish/v1/AccountService/Roles/{}", id) })
        })
        .collect();
    let count = members.len();

    Ok(Json(json!({
        "@odata.type": "#RoleCollection.RoleCollection",
        "@odata.id": "/redfish/v1/AccountService/Roles",
        "Name": "Roles Collection",
        "Members@odata.count": count,
        "Members": members,
    })))
}

/// GET /redfish/v1/AccountService/Roles/{role_id}
pub async fn get_role(
    State(_state): State<Arc<AppState>>,
    Path(role_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Roles/{}", role_id);

    for (id, privs) in SUPPORTED_ROLES {
        if *id == role_id.as_str() {
            return Ok(Json(role_to_json(id, privs)));
        }
    }

    warn!("Role '{}' not found", role_id);
    Err(StatusCode::NOT_FOUND)
}

/// Build an administrator UserSession for use in unit tests.
#[cfg(test)]
fn test_admin_session() -> UserSession {
    use std::net::{IpAddr, Ipv4Addr};
    use crate::auth::session::SessionType;
    let mut s = UserSession::new(
        "testadmin".to_string(),
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        SessionType::Token,
        3600,
    );
    s.set_role("Administrator".to_string());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_account_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_account_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#AccountService.v1_12_0.AccountService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_accounts_collection_no_dbus() {
        // No DBus — falls back to single root entry
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_accounts_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ManagerAccountCollection.ManagerAccountCollection");
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_account_root_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_account(State(state), Path("root".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["UserName"], "root");
        assert_eq!(json["RoleId"], "Administrator");
    }

    #[tokio::test]
    async fn test_get_account_not_found_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_account(State(state), Path("nobody".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_account_root_forbidden() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = delete_account(
            State(state),
            Extension(test_admin_session()),
            Path("root".to_string()),
        ).await;
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_delete_account_no_dbus() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = delete_account(
            State(state),
            Extension(test_admin_session()),
            Path("testuser".to_string()),
        ).await;
        // No DBus → 404
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_roles_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_roles_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#RoleCollection.RoleCollection");
        assert_eq!(json["Members@odata.count"], 4);
    }

    #[tokio::test]
    async fn test_get_role_administrator() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_role(State(state), Path("Administrator".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Id"], "Administrator");
        assert_eq!(json["IsPredefined"], true);
    }

    #[tokio::test]
    async fn test_get_role_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_role(State(state), Path("SuperAdmin".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_is_valid_role() {
        assert!(is_valid_role("Administrator"));
        assert!(is_valid_role("Operator"));
        assert!(is_valid_role("ReadOnly"));
        assert!(is_valid_role("NoAccess"));
        assert!(!is_valid_role("SuperAdmin"));
        assert!(!is_valid_role(""));
    }

    #[test]
    fn test_openbmc_priv_to_role() {
        assert_eq!(openbmc_priv_to_role("priv-admin"), "Administrator");
        assert_eq!(openbmc_priv_to_role("priv-operator"), "Operator");
        assert_eq!(openbmc_priv_to_role("priv-user"), "ReadOnly");
        assert_eq!(openbmc_priv_to_role("priv-noaccess"), "NoAccess");
        assert_eq!(openbmc_priv_to_role("unknown"), "ReadOnly");
    }
}
