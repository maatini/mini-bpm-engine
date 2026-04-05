//! E2E test: Verify that updating instance variables while paused
//! at a User Task correctly persists and is not overwritten when the
//! task is completed.

use serde_json::Value;

const USER_TASK_BPMN_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions id="Def_1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
  <process id="UserTaskProcess">
    <startEvent id="Start" />
    <userTask id="Task_1" />
    <endEvent id="End" />
    <sequenceFlow id="F1" sourceRef="Start" targetRef="Task_1" />
    <sequenceFlow id="F2" sourceRef="Task_1" targetRef="End" />
  </process>
</definitions>"#;

async fn start_server() -> String {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().expect("failed to get local addr");
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });

    base_url
}

#[tokio::test]
async fn test_update_variables_mid_execution_persists() {
    let base = start_server().await;
    let client = reqwest::Client::new();

    // 1. Deploy
    let deploy_res = client
        .post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({
            "xml": USER_TASK_BPMN_XML,
            "name": "UserTaskProcess"
        }))
        .send()
        .await
        .expect("deploy failed");

    let deploy_body: Value = deploy_res.json().await.unwrap();
    let def_key = deploy_body["definition_key"].as_str().unwrap().to_string();

    // 2. Start
    let start_res = client
        .post(format!("{}/api/start", base))
        .json(&serde_json::json!({
            "definition_key": def_key,
            "variables": { "initial_var": "A" }
        }))
        .send()
        .await
        .expect("start failed");

    let start_body: Value = start_res.json().await.unwrap();
    let instance_id = start_body["instance_id"].as_str().unwrap().to_string();

    // 3. Update variables via API while instance is paused at user task
    let update_res = client
        .put(format!("{}/api/instances/{}/variables", base, instance_id))
        .json(&serde_json::json!({
            "variables": { "mid_execution_var": "B" }
        }))
        .send()
        .await
        .expect("update variables failed");

    assert_eq!(
        update_res.status(),
        204,
        "Variable update should return 204 No Content"
    );

    // 4. Fetch pending tasks to get the Task ID
    let tasks_res = client
        .get(format!("{}/api/tasks", base))
        .send()
        .await
        .expect("get tasks failed");

    let tasks_body: Value = tasks_res.json().await.unwrap();
    let tasks_arr = tasks_body.as_array().unwrap();
    assert_eq!(
        tasks_arr.len(),
        1,
        "There should be exactly one pending user task"
    );

    let task_id = tasks_arr[0]["task_id"].as_str().unwrap().to_string();

    // 5. Complete User Task
    let complete_res = client
        .post(format!("{}/api/complete/{}", base, task_id))
        .json(&serde_json::json!({
            "variables": { "final_var": "C" }
        }))
        .send()
        .await
        .expect("complete task failed");

    assert_eq!(
        complete_res.status(),
        204,
        "Complete should return 204 No Content"
    );

    // 6. Fetch final instance details
    let details_res = client
        .get(format!("{}/api/instances/{}", base, instance_id))
        .send()
        .await
        .expect("get instance failed");

    let details: Value = details_res.json().await.unwrap();

    assert_eq!(
        details["state"], "Completed",
        "Instance should be completed"
    );

    // Verify variables contains ALL phases of variables
    let vars = &details["variables"];
    assert_eq!(vars["initial_var"], "A", "Initial var missing");
    assert_eq!(
        vars["mid_execution_var"], "B",
        "Directly updated var was overwritten! (Bug reproduced)"
    );
    assert_eq!(vars["final_var"], "C", "Completion var missing");
}
