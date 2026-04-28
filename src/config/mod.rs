//! Configuration management for bmcweb-ng

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub features: FeaturesConfig,
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
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub methods: Vec<String>,
    pub session_timeout_seconds: u64,
    pub max_sessions: usize,
}

/// Feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesConfig {
    pub redfish: bool,
    pub dbus_rest: bool,
    pub kvm: bool,
    pub virtual_media: bool,
    pub event_service: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
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

    /// Get default configuration
    pub fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                port: 443,
                tls_cert: "/etc/bmcweb/cert.pem".to_string(),
                tls_key: "/etc/bmcweb/key.pem".to_string(),
                max_connections: 100,
            },
            auth: AuthConfig {
                methods: vec!["basic".to_string(), "session".to_string()],
                session_timeout_seconds: 3600,
                max_sessions: 64,
            },
            features: FeaturesConfig {
                redfish: true,
                dbus_rest: true,
                kvm: true,
                virtual_media: true,
                event_service: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
            },
        }
    }
}
