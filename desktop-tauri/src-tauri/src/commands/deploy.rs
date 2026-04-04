use crate::state::AppState;

#[tauri::command]
pub async fn deploy_simple_process(_state: tauri::State<'_, AppState>) -> Result<String, String> {
    Err(
        "deploy_simple_process is not supported in HTTP mode. Use deploy_definition instead."
            .into(),
    )
}

#[tauri::command]
pub async fn deploy_definition(
    state: tauri::State<'_, AppState>,
    xml: String,
    name: String,
) -> Result<String, String> {
    let payload = serde_json::json!({
        "xml": xml,
        "name": name
    });

    let data = crate::api_helpers::api_post(&state, "/api/deploy", &payload).await?;
    let def_key = data["definition_key"].as_str().unwrap_or("").to_string();
    Ok(def_key)
}

#[tauri::command]
pub async fn list_definitions(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    crate::api_helpers::api_get(&state, "/api/definitions").await
}

#[tauri::command]
pub async fn get_definition_xml(
    state: tauri::State<'_, AppState>,
    definition_id: String,
) -> Result<String, String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/definitions/{}/xml", base, definition_id);
    let res = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Get definition XML failed: {}", res.status()));
    }

    res.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_definition(
    state: tauri::State<'_, AppState>,
    definition_id: String,
    cascade: bool,
) -> Result<(), String> {
    let path = format!("/api/definitions/{}?cascade={}", definition_id, cascade);
    crate::api_helpers::api_delete(&state, &path).await
}
