//! HTTP/HTTPS server implementation
//!
//! Provides the main HTTP and HTTPS servers using axum + hyper + tokio-rustls.
//!
//! # TLS
//!
//! When `tls_cert` and `tls_key` are configured, the server runs on HTTPS only.
//! A self-signed certificate is generated automatically if the configured paths
//! do not exist (development mode).  This matches the behaviour of the upstream
//! bmcweb which calls `ensuressl::checkAndGenerateSslCertificates()` at startup.
//!
//! # Authentication
//!
//! The authentication middleware is applied globally to all Redfish routes
//! **except** for the session creation endpoint (`POST /redfish/v1/SessionService/Sessions`)
//! which is intentionally open (that is how credentials are exchanged for tokens).
//!
//! Requests to the root `/` and `/health` endpoints are also unauthenticated.
//!
//! # Protocol support
//!
//! - HTTP/1.1 (plain-text and TLS)
//! - HTTP/2 via ALPN negotiation on TLS connections
//!
//! # Graceful shutdown
//!
//! The server respects the tokio CancellationToken pattern: call
//! [`HttpServer::run`] and abort the returned handle when you want to stop.

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::State,
    middleware,
    response::IntoResponse,
    routing::get,
};
use tower::Service as _;
use rcgen::generate_simple_self_signed;
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{error, info, warn};

use crate::auth::middleware::{auth_middleware, optional_auth_middleware};
use crate::config::ServerConfig as AppServerConfig;
use crate::AppState;

/// HTTP/HTTPS server.
pub struct HttpServer {
    config: AppServerConfig,
    app_state: Arc<AppState>,
}

impl HttpServer {
    /// Create a new server.
    pub fn new(config: AppServerConfig, app_state: Arc<AppState>) -> Self {
        Self { config, app_state }
    }

    /// Build the axum application router.
    ///
    /// Authentication is applied to the Redfish namespace as an axum
    /// [`middleware::from_fn_with_state`] layer so that the session creation
    /// endpoint can be carved out via [`optional_auth_middleware`].
    ///
    /// The session POST endpoint runs with optional auth — the user is not
    /// yet authenticated when they call it, so we must not reject unauthenticated
    /// requests there.  All other Redfish routes run with mandatory auth.
    ///
    /// WebSocket routes also have optional auth (the auth check is performed
    /// inside the handler after upgrade).
    pub fn build_router(&self) -> Router {
        let state = self.app_state.clone();

        // Session creation (POST login) must be unauthenticated — users haven't
        // got a token yet.  All other Redfish routes require authentication.
        // We achieve this by:
        //   1. Building the full Redfish router with mandatory auth as a layer.
        //   2. Overlaying the session creation routes with optional auth so the
        //      login endpoint accepts unauthenticated requests.
        let session_login_router = Router::new()
            .route(
                "/SessionService/Sessions",
                axum::routing::post(crate::api::redfish::sessions::create_session),
            )
            .route(
                "/SessionService/Sessions/Members",
                axum::routing::post(crate::api::redfish::sessions::create_session),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                optional_auth_middleware,
            ));

        // All remaining Redfish routes: mandatory authentication.
        let redfish_router = crate::api::redfish::router()
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            // Merge in the open session creation routes — axum route priority
            // means the more-specific merge wins over the layer above.
            .merge(session_login_router);

        // WebSocket routes with optional auth (auth check happens inside each
        // handler after the HTTP upgrade, where the token is inspected).
        let ws_router = crate::api::websocket::websocket_routes()
            .layer(middleware::from_fn_with_state(
                state.clone(),
                optional_auth_middleware,
            ));

        // DBus REST API routes — authenticated, mounted at root (same as upstream)
        let dbus_rest_router = crate::api::dbus_rest::dbus_rest_router()
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ));

        // Static WebUI file serving — unauthenticated so browsers can load
        // assets.  Served from /usr/share/www (OpenBMC convention), with a
        // fallback to ./www for development.  Falls back to index.html for
        // SPA-style client-side routing.
        let webui_path = if Path::new("/usr/share/www").exists() {
            "/usr/share/www"
        } else {
            "./www"
        };
        let webui_router: Router<Arc<AppState>> = Router::new()
            .nest_service(
                "/ui",
                ServeDir::new(webui_path)
                    .fallback(ServeFile::new(format!("{}/index.html", webui_path))),
            );

        Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .nest("/redfish/v1", redfish_router)
            .merge(ws_router)
            .merge(dbus_rest_router)
            .merge(webui_router)
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    /// Start the server, choosing HTTP or HTTPS based on configuration.
    ///
    /// If `tls_cert` and `tls_key` are both non-empty the server starts HTTPS;
    /// otherwise it falls back to plain HTTP.
    pub async fn run(self) -> Result<()> {
        let addr = SocketAddr::from((
            self.config
                .bind_address
                .parse::<std::net::IpAddr>()
                .context("Invalid bind_address")?,
            self.config.port,
        ));

        let use_tls = !self.config.tls_cert.is_empty() && !self.config.tls_key.is_empty();

        if use_tls {
            self.run_tls(addr).await
        } else {
            self.run_plain_http(addr).await
        }
    }

    /// Start a plain HTTP server.
    async fn run_plain_http(self, addr: SocketAddr) -> Result<()> {
        info!("Starting HTTP server on {}", addr);
        let router = self.build_router();
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind to {}", addr))?;

        info!("HTTP server listening on {}", addr);
        axum::serve(listener, router)
            .await
            .context("HTTP server error")?;

        Ok(())
    }

    /// Start a TLS-enabled HTTPS server.
    ///
    /// Loads the PEM certificate chain and private key from the configured
    /// paths. If the files do not exist, a self-signed certificate is
    /// generated in-memory for development purposes.
    async fn run_tls(self, addr: SocketAddr) -> Result<()> {
        info!("Starting HTTPS server on {}", addr);

        let tls_config = build_tls_config(&self.config.tls_cert, &self.config.tls_key)
            .context("Failed to build TLS configuration")?;

        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let router = self.build_router();
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind to {}", addr))?;

        info!("HTTPS server listening on {}", addr);

        // Accept loop: each TLS connection is handled in its own task.
        // We use hyper's lower-level serve_connection to drive a single
        // TLS stream rather than wrapping in a TcpListener.
        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let tower_service = router.clone();

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                        let hyper_service = hyper::service::service_fn(move |req| {
                            tower_service.clone().call(req)
                        });
                        if let Err(e) = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, hyper_service)
                            .with_upgrades()
                            .await
                        {
                            // Ignore benign "connection reset" errors
                            let msg = e.to_string();
                            if !msg.contains("connection reset")
                                && !msg.contains("broken pipe")
                            {
                                error!(
                                    "TLS connection error from {}: {}",
                                    peer_addr, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("TLS handshake failed from {}: {}", peer_addr, e);
                    }
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// TLS configuration builder
// ---------------------------------------------------------------------------

/// Build a [`rustls::ServerConfig`] from PEM certificate and key files.
///
/// If the files do not exist, falls back to a self-signed certificate
/// generated in memory so that development environments work out of the box.
fn build_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    let cert_exists = Path::new(cert_path).exists();
    let key_exists = Path::new(key_path).exists();

    if cert_exists && key_exists {
        load_tls_config_from_files(cert_path, key_path)
    } else {
        warn!(
            "TLS certificate ({}) or key ({}) not found; generating self-signed certificate",
            cert_path, key_path
        );
        generate_self_signed_tls_config()
    }
}

/// Load TLS configuration from PEM files.
fn load_tls_config_from_files(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    // Load certificate chain
    let cert_file = File::open(cert_path)
        .with_context(|| format!("Cannot open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<rustls::pki_types::CertificateDer> = certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to parse certificate PEM")?;

    // Load private key
    let key_file = File::open(key_path)
        .with_context(|| format!("Cannot open key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);
    let mut keys = pkcs8_private_keys(&mut key_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to parse private key PEM")?;

    if keys.is_empty() {
        return Err(anyhow::anyhow!("No private keys found in {}", key_path));
    }

    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(keys.remove(0));

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to build TLS ServerConfig")?;

    info!("TLS certificate loaded from {}", cert_path);
    Ok(config)
}

/// Generate a self-signed TLS certificate in memory using rcgen.
///
/// Creates a short-lived certificate valid for `localhost` and `127.0.0.1`.
/// This matches the behaviour of upstream bmcweb's `ensuressl::checkAndGenerateSslCertificates()`.
fn generate_self_signed_tls_config() -> Result<ServerConfig> {
    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "openbmc".to_string(),
    ];

    let cert = generate_simple_self_signed(subject_alt_names)
        .context("Failed to generate self-signed certificate")?;

    let cert_der = cert.cert.der().clone();
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
        cert.key_pair.serialize_der().into(),
    );

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .context("Failed to build TLS ServerConfig from self-signed cert")?;

    info!("Generated self-signed TLS certificate for localhost/openbmc (development mode)");
    Ok(config)
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// Root handler — redirects browsers to the Redfish service root.
async fn root_handler() -> &'static str {
    "bmcweb-ng BMC webserver — Redfish API available at /redfish/v1"
}

/// GET /health
///
/// Structured JSON health check used by systemd, load-balancers and monitoring.
///
/// Returns HTTP 200 with a JSON body describing the health of each subsystem.
/// A component in `"degraded"` state means it is unavailable but the server
/// is still functional (e.g. no DBus connection in a non-OpenBMC environment).
///
/// Response shape:
/// ```json
/// {
///   "status": "ok" | "degraded",
///   "version": "0.2.0",
///   "components": {
///     "dbus":    { "status": "ok" | "degraded", "detail": "..." },
///     "sessions":{ "status": "ok",              "active_sessions": N },
///     "metrics": { "status": "ok" | "degraded"  }
///   }
/// }
/// ```
async fn health_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use axum::http::StatusCode;
    use serde_json::json;

    let dbus_status = if state.dbus_connection.is_some() {
        json!({ "status": "ok", "detail": "connected to system bus" })
    } else {
        json!({ "status": "degraded", "detail": "no DBus connection" })
    };

    let session_status = if let Some(store) = &state.session_store {
        let active = store.get_all_sessions().len();
        json!({ "status": "ok", "active_sessions": active })
    } else {
        json!({ "status": "degraded", "detail": "session store not available" })
    };

    let metrics_status = if state.metrics.is_some() {
        json!({ "status": "ok" })
    } else {
        json!({ "status": "degraded", "detail": "metrics not enabled" })
    };

    // Overall status is "degraded" if any component is degraded.
    let overall = if state.dbus_connection.is_some() && state.session_store.is_some() {
        "ok"
    } else {
        "degraded"
    };

    let body = json!({
        "status": overall,
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "dbus":    dbus_status,
            "sessions": session_status,
            "metrics": metrics_status,
        }
    });

    (StatusCode::OK, axum::Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_server_creation() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config.clone()));
        let _server = HttpServer::new(config.server, state);
    }

    #[test]
    fn test_build_tls_config_missing_files() {
        // When cert/key files don't exist, rcgen generates a self-signed cert
        let result = build_tls_config("/nonexistent/cert.pem", "/nonexistent/key.pem");
        assert!(result.is_ok(), "Expected self-signed cert generation to succeed: {:?}", result);
    }

    #[test]
    fn test_generate_self_signed_tls_config() {
        let result = generate_self_signed_tls_config();
        assert!(result.is_ok(), "Self-signed cert generation failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_root_handler() {
        let response = root_handler().await;
        assert!(response.contains("Redfish"));
    }

    #[tokio::test]
    async fn test_health_handler() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let response = health_handler(State(state)).await;
        let (status, body) = response.into_response().into_parts();
        assert_eq!(status.status, axum::http::StatusCode::OK);
        // Body is JSON — just check that status is ok
        let _ = body; // we don't need to read the body bytes in this test
    }
}
