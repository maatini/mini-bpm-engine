use crate::server::state::{AppError, AppState, parse_uuid};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use engine_core::{PendingServiceTask, PendingUserTask};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub(crate) struct CompleteRequest {
    pub variables: Option<HashMap<String, Value>>,
}

pub(crate) async fn get_tasks(State(state): State<Arc<AppState>>) -> Json<Vec<PendingUserTask>> {
    let engine = &state.engine;
    let tasks = engine.get_pending_user_tasks().to_vec();
    Json(tasks)
}

pub(crate) async fn get_service_tasks(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PendingServiceTask>> {
    let engine = &state.engine;
    let tasks = engine.get_pending_service_tasks().to_vec();
    Json(tasks)
}

pub(crate) async fn complete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<CompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let task_id = parse_uuid(&id)?;
    let vars = payload.variables.unwrap_or_default();

    engine.complete_user_task(task_id, vars).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TopicRequest {
    pub topic_name: String,
    pub lock_duration: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FetchAndLockRequest {
    pub worker_id: String,
    pub max_tasks: usize,
    pub topics: Vec<TopicRequest>,
    pub async_response_timeout: Option<u64>,
}

pub(crate) async fn fetch_and_lock_service_tasks(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FetchAndLockRequest>,
) -> Json<Vec<PendingServiceTask>> {
    let topics: Vec<String> = payload
        .topics
        .iter()
        .map(|t| t.topic_name.clone())
        .collect();
    let lock_duration = payload
        .topics
        .first()
        .map(|t| t.lock_duration)
        .unwrap_or(30);
    let timeout_ms = payload.async_response_timeout.unwrap_or(0);

    let poll_interval = tokio::time::Duration::from_millis(500);
    let max_timeout = tokio::time::Duration::from_millis(timeout_ms.min(30_000));

    let start = tokio::time::Instant::now();
    loop {
        let tasks = {
            let engine = &state.engine;
            engine
                .fetch_and_lock_service_tasks(
                    &payload.worker_id,
                    payload.max_tasks,
                    &topics,
                    lock_duration,
                )
                .await
        };

        if !tasks.is_empty() || timeout_ms == 0 {
            return Json(tasks);
        }

        if start.elapsed() >= max_timeout {
            return Json(vec![]);
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompleteServiceTaskRequest {
    pub worker_id: String,
    pub variables: Option<HashMap<String, Value>>,
}

pub(crate) async fn complete_service_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<CompleteServiceTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let task_id = parse_uuid(&id)?;
    let vars = payload.variables.unwrap_or_default();

    engine
        .complete_service_task(task_id, &payload.worker_id, vars)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FailServiceTaskRequest {
    pub worker_id: String,
    pub retries: Option<i32>,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
}

pub(crate) async fn fail_service_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<FailServiceTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let task_id = parse_uuid(&id)?;

    engine
        .fail_service_task(
            task_id,
            &payload.worker_id,
            payload.retries,
            payload.error_message,
            payload.error_details,
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtendLockRequest {
    pub worker_id: String,
    pub new_duration: i64,
}

pub(crate) async fn extend_lock(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<ExtendLockRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let task_id = parse_uuid(&id)?;

    engine
        .extend_lock(task_id, &payload.worker_id, payload.new_duration)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BpmnErrorRequest {
    pub worker_id: String,
    pub error_code: String,
}

pub(crate) async fn bpmn_error(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<BpmnErrorRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let task_id = parse_uuid(&id)?;

    engine
        .handle_bpmn_error(task_id, &payload.worker_id, &payload.error_code)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
