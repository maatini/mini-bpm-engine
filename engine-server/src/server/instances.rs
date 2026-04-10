use crate::server::state::{AppError, AppState, parse_uuid};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use engine_core::ProcessInstance;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub(crate) struct StartRequest {
    pub definition_key: String,
    #[serde(default)]
    pub variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
pub(crate) struct StartResponse {
    pub instance_id: String,
}

pub(crate) async fn start_instance(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartRequest>,
) -> Result<Json<StartResponse>, AppError> {
    let engine = &state.engine;
    let def_key = parse_uuid(&payload.definition_key)?;
    let id = match payload.variables {
        Some(vars) if !vars.is_empty() => engine.start_instance_with_variables(def_key, vars).await,
        _ => engine.start_instance(def_key).await,
    }?;

    Ok(Json(StartResponse {
        instance_id: id.to_string(),
    }))
}

#[derive(Serialize, Deserialize)]
pub(crate) struct StartLatestRequest {
    pub bpmn_id: String,
    #[serde(default)]
    pub variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
pub(crate) struct StartLatestResponse {
    pub instance_id: String,
    pub definition_key: String,
    pub version: i32,
}

pub(crate) async fn start_instance_latest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartLatestRequest>,
) -> Result<Json<StartLatestResponse>, AppError> {
    let engine = &state.engine;
    let vars = payload.variables.unwrap_or_default();
    let (inst_id, def_key) = engine.start_instance_latest(&payload.bpmn_id, vars).await?;

    let def = engine
        .get_definition(&def_key)
        .await
        .ok_or_else(|| AppError::BadRequest("Definition not found".into()))?;
    let version = def.version;

    Ok(Json(StartLatestResponse {
        instance_id: inst_id.to_string(),
        definition_key: def_key.to_string(),
        version,
    }))
}

pub(crate) async fn start_timer_instance(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartRequest>,
) -> Result<Json<StartResponse>, AppError> {
    let engine = Arc::clone(&state.engine);
    let def_key = parse_uuid(&payload.definition_key)?;
    let vars = payload.variables.unwrap_or_default();
    let id = engine.start_timer_instance(def_key, vars).await?;

    Ok(Json(StartResponse {
        instance_id: id.to_string(),
    }))
}

pub(crate) async fn list_instances(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProcessInstance>> {
    let engine = &state.engine;
    let instances = engine.list_instances().await;
    Json(instances)
}

pub(crate) async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProcessInstance>, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;

    let instance = engine.get_instance_details(instance_id).await?;

    Ok(Json(instance))
}

#[derive(Deserialize)]
pub(crate) struct UpdateVariablesRequest {
    pub variables: HashMap<String, Value>,
}

pub(crate) async fn update_instance_variables(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateVariablesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;

    engine
        .update_instance_variables(instance_id, payload.variables)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn delete_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;

    engine.delete_instance(instance_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
