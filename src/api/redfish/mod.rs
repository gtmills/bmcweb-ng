//! Redfish API implementation
//!
//! This module implements the DMTF Redfish specification.

use axum::{Router, routing::get};
use std::sync::Arc;

pub mod service_root;

use crate::AppState;

/// Create the Redfish API router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(service_root::get_service_root))
        // TODO: Add more Redfish resource routes:
        // .route("/Systems", get(systems::get_systems_collection))
        // .route("/Chassis", get(chassis::get_chassis_collection))
        // .route("/Managers", get(managers::get_managers_collection))
        // .route("/AccountService", get(accounts::get_account_service))
        // .route("/SessionService", get(sessions::get_session_service))
        // .route("/EventService", get(event_service::get_event_service))
        // .route("/TaskService", get(task_service::get_task_service))
        // .route("/UpdateService", get(update_service::get_update_service))
}

// TODO: Add more Redfish resource modules:
// - systems
// - chassis
// - managers
// - accounts
// - sessions
// - event_service
// - task_service
// - update_service
