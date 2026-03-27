use async_trait::async_trait;

use crate::engine::{ProcessInstance, PendingUserTask};
use crate::error::EngineResult;
use crate::model::{Token, ProcessDefinition};

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
}
