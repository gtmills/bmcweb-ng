//! bmcweb-ng - Next-generation BMC webserver for OpenBMC
//!
//! This is the main entry point for the bmcweb-ng daemon.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

    // TODO: Load configuration
    // TODO: Initialize DBus connection
    // TODO: Start HTTP/HTTPS server
    // TODO: Register signal handlers
    // TODO: Start metrics server

    info!("bmcweb-ng initialization complete");
    info!("Server ready to accept connections");

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal, gracefully shutting down...");

    Ok(())
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
