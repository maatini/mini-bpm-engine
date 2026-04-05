use crate::server::state::{AppError, AppState};
use axum::{Json, extract::State};
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
