use crate::server::state::{AppError, AppState};
use axum::{Json, extract::State};
use engine_core::runtime::PendingMessageCatch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CorrelateMessageRequest {
    pub message_name: String,
    pub business_key: Option<String>,
    pub variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CorrelateMessageResponse {
    pub affected_instances: Vec<String>,
}

pub(crate) async fn correlate_message(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CorrelateMessageRequest>,
) -> Result<Json<CorrelateMessageResponse>, AppError> {
    let engine = &state.engine;
    let vars = payload.variables.unwrap_or_default();
    let affected = engine
        .correlate_message(payload.message_name, payload.business_key, vars)
        .await?;
    let affected_strs = affected.into_iter().map(|id| id.to_string()).collect();
    Ok(Json(CorrelateMessageResponse {
        affected_instances: affected_strs,
    }))
}

/// Returns all currently pending message catch events.
pub(crate) async fn get_pending_messages(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PendingMessageCatch>> {
    Json(state.engine.get_pending_message_catches())
}
