//! E2E tests for instance file variables.
//! Verifies the full lifecycle: upload → verify → survive task completion → delete.

use reqwest::multipart;
use serde_json::Value;

async fn start_server() -> String {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().expect("failed to get addr");
    let base = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });
    base
}

const BPMN_USER_TASK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Definitions_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="FileTest" isExecutable="true">
    <startEvent id="start" />
    <userTask id="task" />
    <endEvent id="end" />
    <sequenceFlow id="f1" sourceRef="start" targetRef="task" />
    <sequenceFlow id="f2" sourceRef="task" targetRef="end" />
  </process>
</definitions>"#;

/// Helper: deploy + start an instance, returning (base_url, instance_id).
async fn deploy_and_start(client: &reqwest::Client, base: &str) -> String {
    let res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": BPMN_USER_TASK, "name": "file-test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let def_key = res.json::<Value>().await.unwrap()["definition_key"]
        .as_str()
        .unwrap()
        .to_string();

    let res = client
        .post(format!("{}/api/start", base))
        .json(&serde_json::json!({ "definition_key": def_key }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.json::<Value>().await.unwrap()["instance_id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Helper: upload a file to an instance variable.
async fn upload_file(
    client: &reqwest::Client,
    base: &str,
    instance_id: &str,
    var_name: &str,
    filename: &str,
    content: &[u8],
) {
    let part = multipart::Part::bytes(content.to_vec())
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .unwrap();
    let form = multipart::Form::new().part("file", part);
    let res = client
        .post(format!(
            "{}/api/instances/{}/files/{}",
            base, instance_id, var_name
        ))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        201,
        "upload should return 201 CREATED, got: {}",
        res.status()
    );
}

#[tokio::test]
async fn file_variable_upload_creates_reference() {
    let base = start_server().await;
    let client = reqwest::Client::new();
    let inst_id = deploy_and_start(&client, &base).await;

    // Upload a file
    upload_file(
        &client,
        &base,
        &inst_id,
        "contract",
        "contract.pdf",
        b"PDF-CONTENT-HERE",
    )
    .await;

    // Verify the variable is a FileReference
    let res = client
        .get(format!("{}/api/instances/{}", base, inst_id))
        .send()
        .await
        .unwrap();
    let inst: Value = res.json().await.unwrap();
    let file_var = &inst["variables"]["contract"];

    assert_eq!(file_var["type"], "file", "variable type must be 'file'");
    assert_eq!(file_var["filename"], "contract.pdf");
    assert_eq!(file_var["size_bytes"], 16); // b"PDF-CONTENT-HERE".len()
}

#[tokio::test]
async fn file_variable_survives_task_completion() {
    let base = start_server().await;
    let client = reqwest::Client::new();
    let inst_id = deploy_and_start(&client, &base).await;

    // Upload file
    upload_file(
        &client,
        &base,
        &inst_id,
        "attachment",
        "report.xlsx",
        b"EXCEL-DATA",
    )
    .await;

    // Complete user task
    let tasks: Vec<Value> = client
        .get(format!("{}/api/tasks", base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let task_id = tasks
        .iter()
        .find(|t| t["instance_id"].as_str() == Some(&inst_id))
        .expect("task not found")["task_id"]
        .as_str()
        .unwrap();

    let res = client
        .post(format!("{}/api/complete/{}", base, task_id))
        .json(&serde_json::json!({ "variables": {} }))
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success(), "complete task failed");

    // Instance should be Completed, file variable still present
    let inst: Value = client
        .get(format!("{}/api/instances/{}", base, inst_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(inst["state"], "Completed");
    assert_eq!(inst["variables"]["attachment"]["type"], "file");
    assert_eq!(inst["variables"]["attachment"]["filename"], "report.xlsx");
}

#[tokio::test]
async fn multiple_file_variables_and_delete_one() {
    let base = start_server().await;
    let client = reqwest::Client::new();
    let inst_id = deploy_and_start(&client, &base).await;

    // Upload 3 files
    upload_file(&client, &base, &inst_id, "doc_a", "a.txt", b"AAA").await;
    upload_file(&client, &base, &inst_id, "doc_b", "b.txt", b"BBBB").await;
    upload_file(&client, &base, &inst_id, "doc_c", "c.txt", b"CCCCC").await;

    // Verify all 3 exist
    let inst: Value = client
        .get(format!("{}/api/instances/{}", base, inst_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(inst["variables"]["doc_a"]["filename"], "a.txt");
    assert_eq!(inst["variables"]["doc_b"]["filename"], "b.txt");
    assert_eq!(inst["variables"]["doc_c"]["filename"], "c.txt");

    // Delete doc_b
    let res = client
        .delete(format!("{}/api/instances/{}/files/doc_b", base, inst_id))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // Verify only 2 remain
    let inst: Value = client
        .get(format!("{}/api/instances/{}", base, inst_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(inst["variables"]["doc_a"]["filename"], "a.txt");
    assert!(
        inst["variables"].get("doc_b").is_none() || inst["variables"]["doc_b"].is_null(),
        "doc_b should be deleted"
    );
    assert_eq!(inst["variables"]["doc_c"]["filename"], "c.txt");
}
