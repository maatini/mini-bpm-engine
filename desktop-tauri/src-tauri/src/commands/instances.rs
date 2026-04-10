use crate::state::AppState;
use std::collections::HashMap;

#[tauri::command]
pub async fn start_instance(
    state: tauri::State<'_, AppState>,
    def_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let mut payload = serde_json::json!({
        "definition_key": def_id
    });
    if let Some(vars) = variables {
        if !vars.is_empty() {
            payload["variables"] = serde_json::to_value(vars).unwrap_or_default();
        }
    }

    let data = crate::api_helpers::api_post(&state, "/api/start", &payload).await?;
    let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
    Ok(instance_id)
}

#[tauri::command]
pub async fn start_timer_instance(
    state: tauri::State<'_, AppState>,
    def_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let mut payload = serde_json::json!({
        "definition_key": def_id
    });
    if let Some(vars) = variables {
        if !vars.is_empty() {
            payload["variables"] = serde_json::to_value(vars).unwrap_or_default();
        }
    }

    let data = crate::api_helpers::api_post(&state, "/api/start/timer", &payload).await?;
    let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
    Ok(instance_id)
}

#[tauri::command]
pub async fn list_instances(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/instances").await
}

#[tauri::command]
pub async fn get_instance_details(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, &format!("/api/instances/{}", instance_id)).await
}

#[tauri::command]
pub async fn get_instance_history(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    event_types: Option<String>,
    actor_types: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut path = format!("/api/instances/{}/history", instance_id);
    let mut query_params = Vec::new();
    if let Some(et) = event_types {
        if !et.is_empty() {
            query_params.push(format!("event_types={}", et));
        }
    }
    if let Some(at) = actor_types {
        if !at.is_empty() {
            query_params.push(format!("actor_types={}", at));
        }
    }
    if !query_params.is_empty() {
        path = format!("{}?{}", path, query_params.join("&"));
    }

    crate::api_helpers::api_get(&state, &path).await
}

#[tauri::command]
pub async fn update_instance_variables(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    variables: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    crate::api_helpers::api_put(
        &state,
        &format!("/api/instances/{}/variables", instance_id),
        &serde_json::json!({ "variables": variables }),
    )
    .await
}

#[tauri::command]
pub async fn delete_instance(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    crate::api_helpers::api_delete(&state, &format!("/api/instances/{}", instance_id)).await
}
