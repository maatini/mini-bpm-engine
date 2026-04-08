use axum::{Json, http::StatusCode, response::IntoResponse};
use engine_core::WorkflowEngine;
use engine_core::error::EngineError;
use engine_core::persistence::WorkflowPersistence;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unified error type for all REST handlers.
pub(crate) enum AppError {
    /// Wraps an `EngineError` with automatic status-code mapping.
    Engine(EngineError),
    /// Client sent a malformed request (invalid UUID, bad XML, etc.).
    BadRequest(String),
}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Engine(EngineError::InvalidDefinition(msg)) => (StatusCode::BAD_REQUEST, msg),
            Self::Engine(EngineError::NoMatchingCondition(msg)) => (
                StatusCode::BAD_REQUEST,
                format!("No matching condition at gateway '{msg}'"),
            ),
            Self::Engine(EngineError::NoSuchDefinition(id)) => {
                (StatusCode::NOT_FOUND, format!("Definition not found: {id}"))
            }
            Self::Engine(EngineError::NoSuchInstance(id)) => {
                (StatusCode::NOT_FOUND, format!("Instance not found: {id}"))
            }
            Self::Engine(EngineError::NoSuchNode(id)) => {
                (StatusCode::NOT_FOUND, format!("Node not found: {id}"))
            }
            Self::Engine(EngineError::ServiceTaskNotFound(id)) => (
                StatusCode::NOT_FOUND,
                format!("Service task not found: {id}"),
            ),
            Self::Engine(EngineError::TaskNotPending {
                task_id,
                actual_state,
            }) => (
                StatusCode::CONFLICT,
                format!("Task '{task_id}' is not pending (state: {actual_state})"),
            ),
            Self::Engine(EngineError::ServiceTaskLocked { task_id, worker_id }) => (
                StatusCode::CONFLICT,
                format!("Task '{task_id}' locked by worker '{worker_id}'"),
            ),
            Self::Engine(EngineError::ServiceTaskNotLocked(id)) => (
                StatusCode::CONFLICT,
                format!("Service task '{id}' is not locked"),
            ),
            Self::Engine(EngineError::AlreadyCompleted) => (
                StatusCode::CONFLICT,
                "Process instance already completed".to_string(),
            ),
            Self::Engine(EngineError::DefinitionHasInstances(count)) => (
                StatusCode::CONFLICT,
                format!("Cannot delete definition: {count} instances still exist"),
            ),
            Self::Engine(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

/// Parse a UUID from a path segment, returning `AppError::BadRequest` on failure.
pub(crate) fn parse_uuid(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::BadRequest("Invalid UUID format".to_string()))
}

pub struct AppState {
    pub(crate) engine: Arc<WorkflowEngine>,
    pub(crate) persistence: Option<Arc<dyn WorkflowPersistence>>,
    pub(crate) deployed_xml: Arc<RwLock<HashMap<String, String>>>,
    pub(crate) nats_url: String, // Store URL for /api/info
}
