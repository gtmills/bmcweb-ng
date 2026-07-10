//! Redfish API implementation
//!
//! This module implements the DMTF Redfish specification.

use axum::{Router, routing::{delete, get, patch, post}};
use std::sync::Arc;

pub mod accounts;
pub mod chassis;
pub mod event_service;
pub mod managers;
pub mod service_root;
pub mod sessions;
pub mod systems;
pub mod task_service;
pub mod update_service;

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
        .route("/Systems/:system_id/Processors",
               get(systems::get_processors_collection))
        .route("/Systems/:system_id/Memory",
               get(systems::get_memory_collection))
        .route("/Systems/:system_id/Storage",
               get(systems::get_storage_collection))
        .route("/Systems/:system_id/EthernetInterfaces",
               get(systems::get_ethernet_interfaces_collection))
        .route("/Systems/:system_id/LogServices",
               get(systems::get_system_log_services))
        // Chassis routes
        .route("/Chassis", get(chassis::get_chassis_collection))
        .route("/Chassis/:chassis_id", get(chassis::get_chassis))
        .route("/Chassis/:chassis_id/Power", get(chassis::get_chassis_power))
        .route("/Chassis/:chassis_id/Thermal", get(chassis::get_chassis_thermal))
        .route("/Chassis/:chassis_id/Sensors", get(chassis::get_chassis_sensors))
        .route("/Chassis/:chassis_id/NetworkAdapters",
               get(chassis::get_chassis_network_adapters))
        // Managers routes
        .route("/Managers", get(managers::get_managers_collection))
        .route("/Managers/:manager_id", get(managers::get_manager))
        .route("/Managers/:manager_id/Actions/Manager.Reset",
               post(managers::reset_manager))
        .route("/Managers/:manager_id/NetworkProtocol",
               get(managers::get_network_protocol)
               .patch(managers::patch_network_protocol))
        .route("/Managers/:manager_id/EthernetInterfaces",
               get(managers::get_manager_ethernet_interfaces))
        .route("/Managers/:manager_id/EthernetInterfaces/:nic_id",
               get(managers::get_manager_ethernet_interface))
        .route("/Managers/:manager_id/LogServices",
               get(managers::get_manager_log_services))
        // SessionService routes
        .route("/SessionService", get(sessions::get_session_service))
        .route("/SessionService", patch(sessions::patch_session_service))
        .route("/SessionService/Sessions",
               get(sessions::get_sessions_collection)
               .post(sessions::create_session))
        // Alias per DSP0266 §13.3.3
        .route("/SessionService/Sessions/Members",
               post(sessions::create_session))
        .route("/SessionService/Sessions/:session_id",
               get(sessions::get_session)
               .delete(sessions::delete_session))
        // AccountService routes
        .route("/AccountService", get(accounts::get_account_service))
        .route("/AccountService/Accounts",
               get(accounts::get_accounts_collection)
               .post(accounts::create_account))
        .route("/AccountService/Accounts/:account_id",
               get(accounts::get_account)
               .patch(accounts::patch_account)
               .delete(accounts::delete_account))
        .route("/AccountService/Roles", get(accounts::get_roles_collection))
        .route("/AccountService/Roles/:role_id", get(accounts::get_role))
        // EventService routes
        .route("/EventService", get(event_service::get_event_service)
               .patch(event_service::patch_event_service))
        .route("/EventService/Actions/EventService.SubmitTestEvent",
               post(event_service::submit_test_event))
        .route("/EventService/Subscriptions",
               get(event_service::get_subscriptions_collection)
               .post(event_service::create_subscription))
        .route("/EventService/Subscriptions/:sub_id",
               get(event_service::get_subscription)
               .delete(event_service::delete_subscription))
        // TaskService routes
        .route("/TaskService", get(task_service::get_task_service))
        .route("/TaskService/Tasks",
               get(task_service::get_tasks_collection))
        .route("/TaskService/Tasks/:task_id",
               get(task_service::get_task)
               .delete(task_service::delete_task))
        // UpdateService routes
        .route("/UpdateService", get(update_service::get_update_service))
        .route("/UpdateService/FirmwareInventory",
               get(update_service::get_firmware_inventory_collection))
        .route("/UpdateService/FirmwareInventory/:firmware_id",
               get(update_service::get_firmware_inventory))
        .route("/UpdateService/Actions/UpdateService.SimpleUpdate",
               post(update_service::simple_update))
        // TODO: Add more Redfish resource routes:
        // .route("/TelemetryService", get(telemetry_service::get_telemetry_service))
        // .route("/CertificateService", get(certificate_service::get_certificate_service))
}

// TODO: Add more Redfish resource modules:
// - telemetry - Telemetry service
// - certificates - Certificate management
