use async_nats::jetstream::{self, context::Context, stream::Config as StreamConfig};
use async_nats::jetstream::object_store::Config as ObjectStoreConfig;
use async_nats::Client;
use futures::StreamExt;
use std::collections::HashMap;
use engine_core::engine::{ProcessInstance, PendingUserTask, PendingServiceTask};
use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{Token, ProcessDefinition};

use async_trait::async_trait;
use engine_core::persistence::WorkflowPersistence;
use serde::Serialize;

/// Information about the connected NATS server and JetStream account.
#[derive(Debug, Clone, Serialize)]
pub struct NatsInfo {
    pub server_name: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub max_payload: usize,
    /// JetStream memory usage in bytes.
    pub js_memory_bytes: u64,
    /// JetStream file storage usage in bytes.
    pub js_storage_bytes: u64,
    /// Number of active JetStream streams.
    pub js_streams: usize,
    /// Number of active JetStream consumers.
    pub js_consumers: usize,
}

#[derive(Clone)]
pub struct NatsPersistence {
    client: Client,
    js: Context,
    stream_name: String,
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

    async fn list_kv_entries<T: serde::de::DeserializeOwned>(&self, bucket: &str, entity_name: &str) -> EngineResult<Vec<T>> {
        let store = self.js.get_key_value(bucket).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get {bucket} KV: {}", e))
        })?;

        let mut keys = store.keys().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list {entity_name} keys: {}", e))
        });

        let mut entries = Vec::new();
        while let Ok(ref mut stream) = keys {
            match stream.next().await {
                Some(Ok(key)) => {
                    match store.get(&key).await {
                        Ok(Some(entry)) => {
                            match serde_json::from_slice::<T>(&entry) {
                                Ok(item) => entries.push(item),
                                Err(e) => log::warn!("Failed to deserialize {entity_name} '{}': {}", key, e),
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("Failed to get {entity_name} '{key}': {}", e),
                    }
                }
                Some(Err(e)) => log::warn!("Failed to stream {entity_name} key: {}", e),
                None => break,
            }
        }

        Ok(entries)
    }
}


#[async_trait]
impl WorkflowPersistence for NatsPersistence {
    async fn save_token(&self, token: &Token) -> EngineResult<()> {
        let subject = format!("{}.{}", self.stream_name, token.id);
        let payload = serde_json::to_vec(token).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize token: {}", e))
        })?;
        
        self.js
            .publish(subject, payload.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to publish to JetStream: {}", e))
            })?;
            
        Ok(())
    }

    async fn load_tokens(&self, _process_id: &str) -> EngineResult<Vec<Token>> {
        let stream = self.js.get_stream(&self.stream_name).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get stream: {}", e))
        })?;
        
        let consumer = stream.create_consumer(async_nats::jetstream::consumer::pull::Config {
            deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::All,
            ..Default::default()
        }).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to create consumer: {}", e))
        })?;
        
        let mut messages = consumer.messages().await.map_err(|e| {
            EngineError::PersistenceError(format!("Message stream error: {}", e))
        })?;
        
        let mut token_map = HashMap::new();
        
        while let Ok(Some(msg)) = tokio::time::timeout(std::time::Duration::from_millis(500), messages.next()).await {
            if let Ok(msg) = msg {
                let _ = msg.ack().await;
                if let Ok(token) = serde_json::from_slice::<Token>(&msg.payload) {
                    token_map.insert(token.id, token);
                }
            }
        }
        
        Ok(token_map.into_values().collect())
    }

    async fn save_instance(&self, instance: &ProcessInstance) -> EngineResult<()> {
        let store = self.js.get_key_value("instances").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instances KV: {}", e))
        })?;

        let json = serde_json::to_vec(instance).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize instance: {}", e))
        })?;

        store
            .put(instance.id.to_string(), json.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to put instance to KV: {}", e))
            })?;

        Ok(())
    }

    async fn list_instances(&self) -> EngineResult<Vec<ProcessInstance>> {
        self.list_kv_entries("instances", "instance").await
    }

    async fn delete_instance(&self, id: &str) -> EngineResult<()> {
        let store = self.js.get_key_value("instances").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instances KV: {}", e))
        })?;

        store
            .delete(id)
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to delete instance from KV: {}", e))
            })?;

        Ok(())
    }

    async fn save_definition(&self, definition: &ProcessDefinition) -> EngineResult<()> {
        let store = self.js.get_key_value("definitions").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get definitions KV: {}", e))
        })?;

        let json = serde_json::to_vec(definition).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize definition: {}", e))
        })?;

        store
            .put(definition.key.to_string(), json.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to put definition to KV: {}", e))
            })?;

        Ok(())
    }

    async fn list_definitions(&self) -> EngineResult<Vec<ProcessDefinition>> {
        self.list_kv_entries("definitions", "definition").await
    }

    async fn delete_definition(&self, key: &str) -> EngineResult<()> {
        let store = self.js.get_key_value("definitions").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get definitions KV: {}", e))
        })?;

        store
            .delete(key)
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to delete definition from KV: {}", e))
            })?;

        let obj_store = self.js.get_object_store("bpmn_xml").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get bpmn_xml Object Store: {}", e))
        })?;
        
        let _ = obj_store.delete(key).await;

        Ok(())
    }

    async fn save_user_task(&self, task: &PendingUserTask) -> EngineResult<()> {
        let store = self.js.get_key_value("user_tasks").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get user_tasks KV: {}", e))
        })?;

        let json = serde_json::to_vec(task).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize user task: {}", e))
        })?;

        store
            .put(task.task_id.to_string(), json.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to put user task to KV: {}", e))
            })?;

        Ok(())
    }

    async fn delete_user_task(&self, task_id: uuid::Uuid) -> EngineResult<()> {
        let store = self.js.get_key_value("user_tasks").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get user_tasks KV: {}", e))
        })?;

        store
            .delete(task_id.to_string())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to delete user task from KV: {}", e))
            })?;

        Ok(())
    }

    async fn list_user_tasks(&self) -> EngineResult<Vec<PendingUserTask>> {
        self.list_kv_entries("user_tasks", "user task").await
    }

    async fn save_service_task(&self, task: &PendingServiceTask) -> EngineResult<()> {
        let store = self.js.get_key_value("service_tasks").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get service_tasks KV: {}", e))
        })?;

        let json = serde_json::to_vec(task).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize service task: {}", e))
        })?;

        store
            .put(task.id.to_string(), json.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to put service task to KV: {}", e))
            })?;

        Ok(())
    }

    async fn delete_service_task(&self, task_id: uuid::Uuid) -> EngineResult<()> {
        let store = self.js.get_key_value("service_tasks").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get service_tasks KV: {}", e))
        })?;

        store
            .delete(task_id.to_string())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to delete service task from KV: {}", e))
            })?;

        Ok(())
    }

    async fn list_service_tasks(&self) -> EngineResult<Vec<PendingServiceTask>> {
        self.list_kv_entries("service_tasks", "service task").await
    }

    async fn save_bpmn_xml(&self, definition_id: &str, xml: &str) -> EngineResult<()> {
        let store = self.js.get_object_store("bpmn_xml").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get bpmn_xml Object Store: {}", e))
        })?;

        store
            .put(definition_id, &mut xml.as_bytes())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to store BPMN XML: {}", e))
            })?;

        Ok(())
    }

    async fn load_bpmn_xml(&self, definition_id: &str) -> EngineResult<String> {
        use tokio::io::AsyncReadExt;

        let store = self.js.get_object_store("bpmn_xml").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get bpmn_xml Object Store: {}", e))
        })?;

        let mut result = store.get(definition_id).await.map_err(|e| {
            EngineError::PersistenceError(format!(
                "Failed to load BPMN XML for '{}': {}",
                definition_id, e
            ))
        })?;

        let mut data = Vec::new();
        result.read_to_end(&mut data).await.map_err(|e| {
            EngineError::PersistenceError(format!("Error reading XML data: {}", e))
        })?;

        String::from_utf8(data).map_err(|e| {
            EngineError::PersistenceError(format!("BPMN XML is not valid UTF-8: {}", e))
        })
    }

    async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>> {
        let store = self.js.get_object_store("bpmn_xml").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get bpmn_xml Object Store: {}", e))
        })?;

        let mut list = store.list().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list bpmn_xml objects: {}", e))
        })?;

        let mut ids = Vec::new();
        while let Some(info) = list.next().await {
            if let Ok(info) = info {
                ids.push(info.name);
            }
        }
        Ok(ids)
    }

    async fn get_storage_info(&self) -> EngineResult<Option<engine_core::persistence::StorageInfo>> {
        let si = self.client.server_info();

        let account = self.js.query_account().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to query JetStream account: {}", e))
        })?;

        Ok(Some(engine_core::persistence::StorageInfo {
            backend_name: si.server_name.clone(),
            version: si.version.clone(),
            host: si.host.clone(),
            port: si.port,
            memory_bytes: account.memory,
            storage_bytes: account.storage,
            streams: account.streams,
            consumers: account.consumers,
        }))
    }

    async fn append_history_entry(&self, entry: &engine_core::history::HistoryEntry) -> EngineResult<()> {
        let subject = format!("history.instance.{}", entry.instance_id);
        let payload = serde_json::to_vec(entry).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize history entry: {}", e))
        })?;
        
        self.js
            .publish(subject, payload.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to publish history entry to JetStream: {}", e))
            })?;
            
        Ok(())
    }

    async fn query_history(&self, query: engine_core::persistence::HistoryQuery) -> EngineResult<Vec<engine_core::history::HistoryEntry>> {
        let stream = self.js.get_stream("WORKFLOW_HISTORY").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get WORKFLOW_HISTORY stream: {}", e))
        })?;
        
        let subject = format!("history.instance.{}", query.instance_id);
        
        let consumer = stream.create_consumer(async_nats::jetstream::consumer::pull::Config {
            deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::All,
            filter_subject: subject.clone(),
            ..Default::default()
        }).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to create history consumer: {}", e))
        })?;
        
        let mut messages = consumer.messages().await.map_err(|e| {
            EngineError::PersistenceError(format!("Message stream error: {}", e))
        })?;
        
        let mut entries = Vec::new();
        
        // Timeout to drain all existing messages
        while let Ok(Some(msg)) = tokio::time::timeout(std::time::Duration::from_millis(100), messages.next()).await {
            if let Ok(msg) = msg {
                let _ = msg.ack().await;
                if let Ok(entry) = serde_json::from_slice::<engine_core::history::HistoryEntry>(&msg.payload) {
                    let mut matched = true;
                    if let Some(types) = &query.event_types { if !types.contains(&entry.event_type) { matched = false; } }
                    if let Some(nid) = &query.node_id { if entry.node_id.as_deref() != Some(nid) { matched = false; } }
                    if let Some(aty) = &query.actor_type { if &entry.actor_type != aty { matched = false; } }
                    if let Some(f) = query.from { if entry.timestamp < f { matched = false; } }
                    if let Some(t) = query.to { if entry.timestamp > t { matched = false; } }
                    
                    if matched {
                        entries.push(entry);
                    }
                }
            }
        }
        
        // Ensure chronological order
        entries.sort_by_key(|e| e.timestamp);
        
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(entries.len());
        
        Ok(entries.into_iter().skip(offset).take(limit).collect())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;
    use uuid::Uuid;

    pub async fn setup_nats_test() -> Option<Arc<NatsPersistence>> {
        let url = "nats://localhost:4222";
        let stream = format!("TEST_STREAM_{}", Uuid::new_v4());
        
        match NatsPersistence::connect(url, &stream).await {
            Ok(persistence) => Some(Arc::new(persistence)),
            Err(e) => {
                log::warn!("Skipping NATS test, could not connect: {}", e);
                None
            }
        }
    }

    #[tokio::test]
    async fn test_save_and_load_token() {
        let persistence = match setup_nats_test().await {
            Some(p) => p,
            None => return, // Ignore if NATS container is not running
        };

        let mut token = Token::new("start_node");
        token.variables.insert("test_key".into(), serde_json::Value::String("test_value".into()));

        persistence.save_token(&token).await.unwrap();

        // Event-Sourcing Light Scenario
        token.current_node = "next_node".to_string();
        persistence.save_token(&token).await.unwrap();

        let loaded_tokens = persistence.load_tokens("some_process_id").await.unwrap();
        
        assert_eq!(loaded_tokens.len(), 1);
        let loaded_token = &loaded_tokens[0];
        
        assert_eq!(loaded_token.id, token.id);
        assert_eq!(loaded_token.current_node, "next_node");
        assert_eq!(loaded_token.variables.get("test_key").unwrap().as_str().unwrap(), "test_value");
    }

    #[tokio::test]
    async fn test_history_append_and_load() {
        let persistence = match setup_nats_test().await {
            Some(p) => p,
            None => return, // Ignore if NATS container is not running
        };

        let instance_id = Uuid::new_v4();
        let entry1 = engine_core::history::HistoryEntry::new(
            instance_id,
            engine_core::history::HistoryEventType::InstanceStarted,
            "Instance Started",
            engine_core::history::ActorType::Engine,
            None
        );

        let entry2 = engine_core::history::HistoryEntry::new(
            instance_id,
            engine_core::history::HistoryEventType::TokenAdvanced,
            "Token moved",
            engine_core::history::ActorType::Engine,
            None
        ).with_node("task_1");

        // Append to stream
        persistence.append_history_entry(&entry1).await.unwrap();
        persistence.append_history_entry(&entry2).await.unwrap();

        // Give NATS JetStream a tiny bit of time to flush/index
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let history = persistence.query_history(engine_core::persistence::HistoryQuery {
            instance_id,
            ..Default::default()
        }).await.unwrap();

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, entry1.id);
        assert_eq!(history[0].event_type, engine_core::history::HistoryEventType::InstanceStarted);
        assert_eq!(history[1].id, entry2.id);
        assert_eq!(history[1].event_type, engine_core::history::HistoryEventType::TokenAdvanced);
        assert_eq!(history[1].node_id.as_deref(), Some("task_1"));
    }
}
