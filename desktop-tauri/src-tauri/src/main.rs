#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::collections::HashMap;



/// Engine + NATS metrics returned to the Monitoring page.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct MonitoringData {
    definitions_count: usize,
    instances_total: usize,
    instances_running: usize,
    instances_completed: usize,
    pending_user_tasks: usize,
    pending_service_tasks: usize,
    nats_server: Option<NatsServerInfo>,
}

/// NATS server and JetStream account information.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct NatsServerInfo {
    server_name: String,
    version: String,
    host: String,
    port: u16,
    memory_bytes: u64,
    storage_bytes: u64,
    streams: usize,
    consumers: usize,
}

struct AppState {
    client: reqwest::Client,
    base_url: std::sync::Mutex<String>,
}

#[tauri::command]
async fn deploy_simple_process(_state: tauri::State<'_, AppState>) -> Result<String, String> {
    Err("deploy_simple_process is not supported in HTTP mode. Use deploy_definition instead.".into())
}

#[tauri::command]
async fn deploy_definition(state: tauri::State<'_, AppState>, xml: String, name: String) -> Result<String, String> {
    let url = format!("{}/api/deploy", *state.base_url.lock().unwrap());
    let payload = serde_json::json!({
        "xml": xml,
        "name": name
    });
    
    let res = state.client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        return Err(format!("Deploy failed with status: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let def_key = data["definition_key"].as_str().unwrap_or("").to_string();
    Ok(def_key)
}

#[tauri::command]
async fn start_instance(state: tauri::State<'_, AppState>, def_id: String, variables: Option<HashMap<String, serde_json::Value>>) -> Result<String, String> {
    let url = format!("{}/api/start", *state.base_url.lock().unwrap());
    let mut payload = serde_json::json!({
        "definition_key": def_id
    });
    if let Some(vars) = variables {
        if !vars.is_empty() {
            payload["variables"] = serde_json::to_value(vars).unwrap_or_default();
        }
    }
    
    let res = state.client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        return Err(format!("Start instance failed with status: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let instance_id = data["instance_id"].as_str().unwrap_or("").to_string();
    Ok(instance_id)
}

#[tauri::command]
async fn get_pending_tasks(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/tasks", *state.base_url.lock().unwrap());
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get pending tasks failed: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn complete_task(state: tauri::State<'_, AppState>, task_id: String) -> Result<(), String> {
    let url = format!("{}/api/complete/{}", *state.base_url.lock().unwrap(), task_id);
    let payload = serde_json::json!({
        "variables": {}
    });
    
    let res = state.client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        return Err(format!("Complete task failed with status: {}", res.status()));
    }
    Ok(())
}

#[tauri::command]
async fn get_pending_service_tasks(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/service-tasks", *state.base_url.lock().unwrap());
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get pending service tasks failed: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn fetch_and_lock_service_tasks(
    state: tauri::State<'_, AppState>,
    worker_id: String,
    max_tasks: usize,
    topic_name: String,
    lock_duration: i64,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/service-task/fetchAndLock", *state.base_url.lock().unwrap());
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
    
    let res = state.client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        return Err(format!("Fetch and lock failed: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn complete_service_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
    worker_id: String,
    variables: Option<HashMap<String, serde_json::Value>>,
) -> Result<(), String> {
    let url = format!("{}/api/service-task/{}/complete", *state.base_url.lock().unwrap(), task_id);
    let payload = serde_json::json!({
        "workerId": worker_id,
        "variables": variables.unwrap_or_default()
    });
    
    let res = state.client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !res.status().is_success() {
        return Err(format!("Complete service task failed with status: {}", res.status()));
    }
    Ok(())
}

#[tauri::command]
async fn list_instances(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/instances", *state.base_url.lock().unwrap());
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("List instances failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn get_instance_details(state: tauri::State<'_, AppState>, instance_id: String) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/instances/{}", *state.base_url.lock().unwrap(), instance_id);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get instance details failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn get_instance_history(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    event_types: Option<String>,
    actor_types: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut url = format!("{}/api/instances/{}/history", *state.base_url.lock().unwrap(), instance_id);
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
        url = format!("{}?{}", url, query_params.join("&"));
    }
    
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get instance history failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn update_instance_variables(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    variables: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let url = format!("{}/api/instances/{}/variables", *state.base_url.lock().unwrap(), instance_id);
    let res = state.client
        .put(&url)
        .json(&serde_json::json!({ "variables": variables }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Update variables failed: {}", res.status()));
    }
    Ok(())
}

#[tauri::command]
async fn delete_instance(state: tauri::State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let url = format!("{}/api/instances/{}", *state.base_url.lock().unwrap(), instance_id);
    let res = state.client.delete(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Delete instance failed: {}", res.status()));
    }
    Ok(())
}

#[tauri::command]
async fn list_definitions(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/definitions", *state.base_url.lock().unwrap());
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("List definitions failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
async fn get_definition_xml(state: tauri::State<'_, AppState>, definition_id: String) -> Result<String, String> {
    let url = format!("{}/api/definitions/{}/xml", *state.base_url.lock().unwrap(), definition_id);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Get definition XML failed: {}", res.status()));
    }
    let xml = res.text().await.map_err(|e| e.to_string())?;
    Ok(xml)
}

#[tauri::command]
async fn delete_definition(state: tauri::State<'_, AppState>, definition_id: String, cascade: bool) -> Result<(), String> {
    let url = format!("{}/api/definitions/{}?cascade={}", *state.base_url.lock().unwrap(), definition_id, cascade);
    let res = state.client.delete(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Delete definition failed: {}", res.status()));
    }
    Ok(())
}

#[tauri::command]
async fn get_api_url(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(state.base_url.lock().unwrap().clone())
}

#[tauri::command]
async fn set_api_url(state: tauri::State<'_, AppState>, url: String) -> Result<(), String> {
    let mut lock = state.base_url.lock().unwrap();
    *lock = url.trim_end_matches('/').to_string();
    Ok(())
}

#[tauri::command]
async fn get_monitoring_data(state: tauri::State<'_, AppState>) -> Result<MonitoringData, String> {
    let url = format!("{}/api/monitoring", *state.base_url.lock().unwrap());
    match state.client.get(&url).send().await {
        Ok(res) if res.status().is_success() => {
            let data: MonitoringData = res.json().await.map_err(|e| e.to_string())?;
            Ok(data)
        }
        _ => {
            // Return dummy data if engine server doesn't respond properly for monitoring
            Ok(MonitoringData {
                definitions_count: 0,
                instances_total: 0,
                instances_running: 0,
                instances_completed: 0,
                pending_user_tasks: 0,
                pending_service_tasks: 0,
                nats_server: None,
            })
        }
    }
}

#[tauri::command]
async fn read_bpmn_file(path: String) -> Result<String, String> {
    let xml = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read file '{}': {}", path, e))?;
    Ok(xml)
}

fn main() {
    let initial_state = AppState {
        client: reqwest::Client::new(),
        base_url: std::sync::Mutex::new(std::env::var("ENGINE_API_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())),
    };

    tauri::Builder::default()
        .manage(initial_state)
        .invoke_handler(tauri::generate_handler![
            deploy_simple_process,
            deploy_definition,
            start_instance,
            get_pending_tasks,
            complete_task,
            get_pending_service_tasks,
            fetch_and_lock_service_tasks,
            complete_service_task,
            list_instances,
            get_instance_details,
            update_instance_variables,
            delete_instance,
            delete_definition,
            list_definitions,
            get_definition_xml,
            get_api_url,
            set_api_url,
            get_monitoring_data,
            read_bpmn_file,
            get_instance_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
