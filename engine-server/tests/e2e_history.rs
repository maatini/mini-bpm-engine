//! E2E test: Verify that history logs are correctly appended, querying by instance ID
//! and by event ID.

use serde_json::Value;
use std::sync::Arc;


/// Minimal BPMN 2.0 XML with only a StartEvent, EndEvent, and one SequenceFlow.
const MINIMAL_BPMN_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="HistoryProcess">
    <startEvent id="Start_1" />
    <endEvent id="End_1" />
    <sequenceFlow id="Flow_1" sourceRef="Start_1" targetRef="End_1" />
  </process>
</definitions>"#;

/// Helper: starts the engine-server on a random port with NATS persistence 
/// enabled, returning the auto-assigned base URL.
async fn start_server_with_nats() -> Option<String> {
    let url = "nats://localhost:4222";
    let stream = format!("TEST_HISTORY_{}", uuid::Uuid::new_v4());

    let persistence = match persistence_nats::NatsPersistence::connect(url, &stream).await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tracing::warn!("Skipping NATS E2E history test, could not connect: {}", e);
            return None;
        }
    };

    let engine = engine_core::engine::WorkflowEngine::new().with_persistence(persistence.clone());

    let app = engine_server::build_app_with_engine(
        Arc::new(engine),
        Some(persistence),
        std::collections::HashMap::new(),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to random port");
    let addr = listener.local_addr().expect("failed to get local addr");
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });

    Some(base_url)
}

#[tokio::test]
async fn verify_instance_history_is_generated_and_retrieved() {
    let base = match start_server_with_nats().await {
        Some(b) => b,
        None => return, // Ignore if NATS container is not running
    };
    
    let client = reqwest::Client::new();

    // Step 1: Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "HistoryProcess Deploy"
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(deploy_res.status(), 200);
    let deploy_body: Value = deploy_res.json().await.expect("parse deploy response");
    let def_key = deploy_body["definition_key"]
        .as_str()
        .expect("definition_key missing")
        .to_string();

    // Step 2: Start instance
    let start_res = client
        .post(format!("{}/api/start", base))
        .json(&serde_json::json!({ "definition_key": def_key }))
        .send()
        .await
        .expect("start request failed");

    assert_eq!(start_res.status(), 200, "start should return 200 OK");

    let start_body: Value = start_res.json().await.expect("parse start response");
    let instance_id = start_body["instance_id"]
        .as_str()
        .expect("instance_id missing")
        .to_string();

    // Wait a brief moment for JetStream indices to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // Step 3: Fetch history
    let history_res = client
        .get(format!("{}/api/instances/{}/history", base, instance_id))
        .send()
        .await
        .expect("get history request failed");

    assert_eq!(history_res.status(), 200, "get history should return 200");

    let history: Vec<Value> = history_res.json().await.expect("parse history response");
    
    // We expect at least the StartEvent, Token movement, and Process Completed events.
    assert!(!history.is_empty(), "history should not be empty");
    
    let event_types: Vec<&str> = history.iter().filter_map(|e| e["event_type"].as_str()).collect();
    
    assert!(event_types.contains(&"InstanceStarted"), "should contain InstanceStarted");
    assert!(event_types.contains(&"TokenAdvanced"), "should contain TokenAdvanced");
    assert!(event_types.contains(&"InstanceCompleted"), "should contain InstanceCompleted");

    // Fetch a single specific event payload as well
    let first_event_id = history[0]["id"].as_str().expect("first event ID");

    let specific_event_res = client
        .get(format!("{}/api/instances/{}/history/{}", base, instance_id, first_event_id))
        .send()
        .await
        .expect("get specific event request failed");

    assert_eq!(specific_event_res.status(), 200, "get specific event should return 200");
    let specific_event: Value = specific_event_res.json().await.expect("parse specific event");

    assert_eq!(specific_event["id"].as_str(), Some(first_event_id));
    assert_eq!(specific_event["instance_id"].as_str(), Some(instance_id.as_str()));
}
