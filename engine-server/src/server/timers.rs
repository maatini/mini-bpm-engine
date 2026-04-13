use crate::server::state::{AppError, AppState};
use axum::{Json, extract::State};
use engine_core::runtime::PendingTimer;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessTimersResponse {
    pub triggered: usize,
}

pub(crate) async fn process_timers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessTimersResponse>, AppError> {
    let engine = &state.engine;
    let count = engine.process_timers().await?;
    Ok(Json(ProcessTimersResponse { triggered: count }))
}

/// Returns all currently pending timers.
pub(crate) async fn get_pending_timers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PendingTimer>> {
    Json(state.engine.get_pending_timers())
}
