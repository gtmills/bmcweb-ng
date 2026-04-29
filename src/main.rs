//! bmcweb-ng - Next-generation BMC webserver for OpenBMC
//!
//! This is the main entry point for the bmcweb-ng daemon.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn, error, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use bmcweb_ng::{AppState, config::Config, protocol::http::HttpServer};

/// Command-line arguments for bmcweb-ng
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: Level,

    /// Enable JSON logging format
    #[arg(long)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Initialize logging
    init_logging(&args)?;

    info!("Starting bmcweb-ng v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration file: {}", args.config.display());

    // Load configuration
    let config = load_config(&args.config)?;
    info!("Configuration loaded successfully");

    // Initialize application state
    let mut app_state = AppState::new(config.clone());

    // Initialize DBus connection (optional, may fail on non-Linux systems)
    match init_dbus_connection().await {
        Ok(connection) => {
            info!("DBus connection established");
            app_state = app_state.with_dbus(connection);
        }
        Err(e) => {
            warn!("Failed to establish DBus connection: {}. Continuing without DBus support.", e);
        }
    }

    let app_state = Arc::new(app_state);

    info!("bmcweb-ng initialization complete");
    info!("Server configuration:");
    info!("  - Bind address: {}", config.server.bind_address);
    info!("  - Port: {}", config.server.port);
    info!("  - Max connections: {}", config.server.max_connections);

    // Start HTTP/HTTPS server
    let server = HttpServer::new(config.server.clone(), app_state.clone());
    
    // Spawn server task
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            error!("Server error: {}", e);
        }
    });

    info!("Server ready to accept connections");

    // Start metrics server if enabled
    let metrics_handle = if config.metrics.enabled && app_state.metrics.is_some() {
        info!("Starting metrics server on port {}", config.metrics.port);
        let metrics_state = app_state.clone();
        let metrics_port = config.metrics.port;
        
        Some(tokio::spawn(async move {
            if let Err(e) = start_metrics_server(metrics_state, metrics_port).await {
                error!("Metrics server error: {}", e);
            }
        }))
    } else {
        info!("Metrics server disabled");
        None
    };

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal, gracefully shutting down...");

    // Abort server tasks
    server_handle.abort();
    if let Some(handle) = metrics_handle {
        handle.abort();
    }

    info!("Shutdown complete");
    Ok(())
}

/// Load configuration from file or use defaults
fn load_config(path: &PathBuf) -> Result<Config> {
    if path.exists() {
        info!("Loading configuration from {}", path.display());
        Config::from_file(path)
    } else {
        warn!("Configuration file not found, using defaults");
        Ok(Config::default())
    }
}

/// Initialize DBus connection
async fn init_dbus_connection() -> Result<zbus::Connection> {
    // Try to connect to the system bus
    let connection = zbus::Connection::system().await?;
    Ok(connection)
}

/// Initialize the logging subsystem
fn init_logging(args: &Args) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(args.log_level.to_string()));

    if args.json_logs {
        // JSON structured logging for production
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        // Human-readable logging for development
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().pretty())
            .init();
    }

    Ok(())
}

/// Start the metrics server on a separate port
async fn start_metrics_server(state: Arc<AppState>, port: u16) -> Result<()> {
    use axum::{routing::get, Router};
    use bmcweb_ng::observability::metrics_handler;

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    info!("Metrics server listening on {}", addr);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parsing() {
        // Test that command-line argument parsing works
        let args = Args::parse_from(&["bmcwebd-ng", "--config", "test.toml"]);
        assert_eq!(args.config, PathBuf::from("test.toml"));
    }
}
