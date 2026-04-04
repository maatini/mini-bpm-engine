use std::collections::HashMap;

/// Engine + NATS metrics returned to the Monitoring page.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct MonitoringData {
    pub definitions_count: usize,
    pub instances_total: usize,
    pub instances_running: usize,
    pub instances_completed: usize,
    pub pending_user_tasks: usize,
    pub pending_service_tasks: usize,
    #[serde(default)]
    pub pending_timers: usize,
    #[serde(default)]
    pub pending_message_catches: usize,
    pub storage_info: Option<StorageInfoData>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct BucketInfoData {
    pub name: String,
    pub bucket_type: String,
    pub entries: u64,
    pub size_bytes: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct StorageInfoData {
    pub backend_name: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub streams: usize,
    pub consumers: usize,
    #[serde(default)]
    pub buckets: Vec<BucketInfoData>,
}

pub struct AppState {
    pub client: reqwest::Client,
    pub base_url: std::sync::Mutex<String>,
}

pub fn get_base_url(state: &AppState) -> Result<String, String> {
    state
        .base_url
        .lock()
        .map(|guard| guard.clone())
        .map_err(|e| format!("Mutex poisoned: {e}"))
}
