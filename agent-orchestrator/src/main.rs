use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if format.to_lowercase() == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }

    let base_url = std::env::var("ENGINE_API_URL")
        .unwrap_or_else(|_| "http://localhost:8081".to_string());

    let client = reqwest::Client::new();

    tracing::info!("Starting mini-bpm agent-orchestrator (HTTP mode)...");
    tracing::info!("Engine API: {}", base_url);

    // 1. Deploy example BPMN
    let bpmn_xml = std::fs::read_to_string("example.bpmn")?;
    let deploy_res: Value = client
        .post(format!("{}/api/deploy", base_url))
        .json(&serde_json::json!({ "xml": bpmn_xml, "name": "example" }))
        .send().await?
        .json().await?;
    let def_key = deploy_res["definition_key"].as_str().unwrap();
    tracing::info!("Deployed definition: {}", def_key);

    // 2. Start instance
    let start_res: Value = client
        .post(format!("{}/api/start", base_url))
        .json(&serde_json::json!({ "definition_key": def_key }))
        .send().await?
        .json().await?;
    let instance_id = start_res["instance_id"].as_str().unwrap();
    tracing::info!("Started instance: {}", instance_id);

    // 3. Fetch and complete service tasks
    let tasks: Vec<Value> = client
        .post(format!("{}/api/service-task/fetchAndLock", base_url))
        .json(&serde_json::json!({
            "workerId": "orchestrator",
            "maxTasks": 10,
            "topics": [{ "topicName": "InitialProcessing", "lockDuration": 10000 }]
        }))
        .send().await?
        .json().await?;

    for task in &tasks {
        let task_id = task["id"].as_str().unwrap();
        tracing::info!("Completing service task: {}", task_id);
        client
            .post(format!("{}/api/service-task/{}/complete", base_url, task_id))
            .json(&serde_json::json!({
                "workerId": "orchestrator",
                "variables": { "processed": true }
            }))
            .send().await?;
    }

    // 4. Complete pending user tasks
    let user_tasks: Vec<Value> = client
        .get(format!("{}/api/tasks", base_url))
        .send().await?
        .json().await?;

    for task in &user_tasks {
        let task_id = task["task_id"].as_str().unwrap();
        tracing::info!("Completing user task: {}", task_id);
        client
            .post(format!("{}/api/complete/{}", base_url, task_id))
            .json(&serde_json::json!({ "variables": {} }))
            .send().await?;
    }

    // 5. Show final state
    let instance: Value = client
        .get(format!("{}/api/instances/{}", base_url, instance_id))
        .send().await?
        .json().await?;
    tracing::info!("Final state: {}", serde_json::to_string_pretty(&instance)?);

    Ok(())
}
