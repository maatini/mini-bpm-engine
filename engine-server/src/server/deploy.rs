use crate::server::state::{AppError, AppState, parse_uuid};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub(crate) struct DeployRequest {
    pub xml: String,
    pub name: String,
}

#[derive(Serialize)]
pub(crate) struct DeployResponse {
    pub definition_key: String,
    pub version: i32,
}

pub(crate) async fn deploy_definition(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, AppError> {
    const MAX_XML_BYTES: usize = 10 * 1024 * 1024; // 10MB
    if payload.xml.len() > MAX_XML_BYTES {
        return Err(AppError::BadRequest(format!(
            "XML too large: {} bytes (max {})",
            payload.xml.len(),
            MAX_XML_BYTES
        )));
    }
    let engine = &state.engine;
    let def = bpmn_parser::parse_bpmn_xml(&payload.xml)
        .map_err(|e| AppError::BadRequest(format!("Invalid BPMN XML: {e:?}")))?;
    let (key, version) = engine.deploy_definition(def).await;
    let key_str = key.to_string();

    if let Some(persistence) = &state.persistence {
        if let Err(e) = persistence.save_bpmn_xml(&key_str, &payload.xml).await {
            tracing::error!("Failed to save BPMN XML to persistence layer: {:?}", e);
        }
    }
    state
        .deployed_xml
        .write()
        .await
        .insert(key_str.clone(), payload.xml.clone());

    Ok(Json(DeployResponse {
        definition_key: key_str,
        version,
    }))
}

#[derive(Serialize)]
pub(crate) struct DefinitionInfo {
    pub key: String,
    pub bpmn_id: String,
    pub version: i32,
    pub node_count: usize,
    pub is_latest: bool,
}

pub(crate) async fn list_definitions(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DefinitionInfo>> {
    let engine = &state.engine;
    let raw = engine.list_definitions().await;

    // Determine the latest version per bpmn_id
    let mut latest_versions: std::collections::HashMap<String, i32> =
        std::collections::HashMap::new();
    for (_, bpmn_id, version, _) in &raw {
        let entry = latest_versions.entry(bpmn_id.clone()).or_insert(0);
        if *version > *entry {
            *entry = *version;
        }
    }

    let defs: Vec<DefinitionInfo> = raw
        .into_iter()
        .map(|(key, bpmn_id, version, node_count)| {
            let is_latest = latest_versions.get(&bpmn_id).copied() == Some(version);
            DefinitionInfo {
                key: key.to_string(),
                bpmn_id,
                version,
                node_count,
                is_latest,
            }
        })
        .collect();
    Json(defs)
}

pub(crate) async fn get_definition_xml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<String, AppError> {
    {
        let xml_store = &state.deployed_xml;
        if let Some(xml) = xml_store.read().await.get(&id) {
            return Ok(xml.clone());
        }
    }

    if let Some(persistence) = &state.persistence {
        if let Ok(xml) = persistence.load_bpmn_xml(&id).await {
            return Ok(xml);
        }
    }

    Err(AppError::BadRequest(format!(
        "No XML found for definition '{id}'"
    )))
}

#[derive(Deserialize)]
pub(crate) struct DeleteDefinitionQuery {
    pub cascade: Option<bool>,
}

pub(crate) async fn delete_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DeleteDefinitionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let def_key = parse_uuid(&id)?;

    engine
        .delete_definition(def_key, query.cascade.unwrap_or(false))
        .await?;

    state.deployed_xml.write().await.remove(&id);

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn delete_all_definitions(
    State(state): State<Arc<AppState>>,
    Path(bpmn_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DeleteDefinitionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;

    // Get all versions before deleting so we can clean up the XML cache
    let versions = engine.list_definition_versions(&bpmn_id).await;

    engine
        .delete_all_definitions(&bpmn_id, query.cascade.unwrap_or(false))
        .await?;

    let mut xml_cache = state.deployed_xml.write().await;
    for (key, _, _) in versions {
        xml_cache.remove(&key.to_string());
    }

    Ok(StatusCode::NO_CONTENT)
}
