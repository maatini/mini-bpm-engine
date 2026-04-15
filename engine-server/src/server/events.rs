//! Server-Sent Events (SSE) endpoint for push-based engine state notifications.
//!
//! Clients connect to `GET /api/events` and receive a stream of events whenever
//! the engine changes state (instance changed, task created/completed, definition deployed).

use axum::{
    extract::State,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use engine_core::engine::EngineEvent;
use tokio_stream::Stream;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use super::state::AppState;

/// SSE handler — each connecting client gets its own broadcast receiver.
///
/// The event `data` field contains the JSON-serialized `EngineEvent` type tag.
/// Clients filter by `type` to decide which data to refresh.
pub async fn engine_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.engine.subscribe_events();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let event_type = match &event {
                    EngineEvent::InstanceChanged => "instance_changed",
                    EngineEvent::TaskChanged => "task_changed",
                    EngineEvent::DefinitionChanged => "definition_changed",
                };
                Some(Ok(Event::default().event(event_type).data("")))
            }
            // Lagged receiver — subscriber was too slow, skip
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
