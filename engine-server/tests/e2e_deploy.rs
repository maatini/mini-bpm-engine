//! E2E test: Deploy a minimal BPMN workflow (StartEvent → EndEvent)
//! and verify the instance completes immediately.

use serde_json::Value;

/// Minimal BPMN 2.0 XML with only a StartEvent, EndEvent, and one SequenceFlow.
const MINIMAL_BPMN_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="MinimalProcess">
    <startEvent id="Start_1" />
    <endEvent id="End_1" />
    <sequenceFlow id="Flow_1" sourceRef="Start_1" targetRef="End_1" />
  </process>
</definitions>"#;

/// Helper: starts the engine-server on a random port and returns the base URL.
async fn start_server() -> String {
    let app = engine_server::build_app();

    // Bind to port 0 → OS assigns a free port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to random port");
    let addr = listener.local_addr().expect("failed to get local addr");
    let base_url = format!("http://{}", addr);

    // Spawn the server in the background
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });

    base_url
}

#[tokio::test]
async fn deploy_minimal_workflow_returns_definition_id() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Deploy the minimal BPMN process
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "MinimalProcess"
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(res.status(), 200, "deploy should return 200 OK");

    let body: Value = res.json().await.expect("failed to parse deploy response");
    let def_key = body["definition_key"]
        .as_str()
        .expect("response should contain definition_key");

    // The returned key should be a valid UUID
    assert!(
        uuid::Uuid::parse_str(def_key).is_ok(),
        "definition_key should be a valid UUID"
    );
}

#[tokio::test]
async fn deploy_and_start_minimal_workflow_completes_immediately() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Step 1: Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": MINIMAL_BPMN_XML,
            "name": "MinimalProcess"
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

    // Step 3: Verify instance is Completed (Start → End = immediate)
    let details_res = client
        .get(format!("{}/api/instances/{}", base, instance_id))
        .send()
        .await
        .expect("get instance request failed");

    assert_eq!(details_res.status(), 200, "get instance should return 200");

    let details: Value = details_res.json().await.expect("parse instance details");
    assert_eq!(
        details["state"], "Completed",
        "instance should be Completed after Start→End workflow"
    );
    assert_eq!(
        details["definition_key"].as_str().map(|s| s.to_string()),
        Some(def_key),
        "definition_key should match the deployed key"
    );
}

#[tokio::test]
async fn deploy_invalid_xml_returns_400() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": "<invalid>not valid bpmn</invalid>",
            "name": "BadProcess"
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(
        res.status(),
        400,
        "deploying invalid XML should return 400 BAD REQUEST"
    );
}
