use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post, put, delete},
    Json, Router,
};
use engine_core::engine::{PendingServiceTask, PendingUserTask, ProcessInstance, WorkflowEngine};
use engine_core::error::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;
use engine_core::persistence::{StorageInfo, WorkflowPersistence};

// ---------------------------------------------------------------------------
// Centralized error type – maps EngineError to proper HTTP status codes
// ---------------------------------------------------------------------------

/// Unified error type for all REST handlers.
enum AppError {
    /// Wraps an `EngineError` with automatic status-code mapping.
    Engine(EngineError),
    /// Client sent a malformed request (invalid UUID, bad XML, etc.).
    BadRequest(String),
}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        Self::Engine(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            // Client errors (400)
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Engine(EngineError::InvalidDefinition(msg)) => {
                (StatusCode::BAD_REQUEST, msg)
            }
            Self::Engine(EngineError::NoMatchingCondition(msg)) => {
                (StatusCode::BAD_REQUEST, format!("No matching condition at gateway '{msg}'"))
            }
            // Not-found errors (404)
            Self::Engine(EngineError::NoSuchDefinition(id)) => {
                (StatusCode::NOT_FOUND, format!("Definition not found: {id}"))
            }
            Self::Engine(EngineError::NoSuchInstance(id)) => {
                (StatusCode::NOT_FOUND, format!("Instance not found: {id}"))
            }
            Self::Engine(EngineError::NoSuchNode(id)) => {
                (StatusCode::NOT_FOUND, format!("Node not found: {id}"))
            }
            Self::Engine(EngineError::ServiceTaskNotFound(id)) => {
                (StatusCode::NOT_FOUND, format!("Service task not found: {id}"))
            }
            // Conflict errors (409)
            Self::Engine(EngineError::TaskNotPending { task_id, actual_state }) => {
                (StatusCode::CONFLICT, format!("Task '{task_id}' is not pending (state: {actual_state})"))
            }
            Self::Engine(EngineError::ServiceTaskLocked { task_id, worker_id }) => {
                (StatusCode::CONFLICT, format!("Task '{task_id}' locked by worker '{worker_id}'"))
            }
            Self::Engine(EngineError::ServiceTaskNotLocked(id)) => {
                (StatusCode::CONFLICT, format!("Service task '{id}' is not locked"))
            }
            Self::Engine(EngineError::AlreadyCompleted) => {
                (StatusCode::CONFLICT, "Process instance already completed".to_string())
            }
            // Everything else → 500
            Self::Engine(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

/// Parse a UUID from a path segment, returning `AppError::BadRequest` on failure.
fn parse_uuid(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::BadRequest("Invalid UUID format".to_string()))
}

// ---------------------------------------------------------------------------
// State & request/response types
// ---------------------------------------------------------------------------

struct AppState {
    engine: Arc<RwLock<WorkflowEngine>>,
    persistence: Option<Arc<dyn WorkflowPersistence>>,
    deployed_xml: Arc<RwLock<HashMap<String, String>>>,
    nats_url: String, // Store URL for /api/info
}

#[derive(Serialize)]
struct BackendInfo {
    backend_type: String,
    nats_url: Option<String>,
    connected: bool,
}



#[derive(Serialize)]
struct MonitoringData {
    definitions_count: usize,
    instances_total: usize,
    instances_running: usize,
    instances_completed: usize,
    pending_user_tasks: usize,
    pending_service_tasks: usize,
    storage_info: Option<StorageInfo>,
}

#[derive(Serialize, Deserialize)]
struct DeployRequest {
    xml: String,
    name: String,
}

#[derive(Serialize)]
struct DeployResponse {
    definition_key: String,
}

#[derive(Serialize, Deserialize)]
struct StartRequest {
    definition_key: String,
    #[serde(default)]
    variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
struct StartResponse {
    instance_id: String,
}

#[derive(Serialize, Deserialize)]
struct CompleteRequest {
    variables: Option<HashMap<String, Value>>,
}

// ---------------------------------------------------------------------------
// Service Task request/response types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopicRequest {
    topic_name: String,
    lock_duration: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAndLockRequest {
    worker_id: String,
    max_tasks: usize,
    topics: Vec<TopicRequest>,
    /// Optional timeout for long-polling (milliseconds).
    async_response_timeout: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteServiceTaskRequest {
    worker_id: String,
    variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FailServiceTaskRequest {
    worker_id: String,
    retries: Option<i32>,
    error_message: Option<String>,
    error_details: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtendLockRequest {
    worker_id: String,
    new_duration: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BpmnErrorRequest {
    worker_id: String,
    error_code: String,
}

/// Builds the Axum router with all routes and middleware.
///
/// Exposed as `pub` so integration tests can create the app without
/// starting a full server binary.
pub fn build_app() -> Router {
    build_app_with_engine(Arc::new(RwLock::new(WorkflowEngine::new())), None, HashMap::new())
}

pub fn build_app_with_engine(
    engine: Arc<RwLock<WorkflowEngine>>,
    persistence: Option<Arc<dyn WorkflowPersistence>>,
    xml_cache: HashMap<String, String>,
) -> Router {
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    
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
        .route("/api/deploy", post(deploy_definition))
        .route("/api/start", post(start_instance))
        .route("/api/tasks", get(get_tasks))
        .route("/api/complete/:id", post(complete_task))
        .route("/api/instances", get(list_instances))
        .route("/api/instances/:id", get(get_instance).delete(delete_instance))
        .route("/api/definitions", get(list_definitions))
        .route("/api/definitions/:id/xml", get(get_definition_xml))
        .route("/api/definitions/:id", delete(delete_definition))
        .route("/api/instances/:id/variables", put(update_instance_variables))
        .route("/api/info", get(get_backend_info))
        .route("/api/monitoring", get(get_monitoring_data))
        // Service Task endpoints
        .route("/api/service-tasks", get(get_service_tasks))
        .route("/api/service-task/fetchAndLock", post(fetch_and_lock_service_tasks))
        .route("/api/service-task/:id/complete", post(complete_service_task))
        .route("/api/service-task/:id/failure", post(fail_service_task))
        .route("/api/service-task/:id/extendLock", post(extend_lock))
        .route("/api/service-task/:id/bpmnError", post(bpmn_error))
        .layer(cors)
        .with_state(state)
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn deploy_definition(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, AppError> {
    let mut engine = state.engine.write().await;

    let def = bpmn_parser::parse_bpmn_xml(&payload.xml)
        .map_err(|e| AppError::BadRequest(format!("Invalid BPMN XML: {e:?}")))?;

    let key = engine.deploy_definition(def).await;
    let key_str = key.to_string();

    if let Some(persistence) = &state.persistence {
        if let Err(e) = persistence.save_bpmn_xml(&key_str, &payload.xml).await {
            log::error!("Failed to save BPMN XML to persistence layer: {:?}", e);
        }
    }
    state.deployed_xml.write().await.insert(key_str.clone(), payload.xml.clone());

    Ok(Json(DeployResponse { definition_key: key_str }))
}

#[derive(Serialize)]
struct DefinitionInfo {
    key: String,
    bpmn_id: String,
    node_count: usize,
}

async fn list_definitions(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DefinitionInfo>> {
    let engine = state.engine.read().await;
    let defs: Vec<DefinitionInfo> = engine
        .list_definitions()
        .into_iter()
        .map(|(key, bpmn_id, node_count)| DefinitionInfo {
            key: key.to_string(),
            bpmn_id,
            node_count,
        })
        .collect();
    Json(defs)
}

async fn get_definition_xml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<String, AppError> {
    {
        let xml_store = state.deployed_xml.read().await;
        if let Some(xml) = xml_store.get(&id) {
            return Ok(xml.clone());
        }
    }

    if let Some(persistence) = &state.persistence {
        if let Ok(xml) = persistence.load_bpmn_xml(&id).await {
            return Ok(xml);
        }
    }

    Err(AppError::BadRequest(format!("No XML found for definition '{id}'")))
}

async fn start_instance(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartRequest>,
) -> Result<Json<StartResponse>, AppError> {
    let mut engine = state.engine.write().await;
    let def_key = parse_uuid(&payload.definition_key)?;
    let id = match payload.variables {
        Some(vars) if !vars.is_empty() => {
            engine
                .start_instance_with_variables(def_key, vars)
                .await
        }
        _ => engine.start_instance(def_key).await,
    }?;

    Ok(Json(StartResponse { instance_id: id.to_string() }))
}

async fn get_tasks(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PendingUserTask>> {
    let engine = state.engine.read().await;
    let tasks = engine.get_pending_user_tasks().to_vec();
    Json(tasks)
}

async fn get_service_tasks(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PendingServiceTask>> {
    let engine = state.engine.read().await;
    let tasks = engine.get_pending_service_tasks().to_vec();
    Json(tasks)
}

async fn complete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<CompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let task_id = parse_uuid(&id)?;
    let vars = payload.variables.unwrap_or_default();

    engine.complete_user_task(task_id, vars).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_instances(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProcessInstance>> {
    let engine = state.engine.read().await;
    let instances = engine.list_instances();
    Json(instances)
}

async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProcessInstance>, AppError> {
    let engine = state.engine.read().await;
    let instance_id = parse_uuid(&id)?;

    let instance = engine.get_instance_details(instance_id)?;

    Ok(Json(instance))
}

#[derive(Deserialize)]
struct UpdateVariablesRequest {
    variables: HashMap<String, Value>,
}

async fn update_instance_variables(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateVariablesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let instance_id = parse_uuid(&id)?;

    engine.update_instance_variables(instance_id, payload.variables)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let instance_id = parse_uuid(&id)?;

    engine.delete_instance(instance_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DeleteDefinitionQuery {
    cascade: Option<bool>,
}

async fn delete_definition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<DeleteDefinitionQuery>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let def_key = parse_uuid(&id)?;

    engine.delete_definition(def_key, query.cascade.unwrap_or(false)).await?;

    state.deployed_xml.write().await.remove(&id);

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Service Task REST handlers
// ---------------------------------------------------------------------------

/// POST /api/service-task/fetchAndLock
///
/// Long-polling variant: if `asyncResponseTimeout` is set, retries up to that
/// duration (polling every 500ms).
async fn fetch_and_lock_service_tasks(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FetchAndLockRequest>,
) -> Json<Vec<PendingServiceTask>> {
    let topics: Vec<String> = payload.topics.iter().map(|t| t.topic_name.clone()).collect();
    // Use the first topic's lock duration, or default 30s
    let lock_duration = payload.topics.first().map(|t| t.lock_duration).unwrap_or(30);
    let timeout_ms = payload.async_response_timeout.unwrap_or(0);

    // Simple long-polling: retry until tasks found or timeout
    let start = tokio::time::Instant::now();
    loop {
        let mut engine = state.engine.write().await;
        let tasks = engine.fetch_and_lock_service_tasks(
            &payload.worker_id,
            payload.max_tasks,
            &topics,
            lock_duration,
        ).await;

        if !tasks.is_empty() || timeout_ms == 0 {
            return Json(tasks);
        }

        // Release lock before sleeping
        drop(engine);

        if start.elapsed().as_millis() as u64 >= timeout_ms {
            return Json(vec![]);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// POST /api/service-task/:id/complete
async fn complete_service_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<CompleteServiceTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let task_id = parse_uuid(&id)?;
    let vars = payload.variables.unwrap_or_default();

    engine
        .complete_service_task(task_id, &payload.worker_id, vars)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/service-task/:id/failure
async fn fail_service_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<FailServiceTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let task_id = parse_uuid(&id)?;

    engine.fail_service_task(
        task_id,
        &payload.worker_id,
        payload.retries,
        payload.error_message,
        payload.error_details,
    ).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/service-task/:id/extendLock
async fn extend_lock(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<ExtendLockRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let task_id = parse_uuid(&id)?;

    engine.extend_lock(task_id, &payload.worker_id, payload.new_duration).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/service-task/:id/bpmnError
async fn bpmn_error(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(payload): Json<BpmnErrorRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let task_id = parse_uuid(&id)?;

    engine.handle_bpmn_error(task_id, &payload.worker_id, &payload.error_code).await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Info & Monitoring
// ---------------------------------------------------------------------------

async fn get_backend_info(
    State(state): State<Arc<AppState>>,
) -> Json<BackendInfo> {
    if let Some(ref p) = state.persistence {
        let info = p.get_storage_info().await.ok().flatten();
        Json(BackendInfo {
            backend_type: "persistent".to_string(),
            nats_url: info.as_ref().map(|i| format!("{}:{}", i.host, i.port)),
            connected: true,
        })
    } else {
        Json(BackendInfo {
            backend_type: "in-memory".to_string(),
            nats_url: Some(state.nats_url.clone()),
            connected: false,
        })
    }
}

async fn get_monitoring_data(
    State(state): State<Arc<AppState>>,
) -> Json<MonitoringData> {
    let engine = state.engine.read().await;

    let stats = engine.get_stats();

    let storage_info = if let Some(ref persistence) = state.persistence {
        persistence.get_storage_info().await.unwrap_or(None)
    } else {
        None
    };

    Json(MonitoringData {
        definitions_count: stats.definitions_count,
        instances_total: stats.instances_total,
        instances_running: stats.instances_running + stats.instances_waiting_user + stats.instances_waiting_service,
        instances_completed: stats.instances_completed,
        pending_user_tasks: stats.pending_user_tasks,
        pending_service_tasks: stats.pending_service_tasks,
        storage_info,
    })
}
