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
        None,
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

    let event_types: Vec<&str> = history
        .iter()
        .filter_map(|e| e["event_type"].as_str())
        .collect();

    assert!(
        event_types.contains(&"InstanceStarted"),
        "should contain InstanceStarted"
    );
    assert!(
        event_types.contains(&"TokenAdvanced"),
        "should contain TokenAdvanced"
    );
    assert!(
        event_types.contains(&"InstanceCompleted"),
        "should contain InstanceCompleted"
    );

    // Fetch a single specific event payload as well
    let first_event_id = history[0]["id"].as_str().expect("first event ID");

    let specific_event_res = client
        .get(format!(
            "{}/api/instances/{}/history/{}",
            base, instance_id, first_event_id
        ))
        .send()
        .await
        .expect("get specific event request failed");

    assert_eq!(
        specific_event_res.status(),
        200,
        "get specific event should return 200"
    );
    let specific_event: Value = specific_event_res
        .json()
        .await
        .expect("parse specific event");

    assert_eq!(specific_event["id"].as_str(), Some(first_event_id));
    assert_eq!(
        specific_event["instance_id"].as_str(),
        Some(instance_id.as_str())
    );
}

#[tokio::test]
async fn test_completed_instance_appears_in_history_instances() {
    let base = match start_server_with_nats().await {
        Some(b) => b,
        None => return,
    };

    let client = reqwest::Client::new();

    // Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "HistoryInstancesTest"
        }))
        .send()
        .await
        .expect("deploy failed");
    assert_eq!(deploy_res.status(), 200);
    let deploy_body: Value = deploy_res.json().await.unwrap();
    let def_key = deploy_body["definition_key"].as_str().unwrap().to_string();

    // Start instance (completes immediately since Start→End)
    let start_res = client
        .post(format!("{}/api/start", base))
        .json(&serde_json::json!({
            "definition_key": def_key,
            "business_key": "order-42"
        }))
        .send()
        .await
        .expect("start failed");
    assert_eq!(start_res.status(), 200);
    let start_body: Value = start_res.json().await.unwrap();
    let instance_id = start_body["instance_id"].as_str().unwrap().to_string();

    // Wait for archival to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Query completed instances — should find our instance
    let list_res = client
        .get(format!("{}/api/history/instances", base))
        .send()
        .await
        .expect("list completed failed");
    assert_eq!(list_res.status(), 200);
    let list: Vec<Value> = list_res.json().await.unwrap();
    assert!(
        list.iter()
            .any(|i| i["id"].as_str() == Some(instance_id.as_str())),
        "archived instance should appear in history/instances"
    );

    // Get single completed instance
    let get_res = client
        .get(format!("{}/api/history/instances/{}", base, instance_id))
        .send()
        .await
        .expect("get completed instance failed");
    assert_eq!(get_res.status(), 200);
    let inst: Value = get_res.json().await.unwrap();
    assert_eq!(inst["id"].as_str(), Some(instance_id.as_str()));
    assert!(
        inst["completed_at"].as_str().is_some(),
        "completed_at should be set"
    );
    assert!(
        inst["started_at"].as_str().is_some(),
        "started_at should be set"
    );
}

#[tokio::test]
async fn test_history_instances_filter_by_business_key() {
    let base = match start_server_with_nats().await {
        Some(b) => b,
        None => return,
    };

    let client = reqwest::Client::new();

    // Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "FilterTest"
        }))
        .send()
        .await
        .unwrap();
    let deploy_body: Value = deploy_res.json().await.unwrap();
    let def_key = deploy_body["definition_key"].as_str().unwrap().to_string();

    // Start two instances with different business keys
    for bk in ["alpha-123", "beta-456"] {
        client
            .post(format!("{}/api/start", base))
            .json(&serde_json::json!({
                "definition_key": def_key,
                "business_key": bk
            }))
            .send()
            .await
            .unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Filter by business_key=alpha
    let res = client
        .get(format!("{}/api/history/instances?business_key=alpha", base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let list: Vec<Value> = res.json().await.unwrap();
    assert!(
        list.iter()
            .all(|i| i["business_key"].as_str().unwrap_or("").contains("alpha")),
        "all results should match business_key filter"
    );
    assert!(!list.is_empty(), "should find at least one match");
}

#[tokio::test]
async fn test_history_instances_pagination() {
    let base = match start_server_with_nats().await {
        Some(b) => b,
        None => return,
    };

    let client = reqwest::Client::new();

    // Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "PaginationTest"
        }))
        .send()
        .await
        .unwrap();
    let deploy_body: Value = deploy_res.json().await.unwrap();
    let def_key = deploy_body["definition_key"].as_str().unwrap().to_string();

    // Start 3 instances
    for _ in 0..3 {
        client
            .post(format!("{}/api/start", base))
            .json(&serde_json::json!({ "definition_key": def_key }))
            .send()
            .await
            .unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Request with limit=2
    let res = client
        .get(format!("{}/api/history/instances?limit=2", base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let page1: Vec<Value> = res.json().await.unwrap();
    assert!(page1.len() <= 2, "limit=2 should return at most 2 results");
}
