use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use engine_core::engine::WorkflowEngine;
use engine_core::persistence::WorkflowPersistence;
use persistence_nats::NatsPersistence;
use engine_server::build_app_with_engine;

async fn restore_from_nats(
    nats: &NatsPersistence,
    engine: &mut WorkflowEngine,
    deployed_xml: &mut HashMap<String, String>,
) {
    let ids = match nats.list_bpmn_xml_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            log::error!("Failed to list definitions from NATS: {:?}", e);
            return;
        }
    };
    let count = ids.len();
    for nats_key in ids {
        match nats.load_bpmn_xml(&nats_key).await {
            Ok(xml) => match bpmn_parser::parse_bpmn_xml(&xml) {
                Ok(mut def) => {
                    if let Ok(old_uuid) = Uuid::parse_str(&nats_key) {
                        def.key = old_uuid;
                    }
                    let key = engine.deploy_definition(def).await;
                    deployed_xml.insert(key.to_string(), xml);
                    log::info!("Restored definition (key: {})", key);
                }
                Err(e) => log::error!("Failed to parse '{}': {:?}", nats_key, e),
            },
            Err(e) => log::error!("Failed to load XML for '{}': {:?}", nats_key, e),
        }
    }
    log::info!("Restore complete: {count} definition(s) found.");

    match nats.list_instances().await {
        Ok(instances) => {
            let num = instances.len();
            for inst in instances {
                engine.restore_instance(inst).await;
            }
            log::info!("Restored {} process instance(s).", num);
        }
        Err(e) => log::error!("Failed to list instances: {:?}", e),
    }

    match nats.list_user_tasks().await {
        Ok(tasks) => {
            let num = tasks.len();
            for task in tasks {
                engine.restore_user_task(task);
            }
            log::info!("Restored {} pending user task(s).", num);
        }
        Err(e) => log::error!("Failed to list user tasks: {:?}", e),
    }

    match nats.list_service_tasks().await {
        Ok(tasks) => {
            let num = tasks.len();
            for task in tasks {
                engine.restore_service_task(task);
            }
            log::info!("Restored {} pending service task(s).", num);
        }
        Err(e) => log::error!("Failed to list service tasks: {:?}", e),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting mini-bpm engine-server...");

    let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    
    let mut engine = WorkflowEngine::new();
    let mut xml_cache = HashMap::new();
    
    let nats_persistence = match NatsPersistence::connect(&nats_url, "WORKFLOW_EVENTS").await {
        Ok(p) => {
            log::info!("Connected to NATS at {}", nats_url);
            let p_arc = Arc::new(p);
            engine.set_persistence(p_arc.clone() as Arc<dyn WorkflowPersistence>);
            restore_from_nats(&p_arc, &mut engine, &mut xml_cache).await;
            Some(p_arc as Arc<dyn WorkflowPersistence>)
        }
        Err(e) => {
            log::error!("NATS not available at {} - running IN-MEMORY only! Error: {}", nats_url, e);
            None
        }
    };

    let app = build_app_with_engine(Arc::new(RwLock::new(engine)), nats_persistence, xml_cache);

    let port = env::var("PORT").unwrap_or_else(|_| "8081".to_string());
    let addr = format!("0.0.0.0:{}", port);
    log::info!("Server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
