//! Business logic services
//!
//! Service layer containing business logic for Redfish resources

pub mod event;
pub mod task;
pub mod update;

pub use event::{EventService, EventSubscription, EventMessage, EventType, Protocol};
pub use task::{TaskService, Task, TaskState, TaskMessage};
pub use update::{UpdateService, UpdateRequest, UpdateTarget, UpdateProtocol, FirmwareInventory};

// System, chassis, manager, and account resources are handled directly in
// the api::redfish handlers (systems.rs, chassis.rs, managers.rs, accounts.rs).
// No separate service modules are required for those resources.
