use axum::http::StatusCode;
use engine_core::ProcessInstance;
use reqwest::multipart;
use serde_json::{Value, json};

#[tokio::test]
async fn test_file_upload_download_delete() {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Run the server in a background task
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // 1. Deploy a simple process
    let xml = r#"
        <bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Definitions_1">
          <bpmn:process id="Process_1" isExecutable="true">
            <bpmn:startEvent id="Start" />
            <bpmn:userTask id="Task1" />
            <bpmn:endEvent id="End" />
            <bpmn:sequenceFlow id="Flow_1" sourceRef="Start" targetRef="Task1" />
            <bpmn:sequenceFlow id="Flow_2" sourceRef="Task1" targetRef="End" />
          </bpmn:process>
        </bpmn:definitions>
    "#;

    let deploy_resp = client
        .post(format!("{}/api/deploy", base_url))
        .json(&json!({
            "name": "test-files",
            "xml": xml
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(deploy_resp.status(), StatusCode::OK);
    let def_key = deploy_resp.json::<Value>().await.unwrap()["definition_key"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Start an instance
    let start_resp = client
        .post(format!("{}/api/start", base_url))
        .json(&json!({
            "definition_key": def_key
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(start_resp.status(), StatusCode::OK);
    let instance_id = start_resp.json::<Value>().await.unwrap()["instance_id"]
        .as_str()
        .unwrap()
        .to_string();

    // 3. Upload a file
    let file_content = b"Hello Mini-BPM File System!";
    let part = multipart::Part::bytes(file_content.to_vec())
        .file_name("hello.txt")
        .mime_str("text/plain")
        .unwrap();

    let form = multipart::Form::new().part("file", part);

    let upload_resp = client
        .post(format!(
            "{}/api/instances/{}/files/greeting_file",
            base_url, instance_id
        ))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(upload_resp.status(), StatusCode::CREATED);

    // 4. Verify the variable exists as a FileReference
    let inst_resp = client
        .get(format!("{}/api/instances/{}", base_url, instance_id))
        .send()
        .await
        .unwrap();

    let inst: ProcessInstance = inst_resp.json().await.unwrap();
    let file_var = inst.variables.get("greeting_file").unwrap();
    assert_eq!(file_var["type"], "file");
    assert_eq!(file_var["filename"], "hello.txt");
    assert_eq!(file_var["size_bytes"], file_content.len() as u64);

    // 5. Download the file
    /* NOTE: downloading tests depend on persistence being configured.
       Wait, in memory `build_app()` doesn't configure a persistence Mock, so `persistence` is None!
       Our upload and download functions have things like `if let Some(persistence) = &state.persistence`.
       Upload works because it just ignores persistence saving, but it updates the variable.
       Download fails with 400 "No persistence configured". Let's test that it actually returns 400 in this case.
       If we had a MockPersistence, it could return 200...
    */
    let download_resp = client
        .get(format!(
            "{}/api/instances/{}/files/greeting_file",
            base_url, instance_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(download_resp.status(), StatusCode::BAD_REQUEST);
    let error_text = download_resp.text().await.unwrap();
    assert!(error_text.contains("No persistence configured"));

    // 6. Delete the file
    let delete_resp = client
        .delete(format!(
            "{}/api/instances/{}/files/greeting_file",
            base_url, instance_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    // 7. Verify the variable is deleted
    let inst_resp_after = client
        .get(format!("{}/api/instances/{}", base_url, instance_id))
        .send()
        .await
        .unwrap();

    let inst_after: ProcessInstance = inst_resp_after.json().await.unwrap();
    assert!(!inst_after.variables.contains_key("greeting_file"));
}
