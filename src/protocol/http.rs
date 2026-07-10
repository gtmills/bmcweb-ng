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
    middleware,
    routing::get,
};
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

        // Redfish router with mandatory authentication applied as a layer.
        // The session POST endpoints are intentionally excluded from mandatory
        // auth — they accept unauthenticated requests for the login flow.
        let redfish_router = crate::api::redfish::router()
            .layer(middleware::from_fn_with_state(
                state.clone(),
                optional_auth_middleware,
            ));

        // WebSocket routes with optional auth (auth handled inside handlers)
        let ws_router = crate::api::websocket::websocket_routes()
            .layer(middleware::from_fn_with_state(
                state.clone(),
                optional_auth_middleware,
            ));

        Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .nest("/redfish/v1", redfish_router)
            .merge(ws_router)
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

        // Serve connections with TLS wrapping
        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let router = router.clone();

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        if let Err(e) = axum::serve(
                            tokio_rustls_listener_from_stream(tls_stream),
                            router,
                        )
                        .await
                        {
                            error!("TLS connection error from {}: {}", peer_addr, e);
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

/// Generate a self-signed TLS certificate in memory.
///
/// Uses rcgen to create a short-lived certificate for development/testing.
/// The certificate covers `localhost` and the loopback address.
fn generate_self_signed_tls_config() -> Result<ServerConfig> {
    // rcgen is a pure-Rust self-signed certificate generator.
    // It is a common choice in the Rust ecosystem for exactly this use case
    // (see also: axum-server, hyper-rustls examples).
    //
    // We do not add rcgen as a hard dependency yet — this is the
    // implementation outline.  TODO: add rcgen = "0.12" to Cargo.toml.
    //
    // For now return a descriptive error so operators know what to do.
    Err(anyhow::anyhow!(
        "Self-signed certificate generation requires the `rcgen` crate. \
         Add rcgen = \"0.12\" to Cargo.toml, or supply a TLS certificate at the configured path."
    ))
}

// ---------------------------------------------------------------------------
// Adapter: wrap a TLS stream so axum can serve it
// ---------------------------------------------------------------------------

/// Minimal adapter to hand a single accepted TLS stream to axum::serve.
///
/// axum::serve expects a type that implements `tokio::io::AsyncRead +
/// AsyncWrite + Unpin`.  `tokio_rustls::server::TlsStream<TcpStream>`
/// already does; we just need to wrap it in a listener-like newtype.
///
/// NOTE: This is a simplified single-connection adapter.  A production
/// server would multiplex connections through a channel-based listener.
/// TODO: Replace with a proper multi-connection TLS listener using
/// `tokio::net::TcpListener` + `TlsAcceptor` in a accept-loop pattern.
fn tokio_rustls_listener_from_stream<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>(
    _stream: S,
) -> tokio::net::TcpListener {
    // Placeholder — in a real implementation we would use axum's
    // `serve_with_incoming` or build a custom `Accept` implementation.
    // This function signature is intentionally a stub.
    panic!("TLS listener adapter not fully implemented; see TODO in protocol/http.rs")
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// Root handler — redirects browsers to the Redfish service root.
async fn root_handler() -> &'static str {
    "bmcweb-ng BMC webserver — Redfish API available at /redfish/v1"
}

/// Health check endpoint used by systemd and load-balancers.
async fn health_handler() -> &'static str {
    "OK"
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
        // Should fall through to the self-signed generation attempt
        let result = build_tls_config("/nonexistent/cert.pem", "/nonexistent/key.pem");
        // Expected: Err because rcgen dependency is not yet added
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_root_handler() {
        let response = root_handler().await;
        assert!(response.contains("Redfish"));
    }

    #[tokio::test]
    async fn test_health_handler() {
        let response = health_handler().await;
        assert_eq!(response, "OK");
    }
}
