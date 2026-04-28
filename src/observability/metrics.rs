//! Prometheus Metrics
//!
//! Provides metrics collection and exposition for monitoring

use prometheus::{
    Counter, Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry,
};
use std::sync::Arc;
use tracing::warn;

/// Metrics collector for bmcweb-ng
#[derive(Clone)]
pub struct Metrics {
    registry: Arc<Registry>,
    
    // HTTP metrics
    pub http_requests_total: IntCounter,
    pub http_request_duration_seconds: Histogram,
    pub http_requests_in_flight: IntGauge,
    
    // Authentication metrics
    pub auth_attempts_total: IntCounter,
    pub auth_failures_total: IntCounter,
    pub active_sessions: IntGauge,
    
    // Redfish API metrics
    pub redfish_requests_total: IntCounter,
    pub redfish_errors_total: IntCounter,
    
    // DBus metrics
    pub dbus_calls_total: IntCounter,
    pub dbus_errors_total: IntCounter,
    pub dbus_call_duration_seconds: Histogram,
    
    // System metrics
    pub uptime_seconds: Gauge,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // HTTP metrics
        let http_requests_total = IntCounter::with_opts(
            Opts::new("bmcweb_http_requests_total", "Total number of HTTP requests")
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let http_request_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "bmcweb_http_request_duration_seconds",
                "HTTP request duration in seconds"
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0])
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        let http_requests_in_flight = IntGauge::with_opts(
            Opts::new("bmcweb_http_requests_in_flight", "Number of HTTP requests currently being processed")
        )?;
        registry.register(Box::new(http_requests_in_flight.clone()))?;

        // Authentication metrics
        let auth_attempts_total = IntCounter::with_opts(
            Opts::new("bmcweb_auth_attempts_total", "Total number of authentication attempts")
        )?;
        registry.register(Box::new(auth_attempts_total.clone()))?;

        let auth_failures_total = IntCounter::with_opts(
            Opts::new("bmcweb_auth_failures_total", "Total number of authentication failures")
        )?;
        registry.register(Box::new(auth_failures_total.clone()))?;

        let active_sessions = IntGauge::with_opts(
            Opts::new("bmcweb_active_sessions", "Number of active user sessions")
        )?;
        registry.register(Box::new(active_sessions.clone()))?;

        // Redfish API metrics
        let redfish_requests_total = IntCounter::with_opts(
            Opts::new("bmcweb_redfish_requests_total", "Total number of Redfish API requests")
        )?;
        registry.register(Box::new(redfish_requests_total.clone()))?;

        let redfish_errors_total = IntCounter::with_opts(
            Opts::new("bmcweb_redfish_errors_total", "Total number of Redfish API errors")
        )?;
        registry.register(Box::new(redfish_errors_total.clone()))?;

        // DBus metrics
        let dbus_calls_total = IntCounter::with_opts(
            Opts::new("bmcweb_dbus_calls_total", "Total number of DBus calls")
        )?;
        registry.register(Box::new(dbus_calls_total.clone()))?;

        let dbus_errors_total = IntCounter::with_opts(
            Opts::new("bmcweb_dbus_errors_total", "Total number of DBus errors")
        )?;
        registry.register(Box::new(dbus_errors_total.clone()))?;

        let dbus_call_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "bmcweb_dbus_call_duration_seconds",
                "DBus call duration in seconds"
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
        )?;
        registry.register(Box::new(dbus_call_duration_seconds.clone()))?;

        // System metrics
        let uptime_seconds = Gauge::with_opts(
            Opts::new("bmcweb_uptime_seconds", "Server uptime in seconds")
        )?;
        registry.register(Box::new(uptime_seconds.clone()))?;

        Ok(Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration_seconds,
            http_requests_in_flight,
            auth_attempts_total,
            auth_failures_total,
            active_sessions,
            redfish_requests_total,
            redfish_errors_total,
            dbus_calls_total,
            dbus_errors_total,
            dbus_call_duration_seconds,
            uptime_seconds,
        })
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Gather all metrics in Prometheus text format
    pub fn gather(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        
        match encoder.encode_to_string(&metric_families) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to encode metrics: {}", e);
                String::new()
            }
        }
    }

    /// Update session count metric
    pub fn update_session_count(&self, count: usize) {
        self.active_sessions.set(count as i64);
    }

    /// Record HTTP request
    pub fn record_http_request(&self, duration_seconds: f64) {
        self.http_requests_total.inc();
        self.http_request_duration_seconds.observe(duration_seconds);
    }

    /// Record authentication attempt
    pub fn record_auth_attempt(&self, success: bool) {
        self.auth_attempts_total.inc();
        if !success {
            self.auth_failures_total.inc();
        }
    }

    /// Record Redfish request
    pub fn record_redfish_request(&self, is_error: bool) {
        self.redfish_requests_total.inc();
        if is_error {
            self.redfish_errors_total.inc();
        }
    }

    /// Record DBus call
    pub fn record_dbus_call(&self, duration_seconds: f64, is_error: bool) {
        self.dbus_calls_total.inc();
        self.dbus_call_duration_seconds.observe(duration_seconds);
        if is_error {
            self.dbus_errors_total.inc();
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_record_http_request() {
        let metrics = Metrics::new().unwrap();
        metrics.record_http_request(0.1);
        
        // Verify counter incremented
        assert_eq!(metrics.http_requests_total.get(), 1);
    }

    #[test]
    fn test_record_auth_attempt() {
        let metrics = Metrics::new().unwrap();
        
        // Successful auth
        metrics.record_auth_attempt(true);
        assert_eq!(metrics.auth_attempts_total.get(), 1);
        assert_eq!(metrics.auth_failures_total.get(), 0);
        
        // Failed auth
        metrics.record_auth_attempt(false);
        assert_eq!(metrics.auth_attempts_total.get(), 2);
        assert_eq!(metrics.auth_failures_total.get(), 1);
    }

    #[test]
    fn test_gather_metrics() {
        let metrics = Metrics::new().unwrap();
        metrics.record_http_request(0.1);
        
        let output = metrics.gather();
        assert!(output.contains("bmcweb_http_requests_total"));
        assert!(output.contains("bmcweb_http_request_duration_seconds"));
    }

    #[test]
    fn test_update_session_count() {
        let metrics = Metrics::new().unwrap();
        metrics.update_session_count(5);
        assert_eq!(metrics.active_sessions.get(), 5);
    }
}
