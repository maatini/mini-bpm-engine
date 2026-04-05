use tokio::sync::RwLock;
pub(crate) mod deploy;
pub(crate) mod files;
pub(crate) mod history;
pub(crate) mod instances;
pub(crate) mod messages;
pub(crate) mod monitoring;
pub(crate) mod state;
pub(crate) mod tasks;
pub(crate) mod timers;

use axum::{
    Router,
    http::Method,
    routing::{delete, get, post, put},
};
use engine_core::engine::WorkflowEngine;
use engine_core::persistence::WorkflowPersistence;
use state::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Builds the Axum router with all routes and middleware.
///
/// Exposed as `pub` so integration tests can create the app without
/// starting a full server binary.
pub fn build_app() -> Router {
    build_app_with_engine(Arc::new(WorkflowEngine::new()), None, HashMap::new())
}

pub fn build_app_with_engine(
    engine: Arc<WorkflowEngine>,
    persistence: Option<Arc<dyn WorkflowPersistence>>,
    xml_cache: HashMap<String, String>,
) -> Router {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let state = Arc::new(AppState {
        engine,
        persistence,
        deployed_xml: Arc::new(RwLock::new(xml_cache)),
        nats_url,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    Router::new()
        .route("/api/deploy", post(deploy::deploy_definition))
        .route("/api/start", post(instances::start_instance))
        .route("/api/start/latest", post(instances::start_instance_latest))
        .route("/api/tasks", get(tasks::get_tasks))
        .route("/api/complete/:id", post(tasks::complete_task))
        .route("/api/instances", get(instances::list_instances))
        .route(
            "/api/instances/:id",
            get(instances::get_instance).delete(instances::delete_instance),
        )
        .route("/api/definitions", get(deploy::list_definitions))
        .route("/api/definitions/:id/xml", get(deploy::get_definition_xml))
        .route("/api/definitions/:id", delete(deploy::delete_definition))
        .route(
            "/api/definitions/bpmn/:bpmn_id",
            delete(deploy::delete_all_definitions),
        )
        .route(
            "/api/instances/:id/variables",
            put(instances::update_instance_variables),
        )
        .route(
            "/api/instances/:id/files/:var_name",
            post(files::upload_instance_file)
                .get(files::get_instance_file)
                .delete(files::delete_instance_file),
        )
        .route(
            "/api/instances/:id/history",
            get(history::get_instance_history),
        )
        .route(
            "/api/instances/:id/history/:event_id",
            get(history::get_instance_history_entry),
        )
        .route("/api/info", get(monitoring::get_backend_info))
        .route("/api/monitoring", get(monitoring::get_monitoring_data))
        .route(
            "/api/monitoring/buckets/:bucket/entries",
            get(monitoring::get_bucket_entries),
        )
        .route(
            "/api/monitoring/buckets/:bucket/entries/:key",
            get(monitoring::get_bucket_entry_detail),
        )
        // Phase 1 endpoints
        .route("/api/message", post(messages::correlate_message))
        .route("/api/timers/process", post(timers::process_timers))
        // Service Task endpoints
        .route("/api/service-tasks", get(tasks::get_service_tasks))
        .route(
            "/api/service-task/fetchAndLock",
            post(tasks::fetch_and_lock_service_tasks),
        )
        .route(
            "/api/service-task/:id/complete",
            post(tasks::complete_service_task),
        )
        .route(
            "/api/service-task/:id/failure",
            post(tasks::fail_service_task),
        )
        .route("/api/service-task/:id/extendLock", post(tasks::extend_lock))
        .route("/api/service-task/:id/bpmnError", post(tasks::bpmn_error))
        .route("/api/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/api/ready", get(monitoring::ready_endpoint))
        .layer(axum::extract::DefaultBodyLimit::max(5 * 1024 * 1024))
        .layer(cors)
        .with_state(state)
}
