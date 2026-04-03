use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};

use crate::engine::{ProcessInstance, PendingUserTask, PendingServiceTask};
use crate::error::EngineResult;
use crate::model::{Token, ProcessDefinition};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryQuery {
    pub instance_id: uuid::Uuid,
    pub event_types: Option<Vec<crate::history::HistoryEventType>>,
    pub node_id: Option<String>,
    pub actor_type: Option<crate::history::ActorType>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Generic storage backend information for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub backend_name: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub streams: usize,
    pub consumers: usize,
}

/// A trait for persisting workflow engine state.
#[async_trait]
pub trait WorkflowPersistence: Send + Sync {
    /// Save a token's state for a given process instance.
    async fn save_token(&self, token: &Token) -> EngineResult<()>;
    /// Load all tokens for a given process instance.
    async fn load_tokens(&self, process_id: &str) -> EngineResult<Vec<Token>>;

    /// Persist the current state of a process instance.
    async fn save_instance(&self, instance: &ProcessInstance) -> EngineResult<()>;
    /// Load all persisted process instances.
    async fn list_instances(&self) -> EngineResult<Vec<ProcessInstance>>;

    /// Delete a process instance.
    async fn delete_instance(&self, id: &str) -> EngineResult<()>;

    /// Persist a process definition metadata (JSON).
    async fn save_definition(&self, definition: &ProcessDefinition) -> EngineResult<()>;
    /// Load all persisted process definitions.
    async fn list_definitions(&self) -> EngineResult<Vec<ProcessDefinition>>;

    /// Delete a process definition.
    async fn delete_definition(&self, key: &str) -> EngineResult<()>;

    /// Persist a pending user task.
    async fn save_user_task(&self, task: &PendingUserTask) -> EngineResult<()>;
    /// Delete a pending user task (e.g. when completed).
    async fn delete_user_task(&self, task_id: uuid::Uuid) -> EngineResult<()>;
    /// Load all persisted pending user tasks.
    async fn list_user_tasks(&self) -> EngineResult<Vec<PendingUserTask>>;

    /// Persist an service task.
    async fn save_service_task(&self, task: &PendingServiceTask) -> EngineResult<()>;
    /// Delete an service task.
    async fn delete_service_task(&self, task_id: uuid::Uuid) -> EngineResult<()>;
    /// Load all persisted service tasks.
    async fn list_service_tasks(&self) -> EngineResult<Vec<PendingServiceTask>>;

    /// Persist a pending timer.
    async fn save_timer(&self, timer: &crate::engine::PendingTimer) -> EngineResult<()>;
    /// Delete a pending timer.
    async fn delete_timer(&self, timer_id: uuid::Uuid) -> EngineResult<()>;
    /// Load all persisted pending timers.
    async fn list_timers(&self) -> EngineResult<Vec<crate::engine::PendingTimer>>;

    /// Persist a pending message catch.
    async fn save_message_catch(&self, catch: &crate::engine::PendingMessageCatch) -> EngineResult<()>;
    /// Delete a pending message catch.
    async fn delete_message_catch(&self, catch_id: uuid::Uuid) -> EngineResult<()>;
    /// Load all persisted pending message catches.
    async fn list_message_catches(&self) -> EngineResult<Vec<crate::engine::PendingMessageCatch>>;

    /// Store original BPMN 2.0 XML for a definition.
    async fn save_bpmn_xml(&self, definition_key: &str, xml: &str) -> EngineResult<()>;
    /// Load original BPMN 2.0 XML for a definition.
    async fn load_bpmn_xml(&self, definition_key: &str) -> EngineResult<String>;
    /// List all stored BPMN XML definition keys.
    async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>>;

    /// Returns storage backend information (name, version, etc.).
    /// Returns None if the backend doesn't support reporting.
    async fn get_storage_info(&self) -> EngineResult<Option<StorageInfo>>;

    /// Append a new history entry to the instance history log.
    async fn append_history_entry(&self, entry: &crate::history::HistoryEntry) -> EngineResult<()>;
    /// Retrieve all history entries for a specific instance, ordered by time.
    async fn query_history(&self, query: HistoryQuery) -> EngineResult<Vec<crate::history::HistoryEntry>>;
}
