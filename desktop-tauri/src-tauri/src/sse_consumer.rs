//! Background SSE consumer — verbindet sich mit `/api/events` der Engine und emittiert
//! Tauri-Events, damit die UI push-basiert aktualisiert werden kann.
//!
//! Das Tauri-Event `engine-event` wird mit einer `{type: string}` Payload gefeuert.
//! Reconnect-Schleife mit exponentiellem Backoff (max 30s).

use futures_util::TryStreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

/// Spawnt den SSE-Consumer-Task. Gibt sofort zurück; Arbeit läuft im Hintergrund.
pub fn spawn(app_handle: AppHandle, base_url: String, client: Client) {
    tauri::async_runtime::spawn(run_consumer(app_handle, base_url, client));
}

async fn run_consumer(app_handle: AppHandle, base_url: String, client: Client) {
    let mut backoff_secs: u64 = 1;
    let url = format!("{}/api/events", base_url);

    loop {
        tracing_or_eprintln(format!("SSE: Verbinde mit {url}"));

        match connect_and_consume(&client, &url, &app_handle).await {
            Ok(()) => {
                // Stream endete ohne Fehler (Server hat Verbindung geschlossen)
                tracing_or_eprintln("SSE: Stream beendet, verbinde neu…".to_string());
                backoff_secs = 1;
            }
            Err(e) => {
                tracing_or_eprintln(format!("SSE: Fehler — {e}, Wiederverbindung in {backoff_secs}s"));
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);
    }
}

async fn connect_and_consume(
    client: &Client,
    url: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let response = client
        .get(url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let byte_stream = response
        .bytes_stream()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
    let stream_reader = StreamReader::new(byte_stream);
    let mut lines = BufReader::new(stream_reader).lines();

    let mut current_event_type = String::new();

    while let Ok(Some(line)) = lines.next_line().await {
        let line: String = line;
        if let Some(event_type) = line.strip_prefix("event:") {
            current_event_type = event_type.trim().to_string();
        } else if line.starts_with("data:") {
            // Dispatch the event — type carries the semantic
            if !current_event_type.is_empty() {
                let _ = app_handle.emit(
                    "engine-event",
                    serde_json::json!({ "type": current_event_type }),
                );
            }
        } else if line.is_empty() {
            // Blank line = end of SSE message block, reset type
            current_event_type.clear();
        }
    }

    Ok(())
}

/// Minimal fallback logging without requiring a tracing dependency in the Tauri crate.
fn tracing_or_eprintln(msg: String) {
    eprintln!("[bpmninja-sse] {msg}");
}
