use crate::state::AppState;
use serde_json::Value;

/// GET request helper - eliminates identical code blocks
pub async fn api_get(state: &AppState, path: &str) -> Result<Value, String> {
    let base = crate::state::get_base_url(state)?;
    let url = format!("{}{}", base, path);
    let res = state
        .client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Request failed: {} {}", res.status(), path));
    }
    res.json().await.map_err(|e| e.to_string())
}

/// POST request helper with JSON body
pub async fn api_post(state: &AppState, path: &str, body: &Value) -> Result<Value, String> {
    let base = crate::state::get_base_url(state)?;
    let url = format!("{}{}", base, path);
    let res = state
        .client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Request failed: {} {}", res.status(), path));
    }
    res.json().await.map_err(|e| e.to_string())
}

/// POST request returning no body (204 No Content)
pub async fn api_post_no_body(state: &AppState, path: &str, body: &Value) -> Result<(), String> {
    let base = crate::state::get_base_url(state)?;
    let url = format!("{}{}", base, path);
    let res = state
        .client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Request failed: {} {}", res.status(), path));
    }
    Ok(())
}

/// PUT request
pub async fn api_put(state: &AppState, path: &str, body: &Value) -> Result<(), String> {
    let base = crate::state::get_base_url(state)?;
    let url = format!("{}{}", base, path);
    let res = state
        .client
        .put(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Request failed: {} {}", res.status(), path));
    }
    Ok(())
}

/// DELETE request
pub async fn api_delete(state: &AppState, path: &str) -> Result<(), String> {
    let base = crate::state::get_base_url(state)?;
    let url = format!("{}{}", base, path);
    let res = state
        .client
        .delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Request failed: {} {}", res.status(), path));
    }
    Ok(())
}
