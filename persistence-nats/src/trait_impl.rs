use async_trait::async_trait;
use futures::StreamExt;
use std::collections::HashMap;

use engine_core::engine::{PendingServiceTask, PendingUserTask, ProcessInstance};
use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{ProcessDefinition, Token};
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
        let stream =
            self.js.get_stream(&self.stream_name).await.map_err(|e| {
                EngineError::PersistenceError(format!("Failed to get stream: {}", e))
            })?;

        let consumer = stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::All,
                ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
                ..Default::default()
            })
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to create consumer: {}", e))
            })?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| EngineError::PersistenceError(format!("Message stream error: {}", e)))?;

        let mut token_map = HashMap::new();

        while let Ok(Some(msg)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), messages.next()).await
        {
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

        store.delete(id).await.map_err(|e| {
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

        store.delete(key).await.map_err(|e| {
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

        store.delete(task_id.to_string()).await.map_err(|e| {
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

        store.delete(task_id.to_string()).await.map_err(|e| {
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

        store
            .put(timer.id.to_string(), json.into())
            .await
            .map_err(|e| {
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

    async fn save_message_catch(
        &self,
        catch: &engine_core::engine::PendingMessageCatch,
    ) -> EngineResult<()> {
        let store = self.js.get_key_value("messages").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get messages KV: {}", e))
        })?;

        let json = serde_json::to_vec(catch).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize message catch: {}", e))
        })?;

        store
            .put(catch.id.to_string(), json.into())
            .await
            .map_err(|e| {
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

    async fn list_message_catches(
        &self,
    ) -> EngineResult<Vec<engine_core::engine::PendingMessageCatch>> {
        self.list_kv_entries("messages", "message catch").await
    }

    async fn save_file(&self, object_key: &str, data: &[u8]) -> EngineResult<()> {
        const MAX_FILE_SIZE: usize = 50 * 1024 * 1024; // 50 MB
        if data.len() > MAX_FILE_SIZE {
            return Err(EngineError::InvalidDefinition(format!(
                "File exceeds maximum size of 50 MB ({} bytes)",
                data.len()
            )));
        }
        let store = self
            .js
            .get_object_store("instance_files")
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get instance_files Object Store: {}",
                    e
                ))
            })?;
        let mut data_mut = data;
        store
            .put(object_key, &mut data_mut)
            .await
            .map_err(|e| EngineError::PersistenceError(format!("Failed to save file: {}", e)))?;
        Ok(())
    }

    async fn load_file(&self, object_key: &str) -> EngineResult<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let store = self
            .js
            .get_object_store("instance_files")
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get instance_files Object Store: {}",
                    e
                ))
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
        let store = self
            .js
            .get_object_store("instance_files")
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get instance_files Object Store: {}",
                    e
                ))
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
        result
            .read_to_end(&mut data)
            .await
            .map_err(|e| EngineError::PersistenceError(format!("Error reading XML data: {}", e)))?;

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

    async fn get_storage_info(
        &self,
    ) -> EngineResult<Option<engine_core::persistence::StorageInfo>> {
        let si = self.client.server_info();
        let account = self.js.query_account().await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to query JetStream account: {}", e))
        })?;

        let mut buckets = Vec::new();

        // Collect KV bucket stats
        let kv_bucket_names = [
            "instances",
            "definitions",
            "user_tasks",
            "service_tasks",
            "timers",
            "messages",
        ];
        for name in &kv_bucket_names {
            match self.js.get_key_value(*name).await {
                Ok(store) => match store.status().await {
                    Ok(status) => {
                        buckets.push(engine_core::persistence::BucketInfo {
                            name: name.to_string(),
                            bucket_type: "kv".to_string(),
                            entries: status.values(),
                            size_bytes: status.info.state.bytes,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get status for KV bucket '{}': {}", name, e)
                    }
                },
                Err(e) => tracing::warn!("Failed to access KV bucket '{}': {}", name, e),
            }
        }

        // Collect ObjectStore bucket stats
        let obj_bucket_names = ["bpmn_xml", "instance_files"];
        for name in &obj_bucket_names {
            let stream_name = format!("OBJ_{}", name);
            match self.js.get_stream(&stream_name).await {
                Ok(mut stream) => match stream.info().await {
                    Ok(info) => {
                        buckets.push(engine_core::persistence::BucketInfo {
                            name: name.to_string(),
                            bucket_type: "object_store".to_string(),
                            entries: info.state.messages,
                            size_bytes: info.state.bytes,
                        });
                    }
                    Err(e) => tracing::warn!(
                        "Failed to get info for underlying ObjectStore stream '{}': {}",
                        name,
                        e
                    ),
                },
                Err(e) => tracing::warn!(
                    "Failed to access underlying ObjectStore stream '{}': {}",
                    name,
                    e
                ),
            }
        }

        // Collect Stream stats
        let stream_names = ["WORKFLOW_EVENTS", "WORKFLOW_HISTORY"];
        for name in &stream_names {
            match self.js.get_stream(*name).await {
                Ok(mut stream) => match stream.info().await {
                    Ok(info) => {
                        buckets.push(engine_core::persistence::BucketInfo {
                            name: name.to_string(),
                            bucket_type: "stream".to_string(),
                            entries: info.state.messages,
                            size_bytes: info.state.bytes,
                        });
                    }
                    Err(e) => tracing::warn!("Failed to get info for stream '{}': {}", name, e),
                },
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

    async fn append_history_entry(
        &self,
        entry: &engine_core::history::HistoryEntry,
    ) -> EngineResult<()> {
        let subject = format!("history.instance.{}", entry.instance_id);
        let payload = serde_json::to_vec(entry).map_err(|e| {
            EngineError::PersistenceError(format!("Failed to serialize history entry: {}", e))
        })?;

        self.js
            .publish(subject, payload.into())
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to publish history entry to JetStream: {}",
                    e
                ))
            })?;

        Ok(())
    }

    async fn query_history(
        &self,
        query: engine_core::persistence::HistoryQuery,
    ) -> EngineResult<Vec<engine_core::history::HistoryEntry>> {
        let stream = self.js.get_stream("WORKFLOW_HISTORY").await.map_err(|e| {
            EngineError::PersistenceError(format!("Failed to get WORKFLOW_HISTORY stream: {}", e))
        })?;

        let subject = format!("history.instance.{}", query.instance_id);

        let consumer = stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::All,
                ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
                filter_subject: subject.clone(),
                ..Default::default()
            })
            .await
            .map_err(|e| {
                EngineError::PersistenceError(format!("Failed to create history consumer: {}", e))
            })?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| EngineError::PersistenceError(format!("Message stream error: {}", e)))?;

        let mut entries = Vec::new();

        // Timeout to drain all existing messages
        while let Ok(Some(msg)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), messages.next()).await
        {
            if let Ok(msg) = msg {
                if let Ok(entry) =
                    serde_json::from_slice::<engine_core::history::HistoryEntry>(&msg.payload)
                {
                    let mut matched = true;
                    if let Some(types) = &query.event_types {
                        if !types.contains(&entry.event_type) {
                            matched = false;
                        }
                    }
                    if let Some(nid) = &query.node_id {
                        if entry.node_id.as_deref() != Some(nid) {
                            matched = false;
                        }
                    }
                    if let Some(aty) = &query.actor_type {
                        if &entry.actor_type != aty {
                            matched = false;
                        }
                    }
                    if let Some(f) = query.from {
                        if entry.timestamp < f {
                            matched = false;
                        }
                    }
                    if let Some(t) = query.to {
                        if entry.timestamp > t {
                            matched = false;
                        }
                    }

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

    async fn get_bucket_entries(
        &self,
        bucket_name: &str,
        offset: usize,
        limit: usize,
    ) -> EngineResult<Vec<engine_core::persistence::BucketEntry>> {
        use engine_core::persistence::BucketEntry;
        let mut entries = Vec::new();

        let obj_bucket_names = ["bpmn_xml", "instance_files"];
        let kv_bucket_names = [
            "instances",
            "definitions",
            "user_tasks",
            "service_tasks",
            "timers",
            "messages",
        ];

        if kv_bucket_names.contains(&bucket_name) {
            let store = self.js.get_key_value(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!("Failed to get {} KV: {}", bucket_name, e))
            })?;

            let mut keys = store.keys().await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to list KV keys for {}: {}",
                    bucket_name, e
                ))
            })?;

            let mut all_keys = Vec::new();
            while let Some(Ok(key)) = keys.next().await {
                all_keys.push(key);
            }
            // Sort to ensure deterministic pagination across multiple calls
            all_keys.sort();

            let page_keys = all_keys
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect::<Vec<_>>();
            for key in page_keys {
                if let Ok(Some(entry)) = store.get(&key).await {
                    entries.push(BucketEntry {
                        key,
                        size_bytes: Some(entry.len() as u64),
                        created_at: Some(chrono::Utc::now()), // KV get doesn't expose metadata like created_at directly easily here
                    });
                } else {
                    entries.push(BucketEntry {
                        key,
                        size_bytes: None,
                        created_at: None,
                    });
                }
            }
        } else if obj_bucket_names.contains(&bucket_name) {
            let store = self.js.get_object_store(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get {} Object Store: {}",
                    bucket_name, e
                ))
            })?;

            let mut list = store.list().await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to list Object Store {}: {}",
                    bucket_name, e
                ))
            })?;

            let mut metadata_list = Vec::new();
            while let Some(Ok(info)) = list.next().await {
                metadata_list.push(info);
            }
            // Sort by name
            metadata_list.sort_by(|a, b| a.name.cmp(&b.name));

            for info in metadata_list.into_iter().skip(offset).take(limit) {
                let Some(dt) = info.modified else {
                    entries.push(BucketEntry {
                        key: info.name,
                        size_bytes: Some(info.size as u64),
                        created_at: None,
                    });
                    continue;
                };

                let nt = chrono::DateTime::from_timestamp(dt.unix_timestamp(), dt.nanosecond());
                entries.push(BucketEntry {
                    key: info.name,
                    size_bytes: Some(info.size as u64),
                    created_at: nt,
                });
            }
        } else if bucket_name == "WORKFLOW_HISTORY" || bucket_name == "WORKFLOW_EVENTS" {
            // For Streams, just dump latest messages sequence
            let mut stream = self.js.get_stream(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get stream {}: {}",
                    bucket_name, e
                ))
            })?;

            // To properly do this for streams genericly, we get the state
            let info = stream.info().await.map_err(|e| {
                EngineError::PersistenceError(format!("Failed to get stream info: {}", e))
            })?;

            // Just simulate keys based on first/last sequence
            let start = info.state.first_sequence.saturating_add(offset as u64);
            let last_seq = info.state.last_sequence;
            let _ = info; // Free any borrow

            let mut current = start;
            let mut count = 0;

            while count < limit && current <= last_seq {
                if let Ok(msg) = stream.get_raw_message(current).await {
                    entries.push(BucketEntry {
                        key: current.to_string(),
                        size_bytes: Some(msg.payload.len() as u64),
                        created_at: Some(
                            chrono::DateTime::from_timestamp(
                                msg.time.unix_timestamp(),
                                msg.time.nanosecond(),
                            )
                            .unwrap_or_else(chrono::Utc::now),
                        ),
                    });
                }
                current += 1;
                count += 1;
            }
        } else {
            return Err(EngineError::PersistenceError(format!(
                "Unknown bucket name: {}",
                bucket_name
            )));
        }

        Ok(entries)
    }

    async fn get_bucket_entry_detail(
        &self,
        bucket_name: &str,
        key: &str,
    ) -> EngineResult<engine_core::persistence::BucketEntryDetail> {
        use engine_core::persistence::BucketEntryDetail;

        let obj_bucket_names = ["bpmn_xml", "instance_files"];
        let kv_bucket_names = [
            "instances",
            "definitions",
            "user_tasks",
            "service_tasks",
            "timers",
            "messages",
        ];

        if kv_bucket_names.contains(&bucket_name) {
            let store = self.js.get_key_value(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!("Failed to get {} KV: {}", bucket_name, e))
            })?;

            if let Ok(Some(entry)) = store.get(key).await {
                let data = match String::from_utf8(entry.to_vec()) {
                    Ok(s) => s,
                    Err(_) => "Binary Data".to_string(), // Fallback if somehow not UTF8 json
                };
                return Ok(BucketEntryDetail {
                    key: key.to_string(),
                    data,
                });
            } else {
                return Err(EngineError::PersistenceError(format!(
                    "Key {} not found in KV {}",
                    key, bucket_name
                )));
            }
        } else if obj_bucket_names.contains(&bucket_name) {
            let store = self.js.get_object_store(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get {} Object Store: {}",
                    bucket_name, e
                ))
            })?;

            use tokio::io::AsyncReadExt;
            match store.get(key).await {
                Ok(mut result) => {
                    let mut data = Vec::new();
                    result.read_to_end(&mut data).await.map_err(|e| {
                        EngineError::PersistenceError(format!("Error reading object data: {}", e))
                    })?;

                    if bucket_name == "bpmn_xml" {
                        let xml = String::from_utf8(data)
                            .unwrap_or_else(|_| "Invalid UTF-8 XML".to_string());
                        Ok(BucketEntryDetail {
                            key: key.to_string(),
                            data: xml,
                        })
                    } else {
                        // Return Base64 for raw files
                        use base64::{Engine as _, engine::general_purpose::STANDARD};
                        let b64 = STANDARD.encode(data);
                        Ok(BucketEntryDetail {
                            key: key.to_string(),
                            data: b64,
                        })
                    }
                }
                Err(e) => Err(EngineError::PersistenceError(format!(
                    "Key {} not found in Object Store {}: {}",
                    key, bucket_name, e
                ))),
            }
        } else if bucket_name == "WORKFLOW_HISTORY" || bucket_name == "WORKFLOW_EVENTS" {
            let stream = self.js.get_stream(bucket_name).await.map_err(|e| {
                EngineError::PersistenceError(format!(
                    "Failed to get stream {}: {}",
                    bucket_name, e
                ))
            })?;

            let seq: u64 = key.parse().map_err(|_| {
                EngineError::PersistenceError(format!("Invalid sequence ID {}", key))
            })?;

            match stream.get_raw_message(seq).await {
                Ok(msg) => {
                    let data = match String::from_utf8(msg.payload.to_vec()) {
                        Ok(s) => s,
                        Err(_) => "Binary Message".to_string(),
                    };
                    Ok(BucketEntryDetail {
                        key: key.to_string(),
                        data,
                    })
                }
                Err(e) => Err(EngineError::PersistenceError(format!(
                    "Sequence {} not found in stream {}: {}",
                    key, bucket_name, e
                ))),
            }
        } else {
            Err(EngineError::PersistenceError(format!(
                "Unknown bucket name: {}",
                bucket_name
            )))
        }
    }
}
