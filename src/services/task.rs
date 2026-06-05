//! Task Service
//!
//! Implements the Redfish TaskService for managing long-running operations

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is new and not yet started
    New,
    /// Task is starting
    Starting,
    /// Task is running
    Running,
    /// Task is suspended
    Suspended,
    /// Task is interrupted
    Interrupted,
    /// Task is pending (waiting for resources)
    Pending,
    /// Task is stopping
    Stopping,
    /// Task completed successfully
    Completed,
    /// Task was killed
    Killed,
    /// Task encountered an exception
    Exception,
    /// Task was cancelled
    Cancelled,
}

impl TaskState {
    /// Convert to Redfish task state string
    pub fn to_redfish_string(&self) -> &'static str {
        match self {
            TaskState::New => "New",
            TaskState::Starting => "Starting",
            TaskState::Running => "Running",
            TaskState::Suspended => "Suspended",
            TaskState::Interrupted => "Interrupted",
            TaskState::Pending => "Pending",
            TaskState::Stopping => "Stopping",
            TaskState::Completed => "Completed",
            TaskState::Killed => "Killed",
            TaskState::Exception => "Exception",
            TaskState::Cancelled => "Cancelled",
        }
    }

    /// Check if task is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed
                | TaskState::Killed
                | TaskState::Exception
                | TaskState::Cancelled
        )
    }
}

/// Task message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    /// Message ID
    #[serde(rename = "MessageId")]
    pub message_id: String,
    /// Message text
    #[serde(rename = "Message")]
    pub message: String,
    /// Severity
    #[serde(rename = "Severity")]
    pub severity: String,
    /// Resolution
    #[serde(rename = "Resolution", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

impl TaskMessage {
    /// Create a new task message
    pub fn new(message_id: String, message: String, severity: String) -> Self {
        Self {
            message_id,
            message,
            severity,
            resolution: None,
        }
    }

    /// Create an info message
    pub fn info(message: String) -> Self {
        Self::new("Task.Info".to_string(), message, "OK".to_string())
    }

    /// Create a warning message
    pub fn warning(message: String) -> Self {
        Self::new("Task.Warning".to_string(), message, "Warning".to_string())
    }

    /// Create an error message
    pub fn error(message: String) -> Self {
        Self::new("Task.Error".to_string(), message, "Critical".to_string())
    }
}

/// Task representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task ID
    pub id: String,
    /// Task name
    pub name: String,
    /// Task description
    pub description: Option<String>,
    /// Current state
    pub state: TaskState,
    /// Start time
    pub start_time: DateTime<Utc>,
    /// End time (if completed)
    pub end_time: Option<DateTime<Utc>>,
    /// Progress percentage (0-100)
    pub percent_complete: Option<u8>,
    /// Task messages
    pub messages: Vec<TaskMessage>,
    /// Task payload (operation-specific data)
    pub payload: Option<serde_json::Value>,
}

impl Task {
    /// Create a new task
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            state: TaskState::New,
            start_time: Utc::now(),
            end_time: None,
            percent_complete: Some(0),
            messages: Vec::new(),
            payload: None,
        }
    }

    /// Update task state
    pub fn set_state(&mut self, state: TaskState) {
        self.state = state;
        if state.is_terminal() {
            self.end_time = Some(Utc::now());
            self.percent_complete = Some(100);
        }
    }

    /// Update progress
    pub fn set_progress(&mut self, percent: u8) {
        self.percent_complete = Some(percent.min(100));
    }

    /// Add a message to the task
    pub fn add_message(&mut self, message: TaskMessage) {
        self.messages.push(message);
    }

    /// Set task payload
    pub fn set_payload(&mut self, payload: serde_json::Value) {
        self.payload = Some(payload);
    }

    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        self.state.is_terminal()
    }

    /// Get task duration
    pub fn duration(&self) -> Option<Duration> {
        self.end_time.map(|end| end - self.start_time)
    }
}

/// Task Service for managing long-running operations
#[derive(Debug, Clone)]
pub struct TaskService {
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    max_tasks: usize,
    retention_duration: Duration,
}

impl TaskService {
    /// Create a new task service
    pub fn new(max_tasks: usize, retention_hours: i64) -> Self {
        info!("Initializing Task Service with max {} tasks", max_tasks);
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            max_tasks,
            retention_duration: Duration::hours(retention_hours),
        }
    }

    /// Create a new task
    pub fn create_task(&self, name: String, description: Option<String>) -> Result<Task> {
        let mut tasks = self.tasks.write().unwrap();

        // Clean up old completed tasks if at limit
        if tasks.len() >= self.max_tasks {
            self.cleanup_old_tasks_internal(&mut tasks);
            
            // If still at limit, fail
            if tasks.len() >= self.max_tasks {
                return Err(anyhow!("Maximum number of tasks reached"));
            }
        }

        let task = Task::new(name, description);
        let id = task.id.clone();
        
        info!("Created task: {} - {}", id, task.name);
        tasks.insert(id.clone(), task.clone());
        
        Ok(task)
    }

    /// Get a task by ID
    pub fn get_task(&self, id: &str) -> Option<Task> {
        let tasks = self.tasks.read().unwrap();
        tasks.get(id).cloned()
    }

    /// Get all tasks
    pub fn get_all_tasks(&self) -> Vec<Task> {
        let tasks = self.tasks.read().unwrap();
        tasks.values().cloned().collect()
    }

    /// Update task state
    pub fn update_task_state(&self, id: &str, state: TaskState) -> Result<()> {
        let mut tasks = self.tasks.write().unwrap();
        
        let task = tasks.get_mut(id)
            .ok_or_else(|| anyhow!("Task not found"))?;

        task.set_state(state);
        debug!("Updated task {} state to {:?}", id, state);
        
        Ok(())
    }

    /// Update task progress
    pub fn update_task_progress(&self, id: &str, percent: u8) -> Result<()> {
        let mut tasks = self.tasks.write().unwrap();
        
        let task = tasks.get_mut(id)
            .ok_or_else(|| anyhow!("Task not found"))?;

        task.set_progress(percent);
        debug!("Updated task {} progress to {}%", id, percent);
        
        Ok(())
    }

    /// Add message to task
    pub fn add_task_message(&self, id: &str, message: TaskMessage) -> Result<()> {
        let mut tasks = self.tasks.write().unwrap();
        
        let task = tasks.get_mut(id)
            .ok_or_else(|| anyhow!("Task not found"))?;

        task.add_message(message);
        debug!("Added message to task {}", id);
        
        Ok(())
    }

    /// Set task payload
    pub fn set_task_payload(&self, id: &str, payload: serde_json::Value) -> Result<()> {
        let mut tasks = self.tasks.write().unwrap();
        
        let task = tasks.get_mut(id)
            .ok_or_else(|| anyhow!("Task not found"))?;

        task.set_payload(payload);
        debug!("Set payload for task {}", id);
        
        Ok(())
    }

    /// Delete a task
    pub fn delete_task(&self, id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().unwrap();
        
        tasks.remove(id)
            .ok_or_else(|| anyhow!("Task not found"))?;
        
        info!("Deleted task: {}", id);
        Ok(())
    }

    /// Clean up old completed tasks
    pub fn cleanup_old_tasks(&self) -> usize {
        let mut tasks = self.tasks.write().unwrap();
        self.cleanup_old_tasks_internal(&mut tasks)
    }

    /// Internal cleanup implementation
    fn cleanup_old_tasks_internal(&self, tasks: &mut HashMap<String, Task>) -> usize {
        let now = Utc::now();
        let cutoff = now - self.retention_duration;
        
        let to_remove: Vec<String> = tasks
            .iter()
            .filter(|(_, task)| {
                task.is_complete() && task.end_time.map_or(false, |end| end < cutoff)
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            tasks.remove(&id);
        }

        if count > 0 {
            debug!("Cleaned up {} old tasks", count);
        }
        
        count
    }

    /// Get task count
    pub fn task_count(&self) -> usize {
        let tasks = self.tasks.read().unwrap();
        tasks.len()
    }

    /// Get active task count
    pub fn active_task_count(&self) -> usize {
        let tasks = self.tasks.read().unwrap();
        tasks.values().filter(|t| !t.is_complete()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new(
            "Test Task".to_string(),
            Some("A test task".to_string()),
        );

        assert_eq!(task.name, "Test Task");
        assert_eq!(task.state, TaskState::New);
        assert_eq!(task.percent_complete, Some(0));
        assert!(!task.is_complete());
    }

    #[test]
    fn test_task_state_transitions() {
        let mut task = Task::new("Test".to_string(), None);
        
        task.set_state(TaskState::Running);
        assert_eq!(task.state, TaskState::Running);
        assert!(!task.is_complete());
        
        task.set_state(TaskState::Completed);
        assert_eq!(task.state, TaskState::Completed);
        assert!(task.is_complete());
        assert!(task.end_time.is_some());
        assert_eq!(task.percent_complete, Some(100));
    }

    #[test]
    fn test_task_service() {
        let service = TaskService::new(10, 24);
        
        let task = service.create_task(
            "Test Task".to_string(),
            Some("Description".to_string()),
        ).unwrap();

        assert_eq!(service.task_count(), 1);
        assert_eq!(service.active_task_count(), 1);
        
        let retrieved = service.get_task(&task.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Task");
    }

    #[test]
    fn test_task_progress_update() {
        let service = TaskService::new(10, 24);
        let task = service.create_task("Test".to_string(), None).unwrap();
        
        service.update_task_progress(&task.id, 50).unwrap();
        
        let updated = service.get_task(&task.id).unwrap();
        assert_eq!(updated.percent_complete, Some(50));
    }

    #[test]
    fn test_task_messages() {
        let service = TaskService::new(10, 24);
        let task = service.create_task("Test".to_string(), None).unwrap();
        
        service.add_task_message(
            &task.id,
            TaskMessage::info("Task started".to_string()),
        ).unwrap();
        
        service.add_task_message(
            &task.id,
            TaskMessage::warning("Warning occurred".to_string()),
        ).unwrap();
        
        let updated = service.get_task(&task.id).unwrap();
        assert_eq!(updated.messages.len(), 2);
        assert_eq!(updated.messages[0].severity, "OK");
        assert_eq!(updated.messages[1].severity, "Warning");
    }

    #[test]
    fn test_task_limit() {
        let service = TaskService::new(2, 24);
        
        service.create_task("Task1".to_string(), None).unwrap();
        service.create_task("Task2".to_string(), None).unwrap();
        
        // Third task should fail
        let result = service.create_task("Task3".to_string(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_task_deletion() {
        let service = TaskService::new(10, 24);
        let task = service.create_task("Test".to_string(), None).unwrap();
        
        assert_eq!(service.task_count(), 1);
        
        service.delete_task(&task.id).unwrap();
        assert_eq!(service.task_count(), 0);
    }
}
