//! HTTP/HTTPS server implementation
//!
//! This module provides the HTTP and HTTPS server functionality using axum and hyper.

use anyhow::Result;
use axum::{
    Router,
    routing::get,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    trace::TraceLayer,
};
use tracing::{info, error};

use crate::config::ServerConfig;
use crate::AppState;

/// HTTP server that handles both HTTP and HTTPS connections
pub struct HttpServer {
    config: ServerConfig,
    app_state: Arc<AppState>,
}

impl HttpServer {
    /// Create a new HTTP server with the given configuration
    pub fn new(config: ServerConfig, app_state: Arc<AppState>) -> Self {
        Self { config, app_state }
    }

    /// Build the application router with all routes
    fn build_router(&self) -> Router {
        Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            // Redfish routes will be added here
            .nest("/redfish/v1", crate::api::redfish::router())
            // Add middleware
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http())
            .with_state(self.app_state.clone())
    }

    /// Start the HTTP server
    pub async fn run(self) -> Result<()> {
        let addr = SocketAddr::from((
            self.config.bind_address.parse::<std::net::IpAddr>()?,
            self.config.port,
        ));

        info!("Starting HTTP server on {}", addr);

        let router = self.build_router();
        let listener = TcpListener::bind(addr).await?;

        info!("HTTP server listening on {}", addr);

        axum::serve(listener, router)
            .await
            .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

        Ok(())
    }

    /// Start the HTTPS server with TLS
    pub async fn run_tls(self) -> Result<()> {
        // TODO: Implement TLS support using rustls
        // For now, fall back to HTTP
        info!("TLS not yet implemented, falling back to HTTP");
        self.run().await
    }
}

/// Root handler - redirects to Redfish service root
async fn root_handler() -> &'static str {
    "BMC Web Server - Redfish API available at /redfish/v1"
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            tls_cert: "".to_string(),
            tls_key: "".to_string(),
            max_connections: 100,
        };
        let state = Arc::new(AppState::new());
        let _server = HttpServer::new(config, state);
    }
}
