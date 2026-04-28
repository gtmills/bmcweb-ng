//! Authentication Middleware
//!
//! Axum middleware for authenticating HTTP requests

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::AppState;
use super::{basic, session::{SessionStore, SessionType}};

/// Extract client IP from request
fn get_client_ip(headers: &HeaderMap) -> Option<std::net::IpAddr> {
    // Try X-Forwarded-For first
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse() {
                    return Some(ip);
                }
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            if let Ok(ip) = ip_str.parse() {
                return Some(ip);
            }
        }
    }

    // Default to localhost if we can't determine
    Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
}

/// Authentication middleware
///
/// Checks for valid authentication via:
/// 1. Session token (X-Auth-Token header or Cookie)
/// 2. HTTP Basic Authentication
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let client_ip = get_client_ip(&headers).unwrap_or_else(|| {
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
    });

    debug!("Authentication check for IP: {}", client_ip);

    // Get session store from app state
    let session_store = state.session_store.as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Try X-Auth-Token header
    if let Some(token) = headers.get("x-auth-token") {
        if let Ok(token_str) = token.to_str() {
            debug!("Checking X-Auth-Token");
            if let Some(session) = session_store.get_session_by_token(token_str) {
                debug!("Valid session found for user: {}", session.username);
                request.extensions_mut().insert(session);
                return Ok(next.run(request).await);
            }
        }
    }

    // Try Cookie-based session
    if let Some(cookie) = headers.get("cookie") {
        if let Ok(cookie_str) = cookie.to_str() {
            debug!("Checking cookies");
            // Look for BMCWEB-SESSION cookie
            for cookie_part in cookie_str.split(';') {
                let cookie_part = cookie_part.trim();
                if let Some(token) = cookie_part.strip_prefix("BMCWEB-SESSION=") {
                    if let Some(session) = session_store.get_session_by_token(token) {
                        debug!("Valid session found via cookie for user: {}", session.username);
                        request.extensions_mut().insert(session);
                        return Ok(next.run(request).await);
                    }
                }
            }
        }
    }

    // Try Basic Authentication
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Basic ") {
                debug!("Attempting Basic authentication");
                match basic::perform_basic_auth(auth_str, client_ip).await {
                    Ok(username) => {
                        // Create or reuse session
                        if let Some(session) = session_store.create_session(
                            username,
                            client_ip,
                            SessionType::Basic,
                        ) {
                            debug!("Basic authentication successful, session created");
                            request.extensions_mut().insert(session);
                            return Ok(next.run(request).await);
                        } else {
                            warn!("Failed to create session after successful authentication");
                            return Err(StatusCode::INTERNAL_SERVER_ERROR);
                        }
                    }
                    Err(e) => {
                        warn!("Basic authentication failed: {}", e);
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                }
            }
        }
    }

    // No valid authentication found
    warn!("No valid authentication provided from IP: {}", client_ip);
    Err(StatusCode::UNAUTHORIZED)
}

/// Optional authentication middleware
///
/// Similar to auth_middleware but allows unauthenticated requests to pass through
pub async fn optional_auth_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    let client_ip = get_client_ip(&headers).unwrap_or_else(|| {
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
    });

    if let Some(session_store) = state.session_store.as_ref() {
        // Try to authenticate but don't fail if unsuccessful
        
        // Try X-Auth-Token
        if let Some(token) = headers.get("x-auth-token") {
            if let Ok(token_str) = token.to_str() {
                if let Some(session) = session_store.get_session_by_token(token_str) {
                    request.extensions_mut().insert(session);
                    return next.run(request).await;
                }
            }
        }

        // Try Cookie
        if let Some(cookie) = headers.get("cookie") {
            if let Ok(cookie_str) = cookie.to_str() {
                for cookie_part in cookie_str.split(';') {
                    let cookie_part = cookie_part.trim();
                    if let Some(token) = cookie_part.strip_prefix("BMCWEB-SESSION=") {
                        if let Some(session) = session_store.get_session_by_token(token) {
                            request.extensions_mut().insert(session);
                            return next.run(request).await;
                        }
                    }
                }
            }
        }

        // Try Basic Auth
        if let Some(auth_header) = headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str.starts_with("Basic ") {
                    if let Ok(username) = basic::perform_basic_auth(auth_str, client_ip).await {
                        if let Some(session) = session_store.create_session(
                            username,
                            client_ip,
                            SessionType::Basic,
                        ) {
                            request.extensions_mut().insert(session);
                            return next.run(request).await;
                        }
                    }
                }
            }
        }
    }

    // Continue without authentication
    next.run(request).await
}

/// Create a 401 Unauthorized response with WWW-Authenticate header
pub fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", "Basic realm=\"BMC Web Server\"")],
        "Unauthorized",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_get_client_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("192.168.1.100"));
        
        let ip = get_client_ip(&headers);
        assert!(ip.is_some());
        assert_eq!(ip.unwrap().to_string(), "192.168.1.100");
    }

    #[test]
    fn test_get_client_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("10.0.0.50"));
        
        let ip = get_client_ip(&headers);
        assert!(ip.is_some());
        assert_eq!(ip.unwrap().to_string(), "10.0.0.50");
    }

    #[test]
    fn test_get_client_ip_default() {
        let headers = HeaderMap::new();
        let ip = get_client_ip(&headers);
        assert!(ip.is_some());
        assert_eq!(ip.unwrap().to_string(), "127.0.0.1");
    }
}
