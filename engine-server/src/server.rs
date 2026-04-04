use axum::{
    extract::{Path, State, Multipart},
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
    pending_timers: usize,
    pending_message_catches: usize,
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
    version: i32,
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
        .route("/api/start/latest", post(start_instance_latest))
        .route("/api/tasks", get(get_tasks))
        .route("/api/complete/:id", post(complete_task))
        .route("/api/instances", get(list_instances))
        .route("/api/instances/:id", get(get_instance).delete(delete_instance))
        .route("/api/definitions", get(list_definitions))
        .route("/api/definitions/:id/xml", get(get_definition_xml))
        .route("/api/definitions/:id", delete(delete_definition))
        .route("/api/instances/:id/variables", put(update_instance_variables))
        .route("/api/instances/:id/files/:var_name",
            post(upload_instance_file)
            .get(get_instance_file)
            .delete(delete_instance_file)
        )
        .route("/api/instances/:id/history", get(get_instance_history))
        .route("/api/instances/:id/history/:event_id", get(get_instance_history_entry))
        .route("/api/info", get(get_backend_info))
        .route("/api/monitoring", get(get_monitoring_data))
        // Phase 1 endpoints
        .route("/api/message", post(correlate_message))
        .route("/api/timers/process", post(process_timers))
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
    let (key, version) = engine.deploy_definition(def).await;
    let key_str = key.to_string();

    if let Some(persistence) = &state.persistence {
        if let Err(e) = persistence.save_bpmn_xml(&key_str, &payload.xml).await {
            log::error!("Failed to save BPMN XML to persistence layer: {:?}", e);
        }
    }
    state.deployed_xml.write().await.insert(key_str.clone(), payload.xml.clone());

    Ok(Json(DeployResponse { definition_key: key_str, version }))
}

#[derive(Serialize)]
struct DefinitionInfo {
    key: String,
    bpmn_id: String,
    version: i32,
    node_count: usize,
    is_latest: bool,
}

async fn list_definitions(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DefinitionInfo>> {
    let engine = state.engine.read().await;
    let raw = engine.list_definitions().await;

    // Determine the latest version per bpmn_id
    let mut latest_versions: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
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

#[derive(Serialize, Deserialize)]
struct StartLatestRequest {
    bpmn_id: String,
    #[serde(default)]
    variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
struct StartLatestResponse {
    instance_id: String,
    definition_key: String,
    version: i32,
}

async fn start_instance_latest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartLatestRequest>,
) -> Result<Json<StartLatestResponse>, AppError> {
    let mut engine = state.engine.write().await;
    let vars = payload.variables.unwrap_or_default();
    let (inst_id, def_key) = engine.start_instance_latest(&payload.bpmn_id, vars).await?;

    let def = engine.get_definition(&def_key).await
        .ok_or_else(|| AppError::BadRequest("Definition not found".into()))?;
    let version = def.version;

    Ok(Json(StartLatestResponse {
        instance_id: inst_id.to_string(),
        definition_key: def_key.to_string(),
        version,
    }))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrelateMessageRequest {
    message_name: String,
    business_key: Option<String>,
    variables: Option<HashMap<String, Value>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrelateMessageResponse {
    affected_instances: Vec<String>,
}

async fn correlate_message(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CorrelateMessageRequest>,
) -> Result<Json<CorrelateMessageResponse>, AppError> {
    let mut engine = state.engine.write().await;
    let vars = payload.variables.unwrap_or_default();
    let affected = engine.correlate_message(payload.message_name, payload.business_key, vars).await?;
    let affected_strs = affected.into_iter().map(|id| id.to_string()).collect();
    Ok(Json(CorrelateMessageResponse { affected_instances: affected_strs }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessTimersResponse {
    triggered: usize,
}

async fn process_timers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessTimersResponse>, AppError> {
    let mut engine = state.engine.write().await;
    let count = engine.process_timers().await?;
    Ok(Json(ProcessTimersResponse { triggered: count }))
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
    let instances = engine.list_instances().await;
    Json(instances)
}

async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProcessInstance>, AppError> {
    let engine = state.engine.read().await;
    let instance_id = parse_uuid(&id)?;

    let instance = engine.get_instance_details(instance_id).await?;

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

    engine.update_instance_variables(instance_id, payload.variables).await?;

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

    let poll_interval = tokio::time::Duration::from_millis(500);
    let max_timeout = tokio::time::Duration::from_millis(timeout_ms.min(30_000)); // Cap at 30s

    // Simple long-polling: retry until tasks found or timeout
    let start = tokio::time::Instant::now();
    loop {
        // Acquire lock, do work, release lock — all in one scope
        let tasks = {
            let mut engine = state.engine.write().await;
            engine.fetch_and_lock_service_tasks(
                &payload.worker_id,
                payload.max_tasks,
                &topics,
                lock_duration,
            ).await
        }; // ← Lock is released here BEFORE the sleep

        if !tasks.is_empty() || timeout_ms == 0 {
            return Json(tasks);
        }

        if start.elapsed() >= max_timeout {
            return Json(vec![]);
        }

        tokio::time::sleep(poll_interval).await;
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

#[derive(Deserialize, Default)]
struct ServerHistoryQuery {
    event_types: Option<String>,
    node_id: Option<String>,
    actor_type: Option<String>,
    from: Option<chrono::DateTime<chrono::Utc>>,
    to: Option<chrono::DateTime<chrono::Utc>>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn get_instance_history(
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
        
        let parsed_actor_type = query.actor_type.and_then(|s| {
             serde_json::from_value(serde_json::json!(s.trim())).ok()
        });

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
        
        let history = p.query_history(history_query).await.map_err(AppError::from)?;
            
        Ok(Json(history))
    } else {
        Ok(Json(vec![]))
    }
}

async fn get_instance_history_entry(
    State(state): State<Arc<AppState>>,
    Path((id, event_id)): Path<(String, String)>,
) -> Result<Json<engine_core::history::HistoryEntry>, AppError> {
    let instance_id = parse_uuid(&id)?;
    let event_uuid = parse_uuid(&event_id)?;
    
    if let Some(p) = &state.persistence {
        let history = p.query_history(engine_core::persistence::HistoryQuery {
            instance_id,
            ..Default::default()
        }).await.map_err(AppError::from)?;
        if let Some(entry) = history.into_iter().find(|e| e.id == event_uuid) {
            Ok(Json(entry))
        } else {
            Err(AppError::BadRequest(format!("History entry {event_uuid} not found")))
        }
    } else {
        Err(AppError::BadRequest("No persistence configured".to_string()))
    }
}

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

    let stats = engine.get_stats().await;

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
        pending_timers: stats.pending_timers,
        pending_message_catches: stats.pending_message_catches,
        storage_info,
    })
}

async fn upload_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let instance_id = parse_uuid(&id)?;
    if engine.get_instance_details(instance_id).await.is_err() {
        return Err(AppError::BadRequest("Instance not found".into()));
    }
    
    if let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let filename = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
        
        let file_ref = engine_core::model::FileReference::new(
            instance_id,
            &var_name,
            &filename,
            &content_type,
            data.len() as u64,
        );

        if let Some(persistence) = &state.persistence {
            persistence.save_file(&file_ref.object_key, &data).await?;
        }
        
        let mut vars = HashMap::new();
        vars.insert(var_name, file_ref.to_variable_value());
        engine.update_instance_variables(instance_id, vars).await?;
        
        Ok(StatusCode::CREATED)
    } else {
        Err(AppError::BadRequest("No file field provided".into()))
    }
}

async fn get_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let engine = state.engine.read().await;
    let instance_id = parse_uuid(&id)?;
    let instance = engine.get_instance_details(instance_id).await?;
    
    let file_ref = instance.get_file_reference(&var_name)
        .ok_or_else(|| AppError::BadRequest("Variable is not a file".into()))?;

    if let Some(persistence) = &state.persistence {
        let data = persistence.load_file(&file_ref.object_key).await?;
        
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            file_ref.mime_type.parse().unwrap_or(axum::http::HeaderValue::from_static("application/octet-stream"))
        );
        headers.insert(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_ref.filename).parse().unwrap_or(axum::http::HeaderValue::from_static("attachment"))
        );
        
        Ok((headers, data))
    } else {
        Err(AppError::BadRequest("No persistence configured".into()))
    }
}

async fn delete_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut engine = state.engine.write().await;
    let instance_id = parse_uuid(&id)?;
    let instance = engine.get_instance_details(instance_id).await?;
    
    let file_ref = instance.get_file_reference(&var_name)
        .ok_or_else(|| AppError::BadRequest("Variable is not a file".into()))?;

    if let Some(persistence) = &state.persistence {
        persistence.delete_file(&file_ref.object_key).await?;
    }
    
    let mut vars = HashMap::new();
    vars.insert(var_name, Value::Null);
    engine.update_instance_variables(instance_id, vars).await?;
    
    Ok(StatusCode::NO_CONTENT)
}
