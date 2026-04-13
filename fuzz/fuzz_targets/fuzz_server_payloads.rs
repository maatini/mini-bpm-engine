//! Fuzz target: Server DTO payloads
//!
//! Fuzzes `serde_json::from_slice` deserialization of all REST API DTOs from
//! `engine-server`. Since the DTOs are `pub(crate)`, they are replicated here
//! with the exact same field names and serde attributes.
//!
//! This ensures the server gracefully rejects malformed inputs without panicking.

#![no_main]
use libfuzzer_sys::fuzz_target;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// tasks.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopicRequest {
    topic_name: String,
    lock_duration: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAndLockRequest {
    worker_id: String,
    max_tasks: usize,
    topics: Vec<TopicRequest>,
    async_response_timeout: Option<u64>,
}

#[derive(Deserialize)]
struct CompleteRequest {
    variables: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteServiceTaskRequest {
    worker_id: String,
    variables: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FailServiceTaskRequest {
    worker_id: String,
    retries: Option<i32>,
    error_message: Option<String>,
    error_details: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetryIncidentRequest {
    retries: Option<i32>,
}

#[derive(Deserialize)]
struct ResolveIncidentRequest {
    variables: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtendLockRequest {
    worker_id: String,
    new_duration: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BpmnErrorRequest {
    worker_id: String,
    error_code: String,
}

// ---------------------------------------------------------------------------
// instances.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct StartRequest {
    definition_key: String,
    #[serde(default)]
    variables: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct StartLatestRequest {
    bpmn_id: String,
    #[serde(default)]
    variables: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct UpdateVariablesRequest {
    variables: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct MoveTokenRequest {
    target_node_id: String,
    #[serde(default)]
    variables: Option<HashMap<String, Value>>,
    #[serde(default = "default_true")]
    cancel_current: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// deploy.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeployRequest {
    xml: String,
    name: String,
}

#[derive(Deserialize)]
struct DeleteDefinitionQuery {
    cascade: Option<bool>,
}

// ---------------------------------------------------------------------------
// messages.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrelateMessageRequest {
    message_name: String,
    business_key: Option<String>,
    variables: Option<HashMap<String, Value>>,
}

// ---------------------------------------------------------------------------
// history.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ServerHistoryQuery {
    event_types: Option<String>,
    node_id: Option<String>,
    actor_type: Option<String>,
    from: Option<String>, // chrono::DateTime in real code, String here for fuzzing
    to: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Deserialize, Default)]
struct CompletedInstancesQuery {
    definition_key: Option<String>,
    business_key: Option<String>,
    from: Option<String>,
    to: Option<String>,
    state: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// monitoring.rs DTOs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BucketEntriesQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Fuzz target
// ---------------------------------------------------------------------------

fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 1024 * 64 {
        return;
    }

    // Service Task DTOs (camelCase)
    let _ = serde_json::from_slice::<FetchAndLockRequest>(data);
    let _ = serde_json::from_slice::<CompleteServiceTaskRequest>(data);
    let _ = serde_json::from_slice::<FailServiceTaskRequest>(data);
    let _ = serde_json::from_slice::<RetryIncidentRequest>(data);
    let _ = serde_json::from_slice::<ExtendLockRequest>(data);
    let _ = serde_json::from_slice::<BpmnErrorRequest>(data);

    // User Task DTOs
    let _ = serde_json::from_slice::<CompleteRequest>(data);
    let _ = serde_json::from_slice::<ResolveIncidentRequest>(data);

    // Instance DTOs
    let _ = serde_json::from_slice::<StartRequest>(data);
    let _ = serde_json::from_slice::<StartLatestRequest>(data);
    let _ = serde_json::from_slice::<UpdateVariablesRequest>(data);
    let _ = serde_json::from_slice::<MoveTokenRequest>(data);

    // Deploy DTOs
    let _ = serde_json::from_slice::<DeployRequest>(data);
    let _ = serde_json::from_slice::<DeleteDefinitionQuery>(data);

    // Message DTOs
    let _ = serde_json::from_slice::<CorrelateMessageRequest>(data);

    // History Query DTOs
    let _ = serde_json::from_slice::<ServerHistoryQuery>(data);
    let _ = serde_json::from_slice::<CompletedInstancesQuery>(data);

    // Monitoring DTOs
    let _ = serde_json::from_slice::<BucketEntriesQuery>(data);
});
