//! Configuration management for bmcweb-ng

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub logging: LoggingConfig,
    pub metrics: MetricsConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub tls_cert: String,
    pub tls_key: String,
    pub max_connections: usize,
    /// Enable mutual TLS client certificate authentication.
    /// When true, clients must present a certificate signed by `mtls_ca_cert`.
    #[serde(default)]
    pub mtls_enabled: bool,
    /// Path to the PEM CA certificate used to verify client certificates when
    /// `mtls_enabled` is true.  Ignored when `mtls_enabled` is false.
    #[serde(default = "default_mtls_ca_cert")]
    pub mtls_ca_cert: String,
}

fn default_mtls_ca_cert() -> String {
    "/etc/bmcweb/ca.pem".to_string()
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Session timeout in seconds (default 3600)
    pub session_timeout_seconds: u64,
    /// Maximum concurrent sessions (default 64)
    pub max_sessions: usize,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level string passed to tracing-subscriber (info/debug/warn/error/trace).
    /// Can be overridden by the RUST_LOG environment variable.
    pub level: String,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub port: u16,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 443,
                tls_cert: "/etc/bmcweb/cert.pem".to_string(),
                tls_key: "/etc/bmcweb/key.pem".to_string(),
                max_connections: 100,
                mtls_enabled: false,
                mtls_ca_cert: "/etc/bmcweb/ca.pem".to_string(),
            },
            auth: AuthConfig {
                session_timeout_seconds: 3600,
                max_sessions: 64,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
            },
        }
    }
}
