//! E2E tests for monitoring and health endpoints:
//! - GET /api/health
//! - GET /api/ready
//! - GET /api/info
//! - GET /api/monitoring (detailed stats)

use serde_json::Value;

async fn start_server() -> String {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    let addr = listener.local_addr().expect("addr failed");
    let base = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

#[tokio::test]
async fn health_endpoint_returns_200() {
    let base = start_server().await;
    let res = reqwest::get(format!("{}/api/health", base)).await.unwrap();
    assert_eq!(
        res.status(),
        200,
        "Health endpoint should always return 200"
    );
}

#[tokio::test]
async fn ready_endpoint_returns_200_without_nats() {
    let base = start_server().await;
    let res = reqwest::get(format!("{}/api/ready", base)).await.unwrap();
    // Without NATS, ready should still return 200 (no persistence = always ready)
    assert_eq!(res.status(), 200, "Ready without NATS should be 200");
}

#[tokio::test]
async fn info_endpoint_returns_backend_type() {
    let base = start_server().await;
    let res = reqwest::get(format!("{}/api/info", base)).await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body["backend_type"], "in-memory",
        "Without NATS, backend should be in-memory"
    );
    assert_eq!(body["connected"], false);
}

#[tokio::test]
async fn monitoring_endpoint_returns_stats() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // Deploy a definition to have non-zero stats
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
      <process id="TestProcess"><startEvent id="s"/><endEvent id="e"/>
      <sequenceFlow id="f" sourceRef="s" targetRef="e"/></process>
    </definitions>"#;
    client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": xml, "name": "test" }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{}/api/monitoring", base))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["definitions_count"], 1);
    assert_eq!(body["instances_total"], 0);
    assert!(
        body["persistence_errors"].is_number(),
        "Should have persistence_errors field"
    );
}
