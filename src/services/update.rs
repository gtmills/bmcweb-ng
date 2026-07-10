//! Update Service
//!
//! Implements the Redfish UpdateService for firmware updates

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

use super::task::Task;

/// Update target type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateTarget {
    /// BMC firmware
    BMC,
    /// BIOS firmware
    BIOS,
    /// CPLD firmware
    CPLD,
    /// FPGA firmware
    FPGA,
    /// Other firmware
    Other,
}

impl UpdateTarget {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateTarget::BMC => "BMC",
            UpdateTarget::BIOS => "BIOS",
            UpdateTarget::CPLD => "CPLD",
            UpdateTarget::FPGA => "FPGA",
            UpdateTarget::Other => "Other",
        }
    }
}

/// Update protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateProtocol {
    /// HTTP/HTTPS upload
    HTTP,
    /// TFTP transfer
    TFTP,
    /// SCP transfer
    SCP,
    /// Local file
    Local,
}

/// Firmware inventory item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareInventory {
    /// Unique ID
    pub id: String,
    /// Firmware name
    pub name: String,
    /// Firmware version
    pub version: String,
    /// Target type
    pub target: UpdateTarget,
    /// Whether this is the active version
    pub is_active: bool,
    /// Installation date
    pub installed_at: Option<DateTime<Utc>>,
    /// Updateable flag
    pub updateable: bool,
}

impl FirmwareInventory {
    /// Create a new firmware inventory item
    pub fn new(
        name: String,
        version: String,
        target: UpdateTarget,
        is_active: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            version,
            target,
            is_active,
            installed_at: Some(Utc::now()),
            updateable: true,
        }
    }
}

/// Update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    /// Target for the update
    pub target: UpdateTarget,
    /// Protocol to use
    pub protocol: UpdateProtocol,
    /// Image URI (for remote protocols)
    pub image_uri: Option<String>,
    /// Local file path (for local protocol)
    pub local_path: Option<PathBuf>,
    /// Username for authentication (if needed)
    pub username: Option<String>,
    /// Password for authentication (if needed)
    pub password: Option<String>,
    /// Whether to apply immediately
    pub apply_immediately: bool,
}

/// Update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOperation {
    /// Unique operation ID
    pub id: String,
    /// Associated task ID
    pub task_id: String,
    /// Update request
    pub request: UpdateRequest,
    /// Start time
    pub started_at: DateTime<Utc>,
    /// Completion time
    pub completed_at: Option<DateTime<Utc>>,
    /// Whether operation succeeded
    pub success: Option<bool>,
    /// Error message if failed
    pub error: Option<String>,
}

impl UpdateOperation {
    /// Create a new update operation
    pub fn new(request: UpdateRequest, task_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_id,
            request,
            started_at: Utc::now(),
            completed_at: None,
            success: None,
            error: None,
        }
    }

    /// Mark operation as completed
    pub fn complete(&mut self, success: bool, error: Option<String>) {
        self.completed_at = Some(Utc::now());
        self.success = Some(success);
        self.error = error;
    }
}

/// Update Service for managing firmware updates
#[derive(Debug, Clone)]
pub struct UpdateService {
    /// Firmware inventory
    inventory: Arc<RwLock<HashMap<String, FirmwareInventory>>>,
    /// Active update operations
    operations: Arc<RwLock<HashMap<String, UpdateOperation>>>,
    /// Maximum concurrent updates
    max_concurrent_updates: usize,
}

impl UpdateService {
    /// Create a new update service
    pub fn new(max_concurrent_updates: usize) -> Self {
        info!("Initializing Update Service with max {} concurrent updates", max_concurrent_updates);
        Self {
            inventory: Arc::new(RwLock::new(HashMap::new())),
            operations: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent_updates,
        }
    }

    /// Add firmware to inventory
    pub fn add_firmware(&self, firmware: FirmwareInventory) -> Result<()> {
        let mut inventory = self.inventory.write().unwrap();
        let id = firmware.id.clone();
        
        info!("Adding firmware to inventory: {} v{}", firmware.name, firmware.version);
        inventory.insert(id, firmware);
        
        Ok(())
    }

    /// Get firmware by ID
    pub fn get_firmware(&self, id: &str) -> Option<FirmwareInventory> {
        let inventory = self.inventory.read().unwrap();
        inventory.get(id).cloned()
    }

    /// Get all firmware inventory
    pub fn get_all_firmware(&self) -> Vec<FirmwareInventory> {
        let inventory = self.inventory.read().unwrap();
        inventory.values().cloned().collect()
    }

    /// Get firmware by target
    pub fn get_firmware_by_target(&self, target: UpdateTarget) -> Vec<FirmwareInventory> {
        let inventory = self.inventory.read().unwrap();
        inventory
            .values()
            .filter(|f| f.target == target)
            .cloned()
            .collect()
    }

    /// Get active firmware version for a target
    pub fn get_active_version(&self, target: UpdateTarget) -> Option<String> {
        let inventory = self.inventory.read().unwrap();
        inventory
            .values()
            .find(|f| f.target == target && f.is_active)
            .map(|f| f.version.clone())
    }

    /// Start a firmware update
    pub fn start_update(&self, request: UpdateRequest) -> Result<UpdateOperation> {
        let operations = self.operations.read().unwrap();
        
        // Check concurrent update limit
        let active_count = operations
            .values()
            .filter(|op| op.completed_at.is_none())
            .count();
        
        if active_count >= self.max_concurrent_updates {
            return Err(anyhow!("Maximum concurrent updates reached"));
        }
        
        drop(operations);

        // Create a task for this update
        let _task_name = format!("Firmware Update: {}", request.target.as_str());
        let _task_description = Some(format!(
            "Updating {} firmware",
            request.target.as_str()
        ));
        
        // In a real implementation, we would get the task from TaskService
        // For now, create a placeholder task ID
        let task_id = Uuid::new_v4().to_string();
        
        // Create update operation
        let operation = UpdateOperation::new(request, task_id);
        let op_id = operation.id.clone();
        
        info!("Started firmware update operation: {}", op_id);
        
        let mut operations = self.operations.write().unwrap();
        operations.insert(op_id.clone(), operation.clone());
        
        Ok(operation)
    }

    /// Get update operation by ID
    pub fn get_operation(&self, id: &str) -> Option<UpdateOperation> {
        let operations = self.operations.read().unwrap();
        operations.get(id).cloned()
    }

    /// Get all update operations
    pub fn get_all_operations(&self) -> Vec<UpdateOperation> {
        let operations = self.operations.read().unwrap();
        operations.values().cloned().collect()
    }

    /// Update operation status
    pub fn update_operation_status(
        &self,
        id: &str,
        success: bool,
        error: Option<String>,
    ) -> Result<()> {
        let mut operations = self.operations.write().unwrap();
        
        let operation = operations.get_mut(id)
            .ok_or_else(|| anyhow!("Operation not found"))?;

        operation.complete(success, error.clone());
        
        if success {
            info!("Update operation {} completed successfully", id);
        } else {
            warn!("Update operation {} failed: {:?}", id, error);
        }
        
        Ok(())
    }

    /// Cancel an update operation
    pub fn cancel_operation(&self, id: &str) -> Result<()> {
        let mut operations = self.operations.write().unwrap();
        
        let operation = operations.get_mut(id)
            .ok_or_else(|| anyhow!("Operation not found"))?;

        if operation.completed_at.is_some() {
            return Err(anyhow!("Operation already completed"));
        }

        operation.complete(false, Some("Cancelled by user".to_string()));
        info!("Cancelled update operation: {}", id);
        
        Ok(())
    }

    /// Clean up completed operations
    pub fn cleanup_completed_operations(&self, retention_hours: i64) -> usize {
        let mut operations = self.operations.write().unwrap();
        let cutoff = Utc::now() - chrono::Duration::hours(retention_hours);
        
        let to_remove: Vec<String> = operations
            .iter()
            .filter(|(_, op)| {
                op.completed_at.map_or(false, |completed| completed < cutoff)
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            operations.remove(&id);
        }

        if count > 0 {
            debug!("Cleaned up {} completed update operations", count);
        }
        
        count
    }

    /// Get operation count
    pub fn operation_count(&self) -> usize {
        let operations = self.operations.read().unwrap();
        operations.len()
    }

    /// Get active operation count
    pub fn active_operation_count(&self) -> usize {
        let operations = self.operations.read().unwrap();
        operations
            .values()
            .filter(|op| op.completed_at.is_none())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_inventory() {
        let firmware = FirmwareInventory::new(
            "BMC Firmware".to_string(),
            "1.0.0".to_string(),
            UpdateTarget::BMC,
            true,
        );

        assert_eq!(firmware.name, "BMC Firmware");
        assert_eq!(firmware.version, "1.0.0");
        assert!(firmware.is_active);
        assert!(firmware.updateable);
    }

    #[test]
    fn test_update_service() {
        let service = UpdateService::new(2);
        
        let firmware = FirmwareInventory::new(
            "BMC".to_string(),
            "1.0.0".to_string(),
            UpdateTarget::BMC,
            true,
        );
        
        service.add_firmware(firmware.clone()).unwrap();
        
        let retrieved = service.get_firmware(&firmware.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().version, "1.0.0");
    }

    #[test]
    fn test_get_active_version() {
        let service = UpdateService::new(2);
        
        let firmware = FirmwareInventory::new(
            "BMC".to_string(),
            "2.5.1".to_string(),
            UpdateTarget::BMC,
            true,
        );
        
        service.add_firmware(firmware).unwrap();
        
        let version = service.get_active_version(UpdateTarget::BMC);
        assert_eq!(version, Some("2.5.1".to_string()));
    }

    #[test]
    fn test_start_update() {
        let service = UpdateService::new(2);
        
        let request = UpdateRequest {
            target: UpdateTarget::BMC,
            protocol: UpdateProtocol::HTTP,
            image_uri: Some("https://example.com/firmware.bin".to_string()),
            local_path: None,
            username: None,
            password: None,
            apply_immediately: true,
        };
        
        let operation = service.start_update(request).unwrap();
        assert_eq!(service.operation_count(), 1);
        assert_eq!(service.active_operation_count(), 1);
        
        let retrieved = service.get_operation(&operation.id);
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_concurrent_update_limit() {
        let service = UpdateService::new(1);
        
        let request1 = UpdateRequest {
            target: UpdateTarget::BMC,
            protocol: UpdateProtocol::HTTP,
            image_uri: Some("https://example.com/fw1.bin".to_string()),
            local_path: None,
            username: None,
            password: None,
            apply_immediately: true,
        };
        
        service.start_update(request1).unwrap();
        
        let request2 = UpdateRequest {
            target: UpdateTarget::BIOS,
            protocol: UpdateProtocol::HTTP,
            image_uri: Some("https://example.com/fw2.bin".to_string()),
            local_path: None,
            username: None,
            password: None,
            apply_immediately: true,
        };
        
        // Second update should fail due to limit
        let result = service.start_update(request2);
        assert!(result.is_err());
    }

    #[test]
    fn test_operation_completion() {
        let service = UpdateService::new(2);
        
        let request = UpdateRequest {
            target: UpdateTarget::BMC,
            protocol: UpdateProtocol::Local,
            image_uri: None,
            local_path: Some(PathBuf::from("/tmp/firmware.bin")),
            username: None,
            password: None,
            apply_immediately: false,
        };
        
        let operation = service.start_update(request).unwrap();
        assert_eq!(service.active_operation_count(), 1);
        
        service.update_operation_status(&operation.id, true, None).unwrap();
        assert_eq!(service.active_operation_count(), 0);
        
        let updated = service.get_operation(&operation.id).unwrap();
        assert_eq!(updated.success, Some(true));
        assert!(updated.completed_at.is_some());
    }
}
