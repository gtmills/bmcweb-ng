//! Redfish AccountService endpoints
//!
//! Implements the Redfish AccountService resource family:
//! - GET  /redfish/v1/AccountService
//! - GET  /redfish/v1/AccountService/Accounts
//! - POST /redfish/v1/AccountService/Accounts
//! - GET  /redfish/v1/AccountService/Accounts/{account_id}
//! - PATCH /redfish/v1/AccountService/Accounts/{account_id}
//! - DELETE /redfish/v1/AccountService/Accounts/{account_id}
//! - GET  /redfish/v1/AccountService/Roles
//! - GET  /redfish/v1/AccountService/Roles/{role_id}
//!
//! Reference: DMTF DSP0266, AccountService schema v1.12.0, ManagerAccount schema v1.12.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as JsonBody,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

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
}

/// The predefined Redfish roles supported by OpenBMC.
///
/// These correspond to the roles defined in the Redfish Roles privilege map.
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

// ---------------------------------------------------------------------------
// AccountService resource
// ---------------------------------------------------------------------------

/// GET /redfish/v1/AccountService
pub async fn get_account_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService");

    let _timeout = state.config.auth.session_timeout_seconds;
    let response = json!({
        "@odata.type": "#AccountService.v1_12_0.AccountService",
        "@odata.id": "/redfish/v1/AccountService",
        "Id": "AccountService",
        "Name": "Account Service",
        "Description": "Account Service",
        "ServiceEnabled": true,
        "AuthFailureLoggingThreshold": 3,
        "MinPasswordLength": 8,
        "AccountLockoutThreshold": 5,
        "AccountLockoutDuration": 30,
        "AccountLockoutCounterResetAfter": 30,
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

// ---------------------------------------------------------------------------
// Accounts collection
// ---------------------------------------------------------------------------

/// GET /redfish/v1/AccountService/Accounts
///
/// Returns the ManagerAccountCollection.  Account data is read from the
/// system PAM/passwd database on Linux; here we return a static placeholder
/// representing the `root` account until DBus-backed account enumeration
/// is implemented.
pub async fn get_accounts_collection(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Accounts");

    // TODO: Enumerate actual accounts from DBus
    // xyz.openbmc_project.User.Manager / ListUsers
    let response = json!({
        "@odata.type": "#ManagerAccountCollection.ManagerAccountCollection",
        "@odata.id": "/redfish/v1/AccountService/Accounts",
        "Name": "Accounts Collection",
        "Description": "BMC User Accounts",
        "Members@odata.count": 1,
        "Members": [
            { "@odata.id": "/redfish/v1/AccountService/Accounts/root" }
        ]
    });

    Ok(Json(response))
}

/// POST /redfish/v1/AccountService/Accounts
///
/// Creates a new user account.  Full implementation requires DBus calls to
/// `xyz.openbmc_project.User.Manager.CreateUser`.
pub async fn create_account(
    State(_state): State<Arc<AppState>>,
    JsonBody(body): JsonBody<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    debug!("POST /redfish/v1/AccountService/Accounts - user: {}", body.username);

    if body.username.is_empty() || body.password.is_empty() {
        warn!("Missing username or password in account creation request");
        return Err(StatusCode::BAD_REQUEST);
    }

    if !is_valid_role(&body.role_id) {
        warn!("Invalid RoleId '{}' in account creation request", body.role_id);
        return Err(StatusCode::BAD_REQUEST);
    }

    // TODO: Create account via DBus xyz.openbmc_project.User.Manager.CreateUser
    info!("Account creation requested for '{}' with role '{}' (DBus not yet implemented)",
          body.username, body.role_id);

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
pub async fn get_account(
    State(_state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/AccountService/Accounts/{}", account_id);

    // TODO: Retrieve account from DBus
    // Only "root" placeholder is supported until DBus integration is complete.
    if account_id != "root" {
        warn!("Account '{}' not found", account_id);
        return Err(StatusCode::NOT_FOUND);
    }

    let response = json!({
        "@odata.type": "#ManagerAccount.v1_12_0.ManagerAccount",
        "@odata.id": "/redfish/v1/AccountService/Accounts/root",
        "Id": "root",
        "Name": "User Account: root",
        "UserName": "root",
        "RoleId": "Administrator",
        "Enabled": true,
        "Locked": false,
        "PasswordExpirationDays": null,
        "Links": {
            "Role": {
                "@odata.id": "/redfish/v1/AccountService/Roles/Administrator"
            }
        }
    });

    Ok(Json(response))
}

/// PATCH /redfish/v1/AccountService/Accounts/{account_id}
///
/// Updates account properties.  Full implementation requires DBus calls.
pub async fn patch_account(
    State(_state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
    JsonBody(body): JsonBody<PatchAccountRequest>,
) -> Result<Json<Value>, StatusCode> {
    debug!("PATCH /redfish/v1/AccountService/Accounts/{}", account_id);

    if account_id != "root" {
        warn!("Account '{}' not found for PATCH", account_id);
        return Err(StatusCode::NOT_FOUND);
    }

    if let Some(ref role) = body.role_id {
        if !is_valid_role(role) {
            warn!("Invalid RoleId '{}' in account patch", role);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // TODO: Apply changes via DBus xyz.openbmc_project.User.Manager
    info!("Account patch requested for '{}' (DBus not yet implemented)", account_id);

    // Return the (potentially updated) account — still static until DBus
    get_account(State(_state), Path(account_id)).await
}

/// DELETE /redfish/v1/AccountService/Accounts/{account_id}
///
/// Deletes a user account via DBus.
pub async fn delete_account(
    State(_state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/AccountService/Accounts/{}", account_id);

    if account_id == "root" {
        warn!("Attempt to delete the root account is not allowed");
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: Delete via DBus xyz.openbmc_project.User.Manager.DeleteUser
    warn!("Account '{}' not found (DBus not yet implemented)", account_id);
    Err(StatusCode::NOT_FOUND)
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

    let response = json!({
        "@odata.type": "#RoleCollection.RoleCollection",
        "@odata.id": "/redfish/v1/AccountService/Roles",
        "Name": "Roles Collection",
        "Members@odata.count": count,
        "Members": members,
    });

    Ok(Json(response))
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
    async fn test_get_accounts_collection() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_accounts_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#ManagerAccountCollection.ManagerAccountCollection");
        assert_eq!(json["Members@odata.count"], 1);
    }

    #[tokio::test]
    async fn test_get_account_root() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_account(State(state), Path("root".to_string())).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["UserName"], "root");
        assert_eq!(json["RoleId"], "Administrator");
    }

    #[tokio::test]
    async fn test_get_account_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_account(State(state), Path("nobody".to_string())).await;
        assert!(result.is_err());
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
}
