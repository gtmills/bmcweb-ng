//! Redfish API implementation
//!
//! This module implements the DMTF Redfish specification.

use axum::{Router, routing::{get, post}};
use std::sync::Arc;

pub mod service_root;
pub mod systems;
pub mod chassis;
pub mod managers;

use crate::AppState;

/// Create the Redfish API router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(service_root::get_service_root))
        // Systems routes
        .route("/Systems", get(systems::get_systems_collection))
        .route("/Systems/:system_id", get(systems::get_system))
        .route("/Systems/:system_id/Actions/ComputerSystem.Reset",
               post(systems::reset_system))
        // Chassis routes
        .route("/Chassis", get(chassis::get_chassis_collection))
        .route("/Chassis/:chassis_id", get(chassis::get_chassis))
        // Managers routes
        .route("/Managers", get(managers::get_managers_collection))
        .route("/Managers/:manager_id", get(managers::get_manager))
        .route("/Managers/:manager_id/Actions/Manager.Reset",
               post(managers::reset_manager))
        // TODO: Add more Redfish resource routes:
        // .route("/AccountService", get(accounts::get_account_service))
        // .route("/SessionService", get(sessions::get_session_service))
        // .route("/EventService", get(event_service::get_event_service))
        // .route("/TaskService", get(task_service::get_task_service))
        // .route("/UpdateService", get(update_service::get_update_service))
}

// TODO: Add more Redfish resource modules:
// - accounts - Account management
// - sessions - Session management
// - event_service - Event subscriptions
// - task_service - Task management
// - update_service - Firmware updates
// - telemetry - Telemetry service
// - certificates - Certificate management
