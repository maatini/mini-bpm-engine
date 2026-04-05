//! E2E tests for workflow versioning.

use serde_json::Value;

async fn start_server() -> String {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

const BPMN_V1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="OrderProcess" isExecutable="true">
    <startEvent id="start" />
    <userTask id="approve" />
    <endEvent id="end" />
    <sequenceFlow id="f1" sourceRef="start" targetRef="approve" />
    <sequenceFlow id="f2" sourceRef="approve" targetRef="end" />
  </process>
</definitions>"#;

const BPMN_V2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="OrderProcess" isExecutable="true">
    <startEvent id="start" />
    <userTask id="review" />
    <userTask id="approve" />
    <endEvent id="end" />
    <sequenceFlow id="f1" sourceRef="start" targetRef="review" />
    <sequenceFlow id="f2" sourceRef="review" targetRef="approve" />
    <sequenceFlow id="f3" sourceRef="approve" targetRef="end" />
  </process>
</definitions>"#;

#[tokio::test]
async fn deploy_same_bpmn_id_increments_version() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Deploy V1
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V1, "name": "Order" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let key_v1 = body["definition_key"].as_str().unwrap().to_string();
    assert_eq!(body["version"], 1);

    // Deploy V2 (same process ID "OrderProcess")
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V2, "name": "Order" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let key_v2 = body["definition_key"].as_str().unwrap().to_string();
    assert_eq!(body["version"], 2);
    assert_ne!(key_v1, key_v2);

    // List definitions should show both with correct versions
    let res = client
        .get(format!("{}/api/definitions", base))
        .send()
        .await
        .unwrap();
    let defs: Vec<Value> = res.json().await.unwrap();
    assert_eq!(defs.len(), 2);

    let v1 = defs.iter().find(|d| d["key"] == key_v1).unwrap();
    let v2 = defs.iter().find(|d| d["key"] == key_v2).unwrap();
    assert_eq!(v1["version"], 1);
    assert_eq!(v1["is_latest"], false);
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["is_latest"], true);
}

#[tokio::test]
async fn instances_stay_on_original_version() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Deploy V1 and start an instance
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V1, "name": "Order" }))
        .send()
        .await
        .unwrap();
    let key_v1 = res.json::<Value>().await.unwrap()["definition_key"]
        .as_str()
        .unwrap()
        .to_string();

    let res = client
        .post(format!("{}/api/start", base))
        .json(&serde_json::json!({ "definition_key": key_v1 }))
        .send()
        .await
        .unwrap();
    let inst_id = res.json::<Value>().await.unwrap()["instance_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Deploy V2
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V2, "name": "Order" }))
        .send()
        .await
        .unwrap();
    let key_v2 = res.json::<Value>().await.unwrap()["definition_key"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(key_v1, key_v2);

    // Verify original instance is still on V1
    let res = client
        .get(format!("{}/api/instances/{}", base, inst_id))
        .send()
        .await
        .unwrap();
    let inst: Value = res.json().await.unwrap();
    assert_eq!(inst["definition_key"].as_str().unwrap(), key_v1);
    assert_eq!(inst["current_node"], "approve");
}

#[tokio::test]
async fn start_latest_uses_newest_version() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Deploy V1
    client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V1, "name": "Order" }))
        .send()
        .await
        .unwrap();

    // Deploy V2
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_V2, "name": "Order" }))
        .send()
        .await
        .unwrap();
    let key_v2 = res.json::<Value>().await.unwrap()["definition_key"]
        .as_str()
        .unwrap()
        .to_string();

    // Start via /api/start/latest
    let res = client
        .post(format!("{}/api/start/latest", base))
        .json(&serde_json::json!({ "bpmn_id": "OrderProcess" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["definition_key"].as_str().unwrap(), key_v2);
    assert_eq!(body["version"], 2);
}
