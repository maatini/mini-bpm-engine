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
pub async fn suspend_instance(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/instances/{}/suspend", instance_id),
        &serde_json::json!({}),
    )
    .await
}

#[tauri::command]
pub async fn resume_instance(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), String> {
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/instances/{}/resume", instance_id),
        &serde_json::json!({}),
    )
    .await
}

#[tauri::command]
pub async fn move_token(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    target_node_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
    cancel_current: Option<bool>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "target_node_id": target_node_id,
        "variables": variables.unwrap_or_default(),
        "cancel_current": cancel_current.unwrap_or(true),
    });
    crate::api_helpers::api_post_no_body(
        &state,
        &format!("/api/instances/{}/move-token", instance_id),
        &payload,
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

#[tauri::command]
pub async fn query_completed_instances(
    state: tauri::State<'_, AppState>,
    definition_key: Option<String>,
    business_key: Option<String>,
    from: Option<String>,
    to: Option<String>,
    state_filter: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<serde_json::Value, String> {
    let mut params = Vec::new();
    if let Some(dk) = definition_key {
        params.push(format!("definition_key={dk}"));
    }
    if let Some(bk) = business_key {
        params.push(format!("business_key={bk}"));
    }
    if let Some(f) = from {
        params.push(format!("from={f}"));
    }
    if let Some(t) = to {
        params.push(format!("to={t}"));
    }
    if let Some(s) = state_filter {
        params.push(format!("state={s}"));
    }
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(o) = offset {
        params.push(format!("offset={o}"));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    crate::api_helpers::api_get(&state, &format!("/api/history/instances{query}")).await
}

#[tauri::command]
pub async fn get_completed_instance(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, &format!("/api/history/instances/{instance_id}")).await
}
