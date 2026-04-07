use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

use engine_core::engine::WorkflowEngine;
use engine_core::persistence::WorkflowPersistence;
use engine_server::build_app_with_engine;
use persistence_nats::NatsPersistence;

async fn restore_from_nats(
    nats: &NatsPersistence,
    engine: &mut WorkflowEngine,
    deployed_xml: &mut HashMap<String, String>,
) {
    let ids = match nats.list_bpmn_xml_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("Failed to list definitions from NATS: {:?}", e);
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
                    let (key, _) = engine.deploy_definition(def).await;
                    deployed_xml.insert(key.to_string(), xml);
                    tracing::info!("Restored definition (key: {})", key);
                }
                Err(e) => tracing::error!("Failed to parse '{}': {:?}", nats_key, e),
            },
            Err(e) => tracing::error!("Failed to load XML for '{}': {:?}", nats_key, e),
        }
    }
    tracing::info!("Restore complete: {count} definition(s) found.");

    match nats.list_instances().await {
        Ok(instances) => {
            let num = instances.len();
            for inst in instances {
                engine.restore_instance(inst).await;
            }
            tracing::info!("Restored {} process instance(s).", num);
        }
        Err(e) => tracing::error!("Failed to list instances: {:?}", e),
    }

    match nats.list_user_tasks().await {
        Ok(tasks) => {
            let num = tasks.len();
            for task in tasks {
                engine.restore_user_task(task);
            }
            tracing::info!("Restored {} pending user task(s).", num);
        }
        Err(e) => tracing::error!("Failed to list user tasks: {:?}", e),
    }

    match nats.list_service_tasks().await {
        Ok(tasks) => {
            let num = tasks.len();
            for task in tasks {
                engine.restore_service_task(task);
            }
            tracing::info!("Restored {} pending service task(s).", num);
        }
        Err(e) => tracing::error!("Failed to list service tasks: {:?}", e),
    }

    match nats.list_timers().await {
        Ok(timers) => {
            let num = timers.len();
            for timer in timers {
                engine.restore_timer(timer);
            }
            tracing::info!("Restored {} pending timer(s).", num);
        }
        Err(e) => tracing::error!("Failed to list timers: {:?}", e),
    }

    match nats.list_message_catches().await {
        Ok(catches) => {
            let num = catches.len();
            for catch in catches {
                engine.restore_message_catch(catch);
            }
            tracing::info!("Restored {} pending message catch(es).", num);
        }
        Err(e) => tracing::error!("Failed to list message catches: {:?}", e),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup tracing
    let format = env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if format.to_lowercase() == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    tracing::info!("Starting bpmninja engine-server...");

    let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let mut engine = WorkflowEngine::new();
    let mut xml_cache = HashMap::new();

    let nats_persistence = match NatsPersistence::connect(&nats_url, "WORKFLOW_EVENTS").await {
        Ok(p) => {
            tracing::info!("Connected to NATS at {}", nats_url);
            let p_arc = Arc::new(p);
            engine.set_persistence(p_arc.clone() as Arc<dyn WorkflowPersistence>);
            restore_from_nats(&p_arc, &mut engine, &mut xml_cache).await;
            Some(p_arc as Arc<dyn WorkflowPersistence>)
        }
        Err(e) => {
            tracing::error!(
                "NATS not available at {} - running IN-MEMORY only! Error: {}",
                nats_url,
                e
            );
            None
        }
    };

    let engine_arc = Arc::new(engine);
    let app = build_app_with_engine(engine_arc.clone(), nats_persistence, xml_cache);

    // Background timer scheduler — processes expired timers automatically
    let timer_interval_ms: u64 = env::var("TIMER_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let timer_engine = engine_arc.clone();
    let timer_task = tokio::spawn(async move {
        let interval = tokio::time::Duration::from_millis(timer_interval_ms);
        tracing::info!(
            "Timer scheduler started (interval: {}ms)",
            timer_interval_ms
        );
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let engine = &timer_engine;
                    match engine.process_timers().await {
                        Ok(0) => {} // No expired timers — silent
                        Ok(n) => tracing::info!("Timer scheduler: processed {} expired timer(s)", n),
                        Err(e) => tracing::error!("Timer scheduler error: {}", e),
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("Timer scheduler shutting down");
                    break;
                }
            }
        }
    });

    let port = env::var("PORT").unwrap_or_else(|_| "8081".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    let axum_shutdown = shutdown_signal();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            axum_shutdown.await;
            tracing::info!("Received shutdown signal. Stopping API...");
            let _ = shutdown_tx.send(true);
        })
        .await?;

    tracing::info!("Flushing persistence queues...");
    engine_arc.shutdown().await;

    let _ = timer_task.await;
    tracing::info!("Server shut down gracefully.");

    Ok(())
}
