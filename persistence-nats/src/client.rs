use async_nats::Client;
use async_nats::jetstream::object_store::Config as ObjectStoreConfig;
use async_nats::jetstream::{self, context::Context, stream::Config as StreamConfig};
use engine_core::error::{EngineError, EngineResult};
use futures::StreamExt;

#[derive(Clone)]
pub struct NatsPersistence {
    pub(crate) client: Client,
    pub(crate) js: Context,
    pub(crate) stream_name: String,
}

impl NatsPersistence {
    pub async fn connect(url: &str, stream_name: &str) -> EngineResult<Self> {
        let client = async_nats::connect(url).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to connect to NATS: {}", e))
        })?;

        let js = jetstream::new(client.clone());

        // Optional: Ensure the stream exists.
        // We ignore the error if it already exists.
        let _ = js
            .get_or_create_stream(StreamConfig {
                name: stream_name.to_string(),
                subjects: vec![format!("{}.*", stream_name)],
                ..Default::default()
            })
            .await;

        // Ensure the bpmn_xml Object Store bucket exists (per NATS rules).
        let _ = js
            .create_object_store(ObjectStoreConfig {
                bucket: "bpmn_xml".to_string(),
                description: Some("Original BPMN 2.0 XML artifacts".to_string()),
                ..Default::default()
            })
            .await;

        // Ensure the instance_files Object Store bucket exists.
        let _ = js
            .create_object_store(ObjectStoreConfig {
                bucket: "instance_files".to_string(),
                description: Some(
                    "Binary file attachments for process instance variables".to_string(),
                ),
                ..Default::default()
            })
            .await;

        // Ensure the instances KV bucket exists (per NATS rules).
        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "instances".to_string(),
                description: "ProcessInstance state (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        // Ensure the definitions KV bucket exists (per NATS rules).
        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "definitions".to_string(),
                description: "ProcessDefinition metadata (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        // Ensure the user_tasks KV bucket exists (per NATS rules).
        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "user_tasks".to_string(),
                description: "PendingUserTask objects (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        // Ensure the service_tasks KV bucket exists.
        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "service_tasks".to_string(),
                description: "PendingServiceTask objects (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "timers".to_string(),
                description: "PendingTimer objects (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "messages".to_string(),
                description: "PendingMessageCatch objects (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        // Ensure the history_instances KV bucket exists for archived completed instances.
        let _ = js
            .create_key_value(async_nats::jetstream::kv::Config {
                bucket: "history_instances".to_string(),
                description: "Archived completed ProcessInstance objects (JSON)".to_string(),
                ..Default::default()
            })
            .await;

        // Ensure the WORKFLOW_HISTORY JetStream stream exists for event-sourcing.
        let _ = js
            .get_or_create_stream(StreamConfig {
                name: "WORKFLOW_HISTORY".to_string(),
                subjects: vec!["history.instance.*".to_string()],
                ..Default::default()
            })
            .await;

        Ok(Self {
            client,
            js,
            stream_name: stream_name.to_string(),
        })
    }

    pub(crate) async fn list_kv_entries<T: serde::de::DeserializeOwned>(
        &self,
        bucket: &str,
        entity_name: &str,
    ) -> EngineResult<Vec<T>> {
        let store = self.js.get_key_value(bucket).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get {bucket} KV: {}", e))
        })?;

        let mut keys = store.keys().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list {entity_name} keys: {}", e))
        });

        let mut entries = Vec::new();
        while let Ok(ref mut stream) = keys {
            match stream.next().await {
                Some(Ok(key)) => match store.get(&key).await {
                    Ok(Some(entry)) => match serde_json::from_slice::<T>(&entry) {
                        Ok(item) => entries.push(item),
                        Err(e) => {
                            tracing::warn!("Failed to deserialize {entity_name} '{}': {}", key, e)
                        }
                    },
                    Ok(None) => {}
                    Err(e) => tracing::warn!("Failed to get {entity_name} '{key}': {}", e),
                },
                Some(Err(e)) => tracing::warn!("Failed to stream {entity_name} key: {}", e),
                None => break,
            }
        }

        Ok(entries)
    }
}
