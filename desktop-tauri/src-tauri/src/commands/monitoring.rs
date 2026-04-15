use crate::state::{AppState, MonitoringData};

#[tauri::command]
pub async fn get_api_url(state: tauri::State<'_, AppState>) -> Result<String, String> {
    crate::state::get_base_url(&state)
}

#[tauri::command]
pub async fn set_api_url(state: tauri::State<'_, AppState>, url: String) -> Result<(), String> {
    let mut lock = state.base_url.lock().map_err(|e| e.to_string())?;
    *lock = url.trim_end_matches('/').to_string();
    Ok(())
}

#[tauri::command]
pub async fn get_monitoring_data(
    state: tauri::State<'_, AppState>,
) -> Result<MonitoringData, String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/monitoring", base);
    let res = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Engine antwortete mit Status {}", res.status()));
    }
    res.json().await.map_err(|e| format!("Ungültige Antwort: {e}"))
}

#[tauri::command]
pub async fn read_bpmn_file(path: String) -> Result<String, String> {
    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read file '{}': {}", path, e))?;
    Ok(xml)
}

#[tauri::command]
pub async fn get_bucket_entries(
    state: tauri::State<'_, AppState>,
    bucket: String,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/monitoring/buckets/{}/entries?offset={}&limit={}", base, bucket, offset, limit);
    match state.client.get(&url).send().await {
        Ok(res) if res.status().is_success() => {
            let data = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
            Ok(data)
        }
        Ok(res) => Err(res.text().await.unwrap_or_else(|_| "Unknown error".to_string())),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_bucket_entry_detail(
    state: tauri::State<'_, AppState>,
    bucket: String,
    key: String,
) -> Result<serde_json::Value, String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/monitoring/buckets/{}/entries/{}", base, bucket, key);
    match state.client.get(&url).send().await {
        Ok(res) if res.status().is_success() => {
            let data = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
            Ok(data)
        }
        Ok(res) => Err(res.text().await.unwrap_or_else(|_| "Unknown error".to_string())),
        Err(e) => Err(e.to_string()),
    }
}
