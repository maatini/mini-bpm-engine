use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Engine errors — thiserror-derived for zero boilerplate
// ---------------------------------------------------------------------------

/// All errors that can occur within the BPMN engine.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    /// The process definition is structurally invalid.
    #[error("Invalid definition: {0}")]
    InvalidDefinition(String),

    /// A referenced node does not exist in the definition.
    #[error("No such node: {0}")]
    NoSuchNode(String),

    /// No process definition found for the given key.
    #[error("No such definition: {0}")]
    NoSuchDefinition(Uuid),

    /// No process instance found for the given ID.
    #[error("No such instance: {0}")]
    NoSuchInstance(Uuid),

    /// Tried to complete a user task that is not currently pending.
    #[error("Task '{task_id}' is not pending (current state: {actual_state})")]
    TaskNotPending {
        task_id: Uuid,
        actual_state: String,
    },

    /// The process instance has already completed.
    #[error("Process instance has already completed")]
    #[allow(dead_code)]
    AlreadyCompleted,

    /// The timer duration does not match the start event's configuration.
    #[error("Timer mismatch: expected {expected}s, got {provided}s")]
    TimerMismatch { expected: u64, provided: u64 },

    /// A required service handler is not registered.
    #[error("No service handler registered for '{0}'")]
    HandlerNotFound(String),

    /// No condition matched at a gateway node (and no default flow exists).
    #[error("No matching condition at gateway '{0}'")]
    NoMatchingCondition(String),

    /// The requested external task does not exist.
    #[error("External task not found: {0}")]
    ExternalTaskNotFound(Uuid),

    /// The external task is locked by another worker.
    #[error("External task '{task_id}' is locked by worker '{worker_id}'")]
    ExternalTaskLocked { task_id: Uuid, worker_id: String },

    /// The external task is not currently locked (cannot complete/fail).
    #[error("External task '{0}' is not locked")]
    ExternalTaskNotLocked(Uuid),

    /// Cannot delete definition because it has instances.
    #[error("Cannot delete definition: {0} instances still exist")]
    DefinitionHasInstances(usize),

    #[error("Persistence error: {0}")]
    PersistenceError(String),

    /// A script execution failed.
    #[error("Script error: {0}")]
    ScriptError(String),
}

/// Convenience alias used throughout the engine.
pub type EngineResult<T> = Result<T, EngineError>;
