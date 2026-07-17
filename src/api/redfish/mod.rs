//! Redfish API implementation
//!
//! This module implements the DMTF Redfish specification.

use axum::{Router, routing::{get, patch, post}};
use std::sync::Arc;

pub mod accounts;
pub mod aggregation_service;
pub mod certificate_service;
pub mod chassis;
pub mod event_service;
pub mod fabrics;
pub mod managers;
pub mod odata;
pub mod service_root;
pub mod sessions;
pub mod systems;
pub mod task_service;
pub mod telemetry_service;
pub mod update_service;

use crate::AppState;

/// Create the Redfish API router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // NOTE: GET / (service root) is intentionally NOT here.
        // It is served unauthenticated from the open router in http.rs per Redfish spec §7.3.1.
        // OData service document (§12.6 of DSP0266)
        // NOTE: /$metadata is served unauthenticated from the open router in http.rs
        .route("/odata", get(odata::get_odata))
        // Systems routes
        .route("/Systems", get(systems::get_systems_collection))
        .route("/Systems/:system_id",
               get(systems::get_system)
               .patch(systems::patch_system))
        .route("/Systems/:system_id/Actions/ComputerSystem.Reset",
               post(systems::reset_system))
        .route("/Systems/:system_id/Bios",
               get(systems::get_bios))
        .route("/Systems/:system_id/Bios/Actions/Bios.ResetBios",
               post(systems::reset_bios))
        .route("/Systems/:system_id/Processors",
               get(systems::get_processors_collection))
        .route("/Systems/:system_id/Processors/:processor_id",
               get(systems::get_processor))
        .route("/Systems/:system_id/Processors/:processor_id/EnvironmentMetrics",
               get(systems::get_processor_environment_metrics))
        .route("/Systems/:system_id/Memory",
               get(systems::get_memory_collection))
        .route("/Systems/:system_id/Memory/:memory_id",
               get(systems::get_memory))
        .route("/Systems/:system_id/Storage",
               get(systems::get_storage_collection))
        .route("/Systems/:system_id/Storage/:storage_id/Controllers/:controller_id",
               get(systems::get_storage_controller))
        .route("/Systems/:system_id/FabricAdapters",
               get(systems::get_fabric_adapters))
        .route("/Systems/:system_id/FabricAdapters/:adapter_id",
               get(systems::get_fabric_adapter))
        .route("/Systems/:system_id/Processors/:processor_id/OperatingConfigs",
               get(systems::get_processor_operating_configs))
        .route("/Systems/:system_id/Processors/:processor_id/OperatingConfigs/:config_id",
               get(systems::get_processor_operating_config))
        .route("/Systems/:system_id/EthernetInterfaces",
               get(systems::get_ethernet_interfaces_collection))
        .route("/Systems/:system_id/NetworkInterfaces",
               get(systems::get_network_interfaces_collection))
        .route("/Systems/:system_id/LogServices",
               get(systems::get_system_log_services))
        .route("/Systems/:system_id/LogServices/EventLog",
               get(systems::get_system_event_log))
        .route("/Systems/:system_id/LogServices/EventLog/Entries",
               get(systems::get_event_log_entries))
        .route("/Systems/:system_id/LogServices/EventLog/Entries/:entry_id",
               get(systems::get_event_log_entry))
        .route("/Systems/:system_id/LogServices/EventLog/Actions/LogService.ClearLog",
               post(systems::clear_event_log))
        .route("/Systems/:system_id/LogServices/EventLog/Actions/LogService.ClearLog/ActionInfo",
               get(systems::get_clear_event_log_action_info))
        .route("/Systems/:system_id/LogServices/PostCodes",
               get(systems::get_post_codes_log_service))
        .route("/Systems/:system_id/LogServices/PostCodes/Entries",
               get(systems::get_post_codes_entries))
        .route("/Systems/:system_id/LogServices/HostLogger",
               get(systems::get_host_logger_log_service))
        .route("/Systems/:system_id/LogServices/HostLogger/Entries",
               get(systems::get_host_logger_entries))
        .route("/Systems/:system_id/Actions/ComputerSystem.Reset/ActionInfo",
               get(systems::get_reset_action_info))
        .route("/Systems/:system_id/Storage/:storage_id",
               get(systems::get_storage))
        .route("/Systems/:system_id/PCIeDevices",
               get(systems::get_pcie_devices_collection))
        .route("/Systems/:system_id/PCIeDevices/:pcie_id",
               get(systems::get_pcie_device))
        .route("/Systems/hypervisor",
               get(systems::get_hypervisor_system))
        // Chassis routes
        .route("/Chassis", get(chassis::get_chassis_collection))
        .route("/Chassis/:chassis_id",
               get(chassis::get_chassis)
               .patch(chassis::patch_chassis))
        .route("/Chassis/:chassis_id/Power", get(chassis::get_chassis_power))
        .route("/Chassis/:chassis_id/Thermal", get(chassis::get_chassis_thermal))
        .route("/Chassis/:chassis_id/Sensors", get(chassis::get_chassis_sensors))
        .route("/Chassis/:chassis_id/NetworkAdapters",
               get(chassis::get_chassis_network_adapters))
        .route("/Chassis/:chassis_id/Assembly",
               get(chassis::get_chassis_assembly))
        .route("/Chassis/:chassis_id/PowerSubsystem",
               get(chassis::get_chassis_power_subsystem))
        .route("/Chassis/:chassis_id/PowerSubsystem/PowerSupplies",
               get(chassis::get_chassis_power_supplies))
        .route("/Chassis/:chassis_id/ThermalSubsystem",
               get(chassis::get_chassis_thermal_subsystem))
        .route("/Chassis/:chassis_id/ThermalSubsystem/Fans",
               get(chassis::get_chassis_fans))
        .route("/Chassis/:chassis_id/ThermalSubsystem/Fans/:fan_id",
               get(chassis::get_chassis_fan))
        .route("/Chassis/:chassis_id/PowerSubsystem/PowerSupplies/:psu_id",
               get(chassis::get_chassis_power_supply))
        .route("/Chassis/:chassis_id/ThermalSubsystem/ThermalMetrics",
               get(chassis::get_chassis_thermal_metrics))
        .route("/Chassis/:chassis_id/PCIeSlots",
               get(chassis::get_chassis_pcie_slots))
        .route("/Chassis/:chassis_id/NetworkAdapters/:adapter_id",
               get(chassis::get_chassis_network_adapter))
        .route("/Chassis/:chassis_id/Drives",
               get(chassis::get_chassis_drives))
        .route("/Chassis/:chassis_id/Drives/:drive_id",
               get(chassis::get_chassis_drive))
        // Cables routes
        .route("/Cables", get(chassis::get_cables_collection))
        .route("/Cables/:cable_id", get(chassis::get_cable))
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
               get(managers::get_manager_ethernet_interface)
               .patch(managers::patch_manager_ethernet_interface))
        .route("/Managers/:manager_id/LogServices",
               get(managers::get_manager_log_services))
        .route("/Managers/:manager_id/LogServices/BMC",
               get(managers::get_manager_bmc_log_service))
        .route("/Managers/:manager_id/LogServices/BMC/Entries",
               get(managers::get_manager_bmc_log_entries))
        .route("/Managers/:manager_id/LogServices/BMC/Entries/:entry_id",
               get(managers::get_manager_bmc_log_entry))
        .route("/Managers/:manager_id/LogServices/BMC/Actions/LogService.ClearLog",
               post(managers::clear_manager_bmc_log))
        .route("/Managers/:manager_id/LogServices/Journal",
               get(managers::get_manager_journal_log_service))
        .route("/Managers/:manager_id/LogServices/Journal/Entries",
               get(managers::get_manager_journal_entries))
        .route("/Managers/:manager_id/LogServices/DBusEventLog",
               get(managers::get_manager_dbus_eventlog_service))
        .route("/Managers/:manager_id/LogServices/DBusEventLog/Entries",
               get(managers::get_manager_dbus_eventlog_entries))
        .route("/Managers/:manager_id/ManagerDiagnosticData",
               get(managers::get_manager_diagnostic_data))
        .route("/Managers/:manager_id/NetworkProtocol/HTTPS/Certificates",
               get(certificate_service::get_https_certificates_collection))
        .route("/Managers/:manager_id/NetworkProtocol/HTTPS/Certificates/:cert_id",
               get(certificate_service::get_https_certificate))
        // SessionService routes.
        // NOTE: POST /SessionService/Sessions (login) is intentionally NOT
        // registered here — it is served from the unauthenticated login router
        // built in http.rs so it can bypass the mandatory auth middleware.
        .route("/SessionService", get(sessions::get_session_service))
        .route("/SessionService", patch(sessions::patch_session_service))
        .route("/SessionService/Sessions",
               get(sessions::get_sessions_collection))
        .route("/SessionService/Sessions/:session_id",
               get(sessions::get_session)
               .delete(sessions::delete_session))
        // AccountService routes
        .route("/AccountService",
               get(accounts::get_account_service)
               .patch(accounts::patch_account_service))
        .route("/AccountService/PrivilegeMap",
               get(accounts::get_privilege_map))
        .route("/AccountService/Accounts",
               get(accounts::get_accounts_collection)
               .post(accounts::create_account))
        .route("/AccountService/Accounts/:account_id",
               get(accounts::get_account)
               .patch(accounts::patch_account)
               .delete(accounts::delete_account))
        .route("/AccountService/Roles", get(accounts::get_roles_collection))
        .route("/AccountService/Roles/:role_id", get(accounts::get_role))
        // Registries and JsonSchemas
        .route("/Registries",    get(service_root::get_registries_collection))
        .route("/Registries/:registry_id", get(service_root::get_registry))
        .route("/JsonSchemas",   get(service_root::get_json_schemas_collection))
        .route("/JsonSchemas/:schema_id", get(service_root::get_json_schema))
        // EventService routes
        .route("/EventService", get(event_service::get_event_service)
               .patch(event_service::patch_event_service))
        .route("/EventService/SSE", get(event_service::get_event_service_sse))
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
        .route("/UpdateService/SoftwareInventory",
               get(update_service::get_software_inventory_collection))
        .route("/UpdateService/SoftwareInventory/:software_id",
               get(update_service::get_software_inventory))
        .route("/UpdateService/Actions/UpdateService.SimpleUpdate",
               post(update_service::simple_update))
        // Fabrics routes
        .route("/Fabrics",
               get(fabrics::get_fabrics_collection))
        .route("/Fabrics/:fabric_id",
               get(fabrics::get_fabric))
        .route("/Fabrics/:fabric_id/Switches",
               get(fabrics::get_fabric_switches))
        .route("/Fabrics/:fabric_id/Switches/:switch_id",
               get(fabrics::get_fabric_switch))
        // AggregationService route
        .route("/AggregationService",
               get(aggregation_service::get_aggregation_service))
        // CertificateService routes
        .route("/CertificateService",
               get(certificate_service::get_certificate_service))
        .route("/CertificateService/CertificateLocations",
               get(certificate_service::get_certificate_locations))
        // TelemetryService routes
        .route("/TelemetryService",
               get(telemetry_service::get_telemetry_service))
        .route("/TelemetryService/MetricDefinitions",
               get(telemetry_service::get_metric_definitions))
        .route("/TelemetryService/MetricReportDefinitions",
               get(telemetry_service::get_metric_report_definitions))
        .route("/TelemetryService/MetricReports",
               get(telemetry_service::get_metric_reports))
        .route("/TelemetryService/Triggers",
               get(telemetry_service::get_triggers_collection)
               .post(telemetry_service::create_trigger))
        .route("/TelemetryService/Triggers/:trigger_id",
               get(telemetry_service::get_trigger)
               .patch(telemetry_service::patch_trigger)
               .delete(telemetry_service::delete_trigger))
}
