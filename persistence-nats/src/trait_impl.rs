use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use engine_core::engine::{ProcessInstance, PendingUserTask, PendingServiceTask};
use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{Token, ProcessDefinition};
use engine_core::persistence::WorkflowPersistence;

use crate::client::NatsPersistence;

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
            ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
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

    async fn save_timer(&self, timer: &engine_core::engine::PendingTimer) -> EngineResult<()> {
        let store = self.js.get_key_value("timers").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get timers KV: {}", e))
        })?;

        let json = serde_json::to_vec(timer).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize timer: {}", e))
        })?;

        store.put(timer.id.to_string(), json.into()).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to put timer to KV: {}", e))
        })?;

        Ok(())
    }

    async fn delete_timer(&self, timer_id: uuid::Uuid) -> EngineResult<()> {
        let store = self.js.get_key_value("timers").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get timers KV: {}", e))
        })?;

        store.delete(timer_id.to_string()).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to delete timer from KV: {}", e))
        })?;

        Ok(())
    }

    async fn list_timers(&self) -> EngineResult<Vec<engine_core::engine::PendingTimer>> {
        self.list_kv_entries("timers", "timer").await
    }

    async fn save_message_catch(&self, catch: &engine_core::engine::PendingMessageCatch) -> EngineResult<()> {
        let store = self.js.get_key_value("messages").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get messages KV: {}", e))
        })?;

        let json = serde_json::to_vec(catch).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize message catch: {}", e))
        })?;

        store.put(catch.id.to_string(), json.into()).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to put message catch to KV: {}", e))
        })?;

        Ok(())
    }

    async fn delete_message_catch(&self, catch_id: uuid::Uuid) -> EngineResult<()> {
        let store = self.js.get_key_value("messages").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get messages KV: {}", e))
        })?;

        store.delete(catch_id.to_string()).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to delete message catch from KV: {}", e))
        })?;

        Ok(())
    }

    async fn list_message_catches(&self) -> EngineResult<Vec<engine_core::engine::PendingMessageCatch>> {
        self.list_kv_entries("messages", "message catch").await
    }

    async fn save_file(&self, object_key: &str, data: &[u8]) -> EngineResult<()> {
        const MAX_FILE_SIZE: usize = 50 * 1024 * 1024; // 50 MB
        if data.len() > MAX_FILE_SIZE {
            return Err(EngineError::InvalidDefinition(
                format!("File exceeds maximum size of 50 MB ({} bytes)", data.len())
            ));
        }
        let store = self.js.get_object_store("instance_files").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instance_files Object Store: {}", e))
        })?;
        let mut data_mut = data;
        store.put(object_key, &mut data_mut).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to save file: {}", e))
        })?;
        Ok(())
    }

    async fn load_file(&self, object_key: &str) -> EngineResult<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let store = self.js.get_object_store("instance_files").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instance_files Object Store: {}", e))
        })?;
        let mut result = store.get(object_key).await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to load file '{}': {}", object_key, e))
        })?;
        let mut data = Vec::new();
        result.read_to_end(&mut data).await.map_err(|e| {
            EngineError::PersistenceError(format!("Error reading file data: {}", e))
        })?;
        Ok(data)
    }

    async fn delete_file(&self, object_key: &str) -> EngineResult<()> {
        let store = self.js.get_object_store("instance_files").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get instance_files Object Store: {}", e))
        })?;
        let _ = store.delete(object_key).await;
        Ok(())
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

        let mut buckets = Vec::new();

        // Collect KV bucket stats
        let kv_bucket_names = ["instances", "definitions", "user_tasks", "service_tasks", "timers", "messages"];
        for name in &kv_bucket_names {
            match self.js.get_key_value(*name).await {
                Ok(store) => {
                    match store.status().await {
                        Ok(status) => {
                            buckets.push(engine_core::persistence::BucketInfo {
                                name: name.to_string(),
                                bucket_type: "kv".to_string(),
                                entries: status.values(),
                                size_bytes: status.info.state.bytes,
                            });
                        }
                        Err(e) => tracing::warn!("Failed to get status for KV bucket '{}': {}", name, e),
                    }
                }
                Err(e) => tracing::warn!("Failed to access KV bucket '{}': {}", name, e),
            }
        }

        // Collect ObjectStore bucket stats
        let obj_bucket_names = ["bpmn_xml", "instance_files"];
        for name in &obj_bucket_names {
            let stream_name = format!("OBJ_{}", name);
            match self.js.get_stream(&stream_name).await {
                Ok(mut stream) => {
                    match stream.info().await {
                        Ok(info) => {
                            buckets.push(engine_core::persistence::BucketInfo {
                                name: name.to_string(),
                                bucket_type: "object_store".to_string(),
                                entries: info.state.messages,
                                size_bytes: info.state.bytes,
                            });
                        }
                        Err(e) => tracing::warn!("Failed to get info for underlying ObjectStore stream '{}': {}", name, e),
                    }
                }
                Err(e) => tracing::warn!("Failed to access underlying ObjectStore stream '{}': {}", name, e),
            }
        }

        // Collect Stream stats
        let stream_names = ["WORKFLOW_EVENTS", "WORKFLOW_HISTORY"];
        for name in &stream_names {
            match self.js.get_stream(*name).await {
                Ok(mut stream) => {
                    match stream.info().await {
                        Ok(info) => {
                            buckets.push(engine_core::persistence::BucketInfo {
                                name: name.to_string(),
                                bucket_type: "stream".to_string(),
                                entries: info.state.messages,
                                size_bytes: info.state.bytes,
                            });
                        }
                        Err(e) => tracing::warn!("Failed to get info for stream '{}': {}", name, e),
                    }
                }
                Err(e) => tracing::warn!("Failed to access stream '{}': {}", name, e),
            }
        }

        Ok(Some(engine_core::persistence::StorageInfo {
            backend_name: si.server_name.clone(),
            version: si.version.clone(),
            host: si.host.clone(),
            port: si.port,
            memory_bytes: account.memory,
            storage_bytes: account.storage,
            streams: account.streams,
            consumers: account.consumers,
            buckets,
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
            ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
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
