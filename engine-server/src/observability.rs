//! Prometheus metrics setup and HTTP metrics middleware.
//!
//! Uses the `metrics` crate (Rust-idiomatic, actively maintained) instead of
//! `opentelemetry-prometheus` which is officially **discontinued** as of v0.31.
//!
//! # Exposed metrics
//!
//! | Metric                                  | Type      | Description                                   |
//! |-----------------------------------------|-----------|-----------------------------------------------|
//! | `bpmn_instance_started_total`           | Counter   | Process instances started                     |
//! | `bpmn_instance_completed_total`         | Counter   | Process instances completed                   |
//! | `bpmn_tasks_completed_total`            | Counter   | User/service tasks completed                  |
//! | `bpmn_timer_fired_total`                | Counter   | Timers processed                              |
//! | `bpmn_errors_total`                     | Counter   | Script/engine errors                          |
//! | `bpmn_script_execution_duration_seconds`| Histogram | Rhai script execution latency                 |
//! | `bpmn_active_instances`                 | Gauge     | Currently active (non-completed) instances    |
//! | `http_requests_total`                   | Counter   | HTTP requests (method, path, status)          |
//! | `http_request_duration_seconds`         | Histogram | HTTP request latency (method, path)           |

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
};
use metrics_exporter_prometheus::PrometheusHandle;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Prometheus recorder setup
// ---------------------------------------------------------------------------

/// Installs the global `metrics` recorder backed by Prometheus and returns
/// the handle used to render `/metrics` output.
pub fn install_prometheus_recorder() -> PrometheusHandle {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Register process-level metrics (RSS, CPU, open FDs, ...)
    let collector = metrics_process::Collector::default();
    collector.describe();
    collector.collect();

    // Pre-describe custom metrics so they appear even before the first event
    metrics::describe_counter!(
        "bpmn_instance_started_total",
        "Total number of process instances started"
    );
    metrics::describe_counter!(
        "bpmn_instance_completed_total",
        "Total number of process instances completed"
    );
    metrics::describe_counter!(
        "bpmn_tasks_completed_total",
        "Total number of user/service tasks completed"
    );
    metrics::describe_counter!(
        "bpmn_timer_fired_total",
        "Total number of timers processed"
    );
    metrics::describe_counter!(
        "bpmn_errors_total",
        "Total number of script/engine errors"
    );
    metrics::describe_histogram!(
        "bpmn_script_execution_duration_seconds",
        "Duration of Rhai script executions in seconds"
    );
    metrics::describe_gauge!(
        "bpmn_active_instances",
        "Number of currently active (non-completed) instances"
    );
    metrics::describe_counter!(
        "http_requests_total",
        "Total HTTP requests handled"
    );
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );

    handle
}

// ---------------------------------------------------------------------------
// /metrics handler
// ---------------------------------------------------------------------------

/// Axum handler that renders all registered metrics in Prometheus text format.
pub async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> impl IntoResponse {
    // Refresh process metrics on each scrape
    metrics_process::Collector::default().collect();

    let body = handle.render();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

// ---------------------------------------------------------------------------
// HTTP metrics middleware (Axum middleware::from_fn)
// ---------------------------------------------------------------------------

/// Axum middleware that records `http_requests_total` and
/// `http_request_duration_seconds` for every request.
pub async fn http_metrics_middleware(req: Request<Body>, next: Next) -> impl IntoResponse {
    let method = req.method().to_string();
    let path = normalize_path(req.uri().path());
    let start = Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let elapsed = start.elapsed().as_secs_f64();

    metrics::counter!("http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status,
    )
    .increment(1);

    metrics::histogram!("http_request_duration_seconds",
        "method" => method,
        "path" => path,
    )
    .record(elapsed);

    response
}

/// Normalize path to reduce metric cardinality.
/// Replaces UUID segments and numeric IDs with `{id}`.
fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.len() == 36 && seg.chars().filter(|c| *c == '-').count() == 4 {
                "{id}" // UUID
            } else if !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()) {
                "{id}" // numeric
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("/api/instances/550e8400-e29b-41d4-a716-446655440000"),
            "/api/instances/{id}"
        );
        assert_eq!(normalize_path("/api/complete/42"), "/api/complete/{id}");
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert_eq!(normalize_path("/api/service-task/550e8400-e29b-41d4-a716-446655440000/complete"),
            "/api/service-task/{id}/complete");
    }
}
