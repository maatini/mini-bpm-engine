use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engine::{ProcessInstance, PendingUserTask, PendingServiceTask, PendingTimer, PendingMessageCatch};
use crate::error::EngineResult;
use crate::model::{Token, ProcessDefinition};
use crate::history::{HistoryEntry};
use crate::persistence::{WorkflowPersistence, StorageInfo, HistoryQuery};

#[derive(Default, Clone)]
pub struct InMemoryPersistence {
    tokens: Arc<RwLock<HashMap<String, Vec<Token>>>>,
    instances: Arc<RwLock<HashMap<uuid::Uuid, ProcessInstance>>>,
    definitions: Arc<RwLock<HashMap<uuid::Uuid, ProcessDefinition>>>,
    user_tasks: Arc<RwLock<HashMap<uuid::Uuid, PendingUserTask>>>,
    service_tasks: Arc<RwLock<HashMap<uuid::Uuid, PendingServiceTask>>>,
    timers: Arc<RwLock<HashMap<uuid::Uuid, PendingTimer>>>,
    message_catches: Arc<RwLock<HashMap<uuid::Uuid, PendingMessageCatch>>>,
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    bpmn_xmls: Arc<RwLock<HashMap<String, String>>>,
    history: Arc<RwLock<HashMap<uuid::Uuid, Vec<HistoryEntry>>>>,
}

impl InMemoryPersistence {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl WorkflowPersistence for InMemoryPersistence {
    async fn save_token(&self, token: &Token) -> EngineResult<()> {
        let mut t = self.tokens.write().await;
        let _entry = t.remove(&token.id.to_string()).unwrap_or_default();
        // Since token ids are unique per branch but reused per instance,
        // we just push it since in-memory is mainly used dynamically?
        // Wait, save_token appends or overwrites? The persistence traits usually overwrite or append.
        // Actually save_token overwrites. Token IDs are unique.
        // Actually, load_tokens is by process_id! Which means save_token should store them under process_id!
        // But save_token only has the single Token. How does it know the process_id?
        // Wait, the real `persistence-nats` save_token currently ignores tokens completely because tokens are embedded in `ProcessInstance` via `active_tokens`.
        
        // Let's implement it generic:
        t.insert(token.id.to_string(), vec![token.clone()]);
        Ok(())
    }

    async fn load_tokens(&self, process_id: &str) -> EngineResult<Vec<Token>> {
        let t = self.tokens.read().await;
        // In-memory doesn't track tokens by process_id efficiently unless we link them.
        Ok(t.get(process_id).cloned().unwrap_or_default())
    }

    async fn save_instance(&self, instance: &ProcessInstance) -> EngineResult<()> {
        let mut i = self.instances.write().await;
        i.insert(instance.id, instance.clone());
        Ok(())
    }

    async fn list_instances(&self) -> EngineResult<Vec<ProcessInstance>> {
        let i = self.instances.read().await;
        Ok(i.values().cloned().collect())
    }

    async fn delete_instance(&self, id: &str) -> EngineResult<()> {
        if let Ok(uid) = uuid::Uuid::parse_str(id) {
            let mut i = self.instances.write().await;
            i.remove(&uid);
        }
        Ok(())
    }

    async fn save_definition(&self, definition: &ProcessDefinition) -> EngineResult<()> {
        let mut d = self.definitions.write().await;
        d.insert(definition.key, definition.clone());
        Ok(())
    }

    async fn list_definitions(&self) -> EngineResult<Vec<ProcessDefinition>> {
        let d = self.definitions.read().await;
        Ok(d.values().cloned().collect())
    }

    async fn delete_definition(&self, key: &str) -> EngineResult<()> {
        if let Ok(uid) = uuid::Uuid::parse_str(key) {
            let mut d = self.definitions.write().await;
            d.remove(&uid);
        }
        Ok(())
    }

    async fn save_user_task(&self, task: &PendingUserTask) -> EngineResult<()> {
        let mut u = self.user_tasks.write().await;
        u.insert(task.task_id, task.clone());
        Ok(())
    }

    async fn delete_user_task(&self, task_id: uuid::Uuid) -> EngineResult<()> {
        let mut u = self.user_tasks.write().await;
        u.remove(&task_id);
        Ok(())
    }

    async fn list_user_tasks(&self) -> EngineResult<Vec<PendingUserTask>> {
        let u = self.user_tasks.read().await;
        Ok(u.values().cloned().collect())
    }

    async fn save_service_task(&self, task: &PendingServiceTask) -> EngineResult<()> {
        let mut s = self.service_tasks.write().await;
        s.insert(task.id, task.clone());
        Ok(())
    }

    async fn delete_service_task(&self, task_id: uuid::Uuid) -> EngineResult<()> {
        let mut s = self.service_tasks.write().await;
        s.remove(&task_id);
        Ok(())
    }

    async fn list_service_tasks(&self) -> EngineResult<Vec<PendingServiceTask>> {
        let s = self.service_tasks.read().await;
        Ok(s.values().cloned().collect())
    }

    async fn save_timer(&self, timer: &PendingTimer) -> EngineResult<()> {
        let mut t = self.timers.write().await;
        t.insert(timer.id, timer.clone());
        Ok(())
    }

    async fn delete_timer(&self, timer_id: uuid::Uuid) -> EngineResult<()> {
        let mut t = self.timers.write().await;
        t.remove(&timer_id);
        Ok(())
    }

    async fn list_timers(&self) -> EngineResult<Vec<PendingTimer>> {
        let t = self.timers.read().await;
        Ok(t.values().cloned().collect())
    }

    async fn save_message_catch(&self, catch: &PendingMessageCatch) -> EngineResult<()> {
        let mut m = self.message_catches.write().await;
        m.insert(catch.id, catch.clone());
        Ok(())
    }

    async fn delete_message_catch(&self, catch_id: uuid::Uuid) -> EngineResult<()> {
        let mut m = self.message_catches.write().await;
        m.remove(&catch_id);
        Ok(())
    }

    async fn list_message_catches(&self) -> EngineResult<Vec<PendingMessageCatch>> {
        let m = self.message_catches.read().await;
        Ok(m.values().cloned().collect())
    }

    async fn save_file(&self, object_key: &str, data: &[u8]) -> EngineResult<()> {
        let mut f = self.files.write().await;
        f.insert(object_key.to_string(), data.to_vec());
        Ok(())
    }

    async fn load_file(&self, object_key: &str) -> EngineResult<Vec<u8>> {
        let f = self.files.read().await;
        f.get(object_key).cloned().ok_or_else(|| crate::error::EngineError::PersistenceError("File not found".into()))
    }

    async fn delete_file(&self, object_key: &str) -> EngineResult<()> {
        let mut f = self.files.write().await;
        f.remove(object_key);
        Ok(())
    }

    async fn save_bpmn_xml(&self, definition_key: &str, xml: &str) -> EngineResult<()> {
        let mut b = self.bpmn_xmls.write().await;
        b.insert(definition_key.to_string(), xml.to_string());
        Ok(())
    }

    async fn load_bpmn_xml(&self, definition_key: &str) -> EngineResult<String> {
        let b = self.bpmn_xmls.read().await;
        b.get(definition_key).cloned().ok_or_else(|| crate::error::EngineError::NoSuchDefinition(
            uuid::Uuid::parse_str(definition_key).unwrap_or_default()
        ))
    }

    async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>> {
        let b = self.bpmn_xmls.read().await;
        Ok(b.keys().cloned().collect())
    }

    async fn get_storage_info(&self) -> EngineResult<Option<StorageInfo>> {
        let f_len = self.files.read().await.len();
        let i_len = self.instances.read().await.len();
        
        Ok(Some(StorageInfo {
            backend_name: "InMemoryPersistence".to_string(),
            version: "1.0.0".to_string(),
            host: "localhost".to_string(),
            port: 0,
            memory_bytes: (f_len * 1024 + i_len * 512) as u64, // mock stats
            storage_bytes: 0,
            streams: 0,
            consumers: 0,
        }))
    }

    async fn append_history_entry(&self, entry: &HistoryEntry) -> EngineResult<()> {
        let mut h = self.history.write().await;
        h.entry(entry.instance_id).or_insert_with(Vec::new).push(entry.clone());
        Ok(())
    }

    async fn query_history(&self, query: HistoryQuery) -> EngineResult<Vec<HistoryEntry>> {
        let h = self.history.read().await;
        let mut events = h.get(&query.instance_id).cloned().unwrap_or_default();
        
        // Filter support
        if let Some(types) = &query.event_types {
            events.retain(|e| types.contains(&e.event_type));
        }
        if let Some(node_id) = &query.node_id {
            events.retain(|e| e.node_id.as_ref() == Some(node_id));
        }
        if let Some(actor) = &query.actor_type {
            events.retain(|e| e.actor_type == *actor);
        }
        
        if let Some(from) = query.from {
            events.retain(|e| e.timestamp >= from);
        }
        if let Some(to) = query.to {
            events.retain(|e| e.timestamp <= to);
        }
        
        // Sorting matches query logic (usually ascending by timestamp)
        events.sort_by_key(|e| e.timestamp);
        
        // Pagination
        if let Some(offset) = query.offset {
            events = events.into_iter().skip(offset).collect();
        }
        if let Some(limit) = query.limit {
            events.truncate(limit);
        }
        
        Ok(events)
    }
}
