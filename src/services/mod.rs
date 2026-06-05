//! Business logic services
//!
//! Service layer containing business logic for Redfish resources

pub mod event;
pub mod task;
pub mod update;

pub use event::{EventService, EventSubscription, EventMessage, EventType, Protocol};
pub use task::{TaskService, Task, TaskState, TaskMessage};
pub use update::{UpdateService, UpdateRequest, UpdateTarget, UpdateProtocol, FirmwareInventory};

// TODO: Implement additional service modules:
// - system.rs - System/Computer management
// - chassis.rs - Chassis management
// - manager.rs - Manager resources
// - account.rs - Account management
