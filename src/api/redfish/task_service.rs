//! Redfish TaskService endpoints
//!
//! Implements the Redfish TaskService resource family:
//! - GET /redfish/v1/TaskService
//! - GET /redfish/v1/TaskService/Tasks
//! - GET /redfish/v1/TaskService/Tasks/{task_id}
//! - DELETE /redfish/v1/TaskService/Tasks/{task_id}
//!
//! Tasks are created indirectly by other operations (e.g., firmware updates)
//! that return a 202 Accepted with a `Location` header pointing here.
//!
//! Reference: DMTF DSP0266, TaskService schema v1.2.0, Task schema v1.8.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::services::{Task, TaskState};
use crate::AppState;

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn task_to_json(task: &Task) -> Value {
    let messages: Vec<Value> = task
        .messages
        .iter()
        .map(|m| {
            json!({
                "MessageId": m.message_id,
                "Message": m.message,
                "Severity": m.severity,
            })
        })
        .collect();

    json!({
        "@odata.type": "#Task.v1_8_0.Task",
        "@odata.id": format!("/redfish/v1/TaskService/Tasks/{}", task.id),
        "Id": task.id,
        "Name": task.name,
        "Description": task.description,
        "TaskState": task.state.to_redfish_string(),
        "StartTime": task.start_time.to_rfc3339(),
        "EndTime": task.end_time.map(|t| t.to_rfc3339()),
        "PercentComplete": task.percent_complete,
        "Messages": messages,
        "TaskStatus": task_health_from_state(&task.state),
    })
}

fn task_health_from_state(state: &TaskState) -> &'static str {
    match state {
        TaskState::Exception => "Critical",
        TaskState::Killed => "Warning",
        TaskState::Cancelled => "Warning",
        _ => "OK",
    }
}

// ---------------------------------------------------------------------------
// TaskService resource
// ---------------------------------------------------------------------------

/// GET /redfish/v1/TaskService
pub async fn get_task_service(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TaskService");

    let task_service = state.task_service.as_ref();
    let _total = task_service.map(|s| s.task_count()).unwrap_or(0);
    let _active = task_service.map(|s| s.active_task_count()).unwrap_or(0);

    let response = json!({
        "@odata.type": "#TaskService.v1_2_0.TaskService",
        "@odata.id": "/redfish/v1/TaskService",
        "Id": "TaskService",
        "Name": "Task Service",
        "Description": "Redfish Task Service",
        "ServiceEnabled": true,
        "DateTime": chrono::Utc::now().to_rfc3339(),
        "CompletedTaskOverWritePolicy": "Oldest",
        "LifeCycleEventOnTaskStateChange": true,
        "Status": {
            "State": "Enabled",
            "Health": "OK"
        },
        "Tasks": {
            "@odata.id": "/redfish/v1/TaskService/Tasks"
        }
    });

    Ok(Json(response))
}

/// GET /redfish/v1/TaskService/Tasks
///
/// Returns the TaskCollection listing all tasks.
pub async fn get_tasks_collection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TaskService/Tasks");

    let task_service = state
        .task_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let tasks = task_service.get_all_tasks();
    let members: Vec<Value> = tasks
        .iter()
        .map(|t| json!({ "@odata.id": format!("/redfish/v1/TaskService/Tasks/{}", t.id) }))
        .collect();
    let count = members.len();

    let response = json!({
        "@odata.type": "#TaskCollection.TaskCollection",
        "@odata.id": "/redfish/v1/TaskService/Tasks",
        "Name": "Task Collection",
        "Members@odata.count": count,
        "Members": members,
    });

    Ok(Json(response))
}

/// GET /redfish/v1/TaskService/Tasks/{task_id}
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    debug!("GET /redfish/v1/TaskService/Tasks/{}", task_id);

    let task_service = state
        .task_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    match task_service.get_task(&task_id) {
        Some(task) => Ok(Json(task_to_json(&task))),
        None => {
            warn!("Task '{}' not found", task_id);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// DELETE /redfish/v1/TaskService/Tasks/{task_id}
///
/// Removes a task from the task store.
pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("DELETE /redfish/v1/TaskService/Tasks/{}", task_id);

    let task_service = state
        .task_service
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    task_service.delete_task(&task_id).map_err(|e| {
        warn!("Failed to delete task '{}': {}", task_id, e);
        StatusCode::NOT_FOUND
    })?;

    info!("Deleted task '{}'", task_id);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_get_task_service() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_task_service(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["@odata.type"], "#TaskService.v1_2_0.TaskService");
        assert_eq!(json["ServiceEnabled"], true);
    }

    #[tokio::test]
    async fn test_get_tasks_collection_empty() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_tasks_collection(State(state)).await;
        assert!(result.is_ok());
        let json = result.unwrap().0;
        assert_eq!(json["Members@odata.count"], 0);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = get_task(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let config = Config::default();
        let state = Arc::new(AppState::new(config));
        let result = delete_task(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
