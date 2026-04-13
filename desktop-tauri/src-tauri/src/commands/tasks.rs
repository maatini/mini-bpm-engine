use crate::state::AppState;
use std::collections::HashMap;

#[tauri::command]
pub async fn get_pending_tasks(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/tasks").await
}

#[tauri::command]
pub async fn complete_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "variables": variables.unwrap_or_default()
    });
    crate::api_helpers::api_post_no_body(&state, &format!("/api/complete/{}", task_id), &payload)
        .await
}

#[tauri::command]
pub async fn get_pending_service_tasks(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/service-tasks").await
}

#[tauri::command]
pub async fn fetch_and_lock_service_tasks(
    state: tauri::State<'_, AppState>,
    worker_id: String,
    max_tasks: usize,
    topic_name: String,
    lock_duration: i64,
) -> Result<serde_json::Value, String> {
    let payload = serde_json::json!({
        "workerId": worker_id,
        "maxTasks": max_tasks,
        "topics": [
            {
                "topicName": topic_name,
                "lockDuration": lock_duration
            }
        ]
    });
    crate::api_helpers::api_post(&state, "/api/service-task/fetchAndLock", &payload).await
}

#[tauri::command]
pub async fn complete_service_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
    worker_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "workerId": worker_id,
        "variables": variables.unwrap_or_default()
    });
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/service-task/{}/complete", task_id),
        &payload,
    )
    .await
}

#[tauri::command]
pub async fn retry_incident(
    state: tauri::State<'_, AppState>,
    task_id: String,
    retries: Option<i32>,
) -> Result<(), String> {
    let payload = serde_json::json!({ "retries": retries });
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/service-task/{}/retry", task_id),
        &payload,
    )
    .await
}

#[tauri::command]
pub async fn resolve_incident(
    state: tauri::State<'_, AppState>,
    task_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "variables": variables.unwrap_or_default()
    });
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/service-task/{}/resolve", task_id),
        &payload,
    )
    .await
}

#[tauri::command]
pub async fn get_pending_timers(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/timers").await
}

#[tauri::command]
pub async fn get_pending_message_catches(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/messages").await
}
