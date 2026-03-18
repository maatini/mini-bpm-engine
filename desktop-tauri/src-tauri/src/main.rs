#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(not(feature = "http-backend"))]
use std::collections::HashMap;
#[cfg(not(feature = "http-backend"))]
use std::sync::Arc;
#[cfg(not(feature = "http-backend"))]
use tokio::sync::Mutex;
#[cfg(not(feature = "http-backend"))]
use uuid::Uuid;

#[cfg(feature = "http-backend")]
use std::collections::HashMap;

#[cfg(not(feature = "http-backend"))]
use engine_core::engine::{WorkflowEngine, ProcessInstance, PendingUserTask};
#[cfg(not(feature = "http-backend"))]
use engine_core::model::{ProcessDefinitionBuilder, BpmnElement};
#[cfg(not(feature = "http-backend"))]
use bpmn_parser::parse_bpmn_xml;
#[cfg(not(feature = "http-backend"))]
use persistence_nats::NatsPersistence;

/// Lightweight summary of a deployed process definition.
#[cfg(not(feature = "http-backend"))]
#[derive(serde::Serialize, Clone)]
struct DefinitionInfo {
    id: String,
    node_count: usize,
}

#[cfg(not(feature = "http-backend"))]
struct AppState {
    engine: Arc<Mutex<WorkflowEngine>>,
    /// Stores the original BPMN XML keyed by definition ID (in-memory fallback).
    deployed_xml: Arc<Mutex<HashMap<String, String>>>,
    /// Optional NATS persistence for the bpmn_xml Object Store.
    nats: Option<Arc<NatsPersistence>>,
}

#[cfg(feature = "http-backend")]
struct AppState {
    client: reqwest::Client,
    base_url: String,
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn deploy_simple_process(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut engine = state.engine.lock().await;

    let def = ProcessDefinitionBuilder::new("simple")
        .node("start", BpmnElement::StartEvent)
        .node("task1", BpmnElement::UserTask("admin".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task1")
        .flow("task1", "end")
        .build()
        .map_err(|e| format!("{:?}", e))?;

    engine.deploy_definition(def);
    Ok("Deployed 'simple' process".into())
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn deploy_simple_process(_state: tauri::State<'_, AppState>) -> Result<String, String> {
    Err("deploy_simple_process is not supported in HTTP mode. Use deploy_definition instead.".into())
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn deploy_definition(state: tauri::State<'_, AppState>, xml: String, _name: String) -> Result<String, String> {
    let mut engine = state.engine.lock().await;

    let def = parse_bpmn_xml(&xml).map_err(|e| format!("{:?}", e))?;
    let def_id = def.id.clone();

    // Persist XML: prefer NATS Object Store, fall back to in-memory.
    if let Some(nats) = &state.nats {
        nats.save_bpmn_xml(&def_id, &xml)
            .await
            .map_err(|e| format!("NATS save XML failed: {:?}", e))?;
    }
    // Always keep in-memory copy for fast reads.
    state.deployed_xml.lock().await.insert(def_id.clone(), xml);

    engine.deploy_definition(def);
    Ok(def_id)
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn deploy_definition(state: tauri::State<'_, AppState>, xml: String, name: String) -> Result<String, String> {
    let url = format!("{}/api/deploy", state.base_url);
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
    let def_id = data["definition_id"].as_str().unwrap_or("").to_string();
    Ok(def_id)
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn start_instance(state: tauri::State<'_, AppState>, def_id: String, variables: Option<HashMap<String, serde_json::Value>>) -> Result<String, String> {
    let mut engine = state.engine.lock().await;
    let id = match variables {
        Some(vars) if !vars.is_empty() => {
            engine.start_instance_with_variables(&def_id, vars).await
        }
        _ => engine.start_instance(&def_id).await,
    }
    .map_err(|e| format!("{:?}", e))?;
    Ok(id.to_string())
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn start_instance(state: tauri::State<'_, AppState>, def_id: String, variables: Option<HashMap<String, serde_json::Value>>) -> Result<String, String> {
    let url = format!("{}/api/start", state.base_url);
    let mut payload = serde_json::json!({
        "definition_id": def_id
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
#[cfg(not(feature = "http-backend"))]
async fn get_pending_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<PendingUserTask>, String> {
    let engine = state.engine.lock().await;
    let tasks = engine.get_pending_user_tasks().to_vec();
    Ok(tasks)
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn get_pending_tasks(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/tasks", state.base_url);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get pending tasks failed: {}", res.status()));
    }
    
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn complete_task(state: tauri::State<'_, AppState>, task_id: String) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    engine.complete_user_task(tid, std::collections::HashMap::new()).await.map_err(|e| format!("{:?}", e))?;
    Ok(())
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn complete_task(state: tauri::State<'_, AppState>, task_id: String) -> Result<(), String> {
    let url = format!("{}/api/complete/{}", state.base_url, task_id);
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
#[cfg(not(feature = "http-backend"))]
async fn list_instances(state: tauri::State<'_, AppState>) -> Result<Vec<ProcessInstance>, String> {
    let engine = state.engine.lock().await;
    Ok(engine.list_instances())
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn list_instances(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/instances", state.base_url);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("List instances failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn get_instance_details(state: tauri::State<'_, AppState>, instance_id: String) -> Result<ProcessInstance, String> {
    let engine = state.engine.lock().await;
    let id = Uuid::parse_str(&instance_id).map_err(|e| e.to_string())?;
    engine.get_instance_details(id).map_err(|e| format!("{:?}", e))
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn get_instance_details(state: tauri::State<'_, AppState>, instance_id: String) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/instances/{}", state.base_url, instance_id);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    
    if !res.status().is_success() {
        return Err(format!("Get instance details failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn update_instance_variables(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    variables: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    let id = Uuid::parse_str(&instance_id).map_err(|e| e.to_string())?;
    engine.update_instance_variables(id, variables).map_err(|e| format!("{:?}", e))
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn update_instance_variables(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    variables: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let url = format!("{}/api/instances/{}/variables", state.base_url, instance_id);
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

// ---------------------------------------------------------------------------
// Definitions listing + XML download
// ---------------------------------------------------------------------------

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn list_definitions(state: tauri::State<'_, AppState>) -> Result<Vec<DefinitionInfo>, String> {
    let engine = state.engine.lock().await;
    let defs: Vec<DefinitionInfo> = engine
        .definitions
        .iter()
        .map(|(id, def)| DefinitionInfo {
            id: id.clone(),
            node_count: def.nodes.len(),
        })
        .collect();
    Ok(defs)
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn list_definitions(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/definitions", state.base_url);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("List definitions failed: {}", res.status()));
    }
    let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
#[cfg(not(feature = "http-backend"))]
async fn get_definition_xml(state: tauri::State<'_, AppState>, definition_id: String) -> Result<String, String> {
    // Try in-memory cache first (fast path).
    {
        let xml_store = state.deployed_xml.lock().await;
        if let Some(xml) = xml_store.get(&definition_id) {
            return Ok(xml.clone());
        }
    }

    // Fall back to NATS Object Store.
    if let Some(nats) = &state.nats {
        return nats
            .load_bpmn_xml(&definition_id)
            .await
            .map_err(|e| format!("{:?}", e));
    }

    Err(format!("No XML found for definition '{}'", definition_id))
}

#[tauri::command]
#[cfg(feature = "http-backend")]
async fn get_definition_xml(state: tauri::State<'_, AppState>, definition_id: String) -> Result<String, String> {
    let url = format!("{}/api/definitions/{}/xml", state.base_url, definition_id);
    let res = state.client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Get definition XML failed: {}", res.status()));
    }
    let xml = res.text().await.map_err(|e| e.to_string())?;
    Ok(xml)
}

fn main() {
    // Attempt NATS connection (non-blocking, graceful fallback).
    #[cfg(not(feature = "http-backend"))]
    let nats_persistence: Option<Arc<NatsPersistence>> = {
        tauri::async_runtime::block_on(async {
            match NatsPersistence::connect("nats://localhost:4222", "WORKFLOW_EVENTS").await {
                Ok(p) => {
                    println!("[mini-bpm] Connected to NATS.");
                    Some(Arc::new(p))
                }
                Err(e) => {
                    eprintln!("[mini-bpm] NATS not available, using in-memory only: {}", e);
                    None
                }
            }
        })
    };

    #[cfg(not(feature = "http-backend"))]
    let initial_state = AppState {
        engine: Arc::new(Mutex::new(WorkflowEngine::new())),
        deployed_xml: Arc::new(Mutex::new(HashMap::new())),
        nats: nats_persistence,
    };

    #[cfg(feature = "http-backend")]
    let initial_state = AppState {
        client: reqwest::Client::new(),
        base_url: std::env::var("ENGINE_API_URL").unwrap_or_else(|_| "http://localhost:8080".to_string()),
    };

    tauri::Builder::default()
        .manage(initial_state)
        .invoke_handler(tauri::generate_handler![
            deploy_simple_process,
            deploy_definition,
            start_instance,
            get_pending_tasks,
            complete_task,
            list_instances,
            get_instance_details,
            update_instance_variables,
            list_definitions,
            get_definition_xml
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
