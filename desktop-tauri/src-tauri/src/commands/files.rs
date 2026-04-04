use crate::state::AppState;

#[tauri::command]
pub async fn upload_instance_file(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    var_name: String,
    file_path: String,
) -> Result<serde_json::Value, String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/instances/{}/files/{}", base, instance_id, var_name);

    let file_data = tokio::fs::read(&file_path)
        .await
        .map_err(|e| format!("Cannot read file: {}", e))?;

    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mime_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_data)
        .file_name(filename)
        .mime_str(&mime_type)
        .map_err(|e| e.to_string())?;

    let form = reqwest::multipart::Form::new().part("file", part);

    let res = state
        .client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Upload failed with status: {}", res.status()));
    }

    Ok(serde_json::json!({"status": "success"}))
}

#[tauri::command]
pub async fn download_instance_file(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    var_name: String,
    save_path: String,
) -> Result<(), String> {
    let base = crate::state::get_base_url(&state)?;
    let url = format!("{}/api/instances/{}/files/{}", base, instance_id, var_name);

    let res = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Download failed with status: {}", res.status()));
    }

    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    tokio::fs::write(&save_path, bytes)
        .await
        .map_err(|e| format!("Cannot write file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_instance_file(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    var_name: String,
) -> Result<(), String> {
    crate::api_helpers::api_delete(
        &state,
        &format!("/api/instances/{}/files/{}", instance_id, var_name),
    )
    .await
}
