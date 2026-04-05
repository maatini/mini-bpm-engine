use crate::server::state::{AppError, AppState, parse_uuid};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub(crate) struct ServerHistoryQuery {
    pub event_types: Option<String>,
    pub node_id: Option<String>,
    pub actor_type: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub(crate) async fn get_instance_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<ServerHistoryQuery>,
) -> Result<Json<Vec<engine_core::history::HistoryEntry>>, AppError> {
    let instance_id = parse_uuid(&id)?;
    if let Some(p) = &state.persistence {
        let parsed_event_types = query.event_types.map(|s| {
            s.split(',')
                .filter_map(|part| serde_json::from_value(serde_json::json!(part.trim())).ok())
                .collect::<Vec<_>>()
        });

        let parsed_actor_type = query
            .actor_type
            .and_then(|s| serde_json::from_value(serde_json::json!(s.trim())).ok());

        let history_query = engine_core::persistence::HistoryQuery {
            instance_id,
            event_types: parsed_event_types,
            node_id: query.node_id,
            actor_type: parsed_actor_type,
            from: query.from,
            to: query.to,
            limit: query.limit,
            offset: query.offset,
        };

        let history = p
            .query_history(history_query)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to query history: {:?}", e)))?;

        Ok(Json(history))
    } else {
        Ok(Json(vec![]))
    }
}

pub(crate) async fn get_instance_history_entry(
    State(state): State<Arc<AppState>>,
    Path((id, event_id)): Path<(String, String)>,
) -> Result<Json<engine_core::history::HistoryEntry>, AppError> {
    let instance_id = parse_uuid(&id)?;
    let event_uuid = parse_uuid(&event_id)?;

    if let Some(p) = &state.persistence {
        let history = p
            .query_history(engine_core::persistence::HistoryQuery {
                instance_id,
                ..Default::default()
            })
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to query history: {:?}", e)))?;
        if let Some(entry) = history.into_iter().find(|e| e.id == event_uuid) {
            Ok(Json(entry))
        } else {
            Err(AppError::BadRequest(format!(
                "History entry {event_uuid} not found"
            )))
        }
    } else {
        Err(AppError::BadRequest(
            "No persistence configured".to_string(),
        ))
    }
}
