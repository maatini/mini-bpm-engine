use std::sync::Arc;
use tokio::time::{sleep, Duration};

use bpmn_parser::parse_bpmn_xml;
use engine_core::engine::WorkflowEngine;
use persistence_nats::persistence::NatsPersistence;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize standard logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting mini-bpm workflow engine...");
    let mut engine = WorkflowEngine::new();

    // Optional: Connect to NATS JetStream for tracking workflow events
    match NatsPersistence::connect("nats://localhost:4222", "WORKFLOW_EVENTS").await {
        Ok(nats) => {
            log::info!("Connected to NATS JetStream for token persistence.");
            engine = engine.with_persistence(Arc::new(nats));
        }
        Err(e) => {
            log::warn!(
                "NATS not available at nats://localhost:4222, running in strictly in-memory mode. Reason: {}",
                e
            );
        }
    }

    // 1. Read and Parse BPMN file
    let bpmn_xml = std::fs::read_to_string("example.bpmn")?;
    let definition = parse_bpmn_xml(&bpmn_xml)?;

    // 2. Deploy definition to Engine
    let def_key = engine.deploy_definition(definition).await;

    // 4. Start instance
    log::info!("===========================================");
    log::info!("Starting Process_1...");
    let instance_id = engine.start_instance(def_key).await?;
    log::info!("Process instance started with ID: {}", instance_id);


    // Give engine time to progress to the service task
    sleep(Duration::from_millis(150)).await;

    let svc_tasks = engine.fetch_and_lock_service_tasks("orchestrator", 1, &["InitialProcessing".to_string()], 10000).await;
    for task in svc_tasks {
        log::info!("  -> [ServiceTask] Completing '{}'", task.node_id);
        let mut vars = std::collections::HashMap::new();
        vars.insert("processed".to_string(), serde_json::Value::Bool(true));
        engine.complete_service_task(task.id, "orchestrator", vars).await?;
    }

    // Give engine time to progress to the user task...
    sleep(Duration::from_millis(150)).await;

    // 5. Check out pending manual tasks
    let pending_tasks = engine.get_pending_user_tasks();
    log::info!("Found {} pending user task(s).", pending_tasks.len());

    if let Some(task) = pending_tasks.first() {
        log::info!("  -> [UserTask] Completing '{}' for user '{}'", task.node_id, task.assignee);
        engine.complete_user_task(task.task_id, Default::default()).await?;
    }

    // Give engine time to complete the flow...
    sleep(Duration::from_millis(50)).await;

    // View final instance state
    if let Some(instance) = engine.instances.get(&instance_id) {
        log::info!("===========================================");
        log::info!("Final instance state:");
        log::info!("ID: {}", instance.id);
        log::info!("Definition: {}", instance.definition_key);
        log::info!("State: {:?}", instance.state);
        log::info!("Current Node: {}", instance.current_node);
        log::info!("Audit Log Length: {}", instance.audit_log.len());
        for event in instance.audit_log.iter() {
            log::info!(" - Event: {:?}", event);
        }
    }

    Ok(())
}
