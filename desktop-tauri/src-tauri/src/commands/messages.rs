use crate::state::AppState;
use std::collections::HashMap;

#[tauri::command]
pub async fn correlate_message(
    state: tauri::State<'_, AppState>,
    message_name: String,
    business_key: Option<String>,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<Vec<String>, String> {
    let payload = serde_json::json!({
        "messageName": message_name,
        "businessKey": business_key,
        "variables": variables.unwrap_or_default()
    });

    let data = crate::api_helpers::api_post(&state, "/api/message", &payload).await?;
    let ids = data["affectedInstances"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(ids)
}
