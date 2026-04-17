use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use engine_core::WorkflowEngine;
use engine_core::persistence::WorkflowPersistence;
use engine_server::{LogBuffer, NatsLogSink, StartupCoordinator, build_app_with_engine};
use persistence_nats::NatsPersistence;
use tracing_subscriber::prelude::*;

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
    // Setup tracing — fmt-Layer für Konsole + LogBuffer-Layer für /api/logs
    let format = env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // Datei-Persistenz für den Log-Buffer.
    // LOG_FILE=off  → rein in-memory (kein Schreiben auf Disk)
    // LOG_FILE=/pfad/zur/engine_logs.jsonl  → benutzerdefinierter Pfad
    // Standard: ./engine_logs.jsonl im Arbeitsverzeichnis
    let log_file_env = env::var("LOG_FILE").unwrap_or_else(|_| "engine_logs.jsonl".to_string());
    let log_buffer = Arc::new(if log_file_env.eq_ignore_ascii_case("off") {
        LogBuffer::new()
    } else {
        LogBuffer::new_with_persistence(&log_file_env)
    });
    let buffer_layer = (*log_buffer).clone();

    // Der Buffer-Layer bekommt denselben Filter wie der fmt-Layer, damit er
    // nicht mit internem TRACE/DEBUG-Spam von tokio/hyper/nats geflutet wird.
    // Der Filter wird neu aus RUST_LOG erzeugt (EnvFilter ist nicht Clone).
    let buffer_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if format.to_lowercase() == "json" {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().json().with_filter(filter))
            .with(buffer_layer.with_filter(buffer_filter))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(filter))
            .with(buffer_layer.with_filter(buffer_filter))
            .init();
    }

    tracing::info!("Starting bpmninja engine-server...");

    let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    let mut xml_cache = HashMap::new();

    let (engine, nats_persistence) = match NatsPersistence::connect(&nats_url, "WORKFLOW_EVENTS").await {
        Ok(p) => {
            tracing::info!("Connected to NATS at {}", nats_url);
            let p_arc = Arc::new(p);
            let mut engine = WorkflowEngine::new()
                .with_persistence(p_arc.clone() as Arc<dyn WorkflowPersistence>);
            StartupCoordinator::new(p_arc.clone())
                .restore(&mut engine, &mut xml_cache)
                .await;

            // Log-Persistenz in NATS einrichten
            let log_sink = NatsLogSink::new(p_arc.jetstream()).await;
            let restored = log_buffer.attach_nats_sink(log_sink).await;
            tracing::info!("Log-Persistenz: {} Einträge aus NATS geladen.", restored);

            (engine, Some(p_arc as Arc<dyn WorkflowPersistence>))
        }
        Err(e) => {
            tracing::error!(
                "NATS not available at {} - running IN-MEMORY only! Error: {}",
                nats_url,
                e
            );
            (WorkflowEngine::new(), None)
        }
    };

    // Install Prometheus metrics recorder (must happen before any metrics::* calls)
    let prometheus_handle = engine_server::observability::install_prometheus_recorder();
    tracing::info!("Prometheus metrics enabled at /metrics");

    let engine_arc = Arc::new(engine);
    let app = build_app_with_engine(
        engine_arc.clone(),
        nats_persistence,
        xml_cache,
        Some(prometheus_handle),
        log_buffer,
    );

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
