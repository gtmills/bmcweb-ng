//! HTTP Basic Authentication
//!
//! Implements RFC 7617 HTTP Basic Authentication with PAM integration

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use pam::Authenticator;
use std::net::IpAddr;
use tracing::{debug, warn};

/// Credentials extracted from Basic Auth header
#[derive(Debug, Clone)]
pub struct BasicCredentials {
    pub username: String,
    pub password: String,
}

/// Parse Basic Authentication header
///
/// Expected format: "Basic <base64(username:password)>"
pub fn parse_basic_auth_header(auth_header: &str) -> Result<BasicCredentials> {
    // Check for "Basic " prefix
    if !auth_header.starts_with("Basic ") {
        return Err(anyhow!("Invalid Basic Auth header format"));
    }

    // Extract base64 encoded credentials
    let encoded = auth_header.trim_start_matches("Basic ").trim();
    
    // Decode base64
    let decoded_bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| anyhow!("Failed to decode base64: {}", e))?;
    
    let decoded = String::from_utf8(decoded_bytes)
        .map_err(|e| anyhow!("Invalid UTF-8 in credentials: {}", e))?;

    // Split on first colon
    let parts: Vec<&str> = decoded.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid credentials format, expected username:password"));
    }

    Ok(BasicCredentials {
        username: parts[0].to_string(),
        password: parts[1].to_string(),
    })
}

/// Authenticate user with PAM
///
/// Uses the system's PAM configuration to verify credentials
pub fn authenticate_with_pam(username: &str, password: &str) -> Result<()> {
    debug!("Attempting PAM authentication for user: {}", username);

    // Create PAM authenticator
    let mut auth = Authenticator::with_password("bmcweb")
        .map_err(|e| anyhow!("Failed to create PAM authenticator: {}", e))?;

    // Set credentials
    auth.get_handler().set_credentials(username, password);

    // Authenticate
    match auth.authenticate() {
        Ok(_) => {
            debug!("PAM authentication successful for user: {}", username);
            Ok(())
        }
        Err(e) => {
            warn!("PAM authentication failed for user {}: {}", username, e);
            Err(anyhow!("Authentication failed"))
        }
    }
}

/// Perform Basic Authentication
///
/// Parses the Authorization header and authenticates the user via PAM
pub async fn perform_basic_auth(
    auth_header: &str,
    client_ip: IpAddr,
) -> Result<String> {
    debug!("Performing Basic authentication from IP: {}", client_ip);

    // Parse credentials
    let creds = parse_basic_auth_header(auth_header)?;

    // Authenticate with PAM
    authenticate_with_pam(&creds.username, &creds.password)?;

    debug!("Basic authentication successful for user: {}", creds.username);
    Ok(creds.username)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_auth_header() {
        // Test valid header
        let header = "Basic dXNlcjpwYXNz"; // user:pass
        let creds = parse_basic_auth_header(header).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_parse_basic_auth_header_with_colon_in_password() {
        // Test password with colon
        let header = "Basic dXNlcjpwYXNzOndvcmQ="; // user:pass:word
        let creds = parse_basic_auth_header(header).unwrap();
        assert_eq!(creds.username, "user");
        assert_eq!(creds.password, "pass:word");
    }

    #[test]
    fn test_parse_basic_auth_header_invalid_format() {
        let header = "Bearer token123";
        assert!(parse_basic_auth_header(header).is_err());
    }

    #[test]
    fn test_parse_basic_auth_header_invalid_base64() {
        let header = "Basic !!!invalid!!!";
        assert!(parse_basic_auth_header(header).is_err());
    }

    #[test]
    fn test_parse_basic_auth_header_missing_colon() {
        let header = "Basic dXNlcm5hbWU="; // username (no colon)
        assert!(parse_basic_auth_header(header).is_err());
    }
}
