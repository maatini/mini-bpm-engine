#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use engine_core::engine::{WorkflowEngine, PendingUserTask, ProcessInstance};
use engine_core::model::{ProcessDefinitionBuilder, BpmnElement};
use bpmn_parser::parse_bpmn_xml;

struct AppState {
    engine: Arc<Mutex<WorkflowEngine>>,
}

#[tauri::command]
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
async fn deploy_definition(state: tauri::State<'_, AppState>, xml: String, _name: String) -> Result<String, String> {
    let mut engine = state.engine.lock().await;

    let def = parse_bpmn_xml(&xml).map_err(|e| format!("{:?}", e))?;
    let def_id = def.id.clone();
    engine.deploy_definition(def);
    Ok(def_id)
}

#[tauri::command]
async fn start_instance(state: tauri::State<'_, AppState>, def_id: String) -> Result<String, String> {
    let mut engine = state.engine.lock().await;
    let id = engine.start_instance(&def_id).await.map_err(|e| format!("{:?}", e))?;
    Ok(id.to_string())
}

#[tauri::command]
async fn get_pending_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<PendingUserTask>, String> {
    let engine = state.engine.lock().await;
    let tasks = engine.get_pending_user_tasks().to_vec();
    Ok(tasks)
}

#[tauri::command]
async fn complete_task(state: tauri::State<'_, AppState>, task_id: String) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    let tid = Uuid::parse_str(&task_id).map_err(|e| e.to_string())?;
    engine.complete_user_task(tid, std::collections::HashMap::new()).await.map_err(|e| format!("{:?}", e))?;
    Ok(())
}

#[tauri::command]
async fn list_instances(state: tauri::State<'_, AppState>) -> Result<Vec<ProcessInstance>, String> {
    let engine = state.engine.lock().await;
    Ok(engine.list_instances())
}

#[tauri::command]
async fn get_instance_details(state: tauri::State<'_, AppState>, instance_id: String) -> Result<ProcessInstance, String> {
    let engine = state.engine.lock().await;
    let id = Uuid::parse_str(&instance_id).map_err(|e| e.to_string())?;
    engine.get_instance_details(id).map_err(|e| format!("{:?}", e))
}

fn main() {
    let engine = WorkflowEngine::new();

    tauri::Builder::default()
        .manage(AppState {
            engine: Arc::new(Mutex::new(engine)),
        })
        .invoke_handler(tauri::generate_handler![
            deploy_simple_process,
            deploy_definition,
            start_instance,
            get_pending_tasks,
            complete_task,
            list_instances,
            get_instance_details
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
