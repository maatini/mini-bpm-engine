use async_nats::jetstream::{self, context::Context, stream::Config as StreamConfig};
use async_nats::jetstream::object_store::Config as ObjectStoreConfig;
use async_nats::Client;
use futures::StreamExt;
use std::collections::HashMap;
use engine_core::engine::{ProcessInstance, PendingUserTask};
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
            
        Ok(Self {
            client,
            js,
            stream_name: stream_name.to_string(),
        })
    }

    /// Stores the original BPMN XML in the `bpmn_xml` Object Store bucket.
    pub async fn save_bpmn_xml(&self, definition_id: &str, xml: &str) -> EngineResult<()> {
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

    /// Loads the original BPMN XML from the `bpmn_xml` Object Store bucket.
    pub async fn load_bpmn_xml(&self, definition_id: &str) -> EngineResult<String> {
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

    /// Returns monitoring information about the connected NATS server
    /// and JetStream account.
    pub async fn get_nats_info(&self) -> EngineResult<NatsInfo> {
        let si = self.client.server_info();

        let account = self.js.query_account().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to query JetStream account: {}", e))
        })?;

        Ok(NatsInfo {
            server_name: si.server_name.clone(),
            version: si.version.clone(),
            host: si.host.clone(),
            port: si.port,
            max_payload: si.max_payload,
            js_memory_bytes: account.memory,
            js_storage_bytes: account.storage,
            js_streams: account.streams,
            js_consumers: account.consumers,
        })
    }
    /// Lists all definition IDs currently stored in the `bpmn_xml` Object Store.
    pub async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>> {
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
        let store = self.js.get_key_value("instances").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instances KV: {}", e))
        })?;

        let mut keys = store.keys().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list instance keys: {}", e))
        });

        let mut instances = Vec::new();
        while let Ok(ref mut stream) = keys {
            match stream.next().await {
                Some(Ok(key)) => {
                    match store.get(&key).await {
                        Ok(Some(entry)) => {
                            match serde_json::from_slice::<ProcessInstance>(&entry) {
                                Ok(inst) => instances.push(inst),
                                Err(e) => log::warn!("Failed to deserialize instance '{}': {}", key, e),
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("Failed to get instance '{}': {}", key, e),
                    }
                }
                Some(Err(e)) => log::warn!("Failed to stream instance key: {}", e),
                None => break,
            }
        }

        Ok(instances)
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
        let store = self.js.get_key_value("definitions").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get definitions KV: {}", e))
        })?;

        let mut keys = store.keys().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list definition keys: {}", e))
        });

        let mut defs = Vec::new();
        while let Ok(ref mut stream) = keys {
            match stream.next().await {
                Some(Ok(key)) => {
                    match store.get(&key).await {
                        Ok(Some(entry)) => {
                            match serde_json::from_slice::<ProcessDefinition>(&entry) {
                                Ok(def) => defs.push(def),
                                Err(e) => log::warn!("Failed to deserialize definition '{}': {}", key, e),
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("Failed to get definition '{}': {}", key, e),
                    }
                }
                Some(Err(e)) => log::warn!("Failed to stream definition key: {}", e),
                None => break,
            }
        }

        Ok(defs)
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
        let store = self.js.get_key_value("user_tasks").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get user_tasks KV: {}", e))
        })?;

        let mut keys = store.keys().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to list user task keys: {}", e))
        });

        let mut tasks = Vec::new();
        while let Ok(ref mut stream) = keys {
            match stream.next().await {
                Some(Ok(key)) => {
                    match store.get(&key).await {
                        Ok(Some(entry)) => {
                            match serde_json::from_slice::<PendingUserTask>(&entry) {
                                Ok(task) => tasks.push(task),
                                Err(e) => log::warn!("Failed to deserialize user task '{}': {}", key, e),
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("Failed to get user task '{}': {}", key, e),
                    }
                }
                Some(Err(e)) => log::warn!("Failed to stream user task key: {}", e),
                None => break,
            }
        }

        Ok(tasks)
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
}
