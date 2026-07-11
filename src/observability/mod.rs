//! Observability - Logging, Metrics, Tracing
//!
//! Provides structured logging, Prometheus metrics, and OpenTelemetry tracing

pub mod metrics;

pub use metrics::Metrics;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;

use crate::AppState;

/// GET /metrics
///
/// Prometheus metrics endpoint
pub async fn metrics_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Some(metrics) = &state.metrics {
        let output = metrics.gather();
        (StatusCode::OK, output)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, String::from("Metrics not available"))
    }
}

// The /health endpoint lives in protocol::http (health_handler).
// OpenTelemetry tracing setup is planned for a future tracing.rs module.
