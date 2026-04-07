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
    TaskNotPending { task_id: Uuid, actual_state: String },

    /// The process instance has already completed.
    #[error("Process instance has already completed")]
    #[allow(dead_code)]
    AlreadyCompleted,

    /// The timer duration does not match the start event's configuration.
    #[error("Timer mismatch: expected {expected}s, got {provided}s")]
    TimerMismatch { expected: u64, provided: u64 },

    /// No condition matched at a gateway node (and no default flow exists).
    #[error("No matching condition at gateway '{0}'")]
    NoMatchingCondition(String),

    /// The requested service task does not exist.
    #[error("Service task not found: {0}")]
    ServiceTaskNotFound(Uuid),

    /// The service task is locked by another worker.
    #[error("Service task '{task_id}' is locked by worker '{worker_id}'")]
    ServiceTaskLocked { task_id: Uuid, worker_id: String },

    /// The service task is not currently locked (cannot complete/fail).
    #[error("Service task '{0}' is not locked")]
    ServiceTaskNotLocked(Uuid),

    /// Cannot delete definition because it has instances.
    #[error("Cannot delete definition: {0} instances still exist")]
    DefinitionHasInstances(usize),

    #[error("Persistence error: {0}")]
    PersistenceError(String),

    /// A script execution failed.
    #[error("Script error: {0}")]
    ScriptError(String),

    /// The execution step limit was exceeded (possible infinite loop).
    #[error("Execution limit exceeded: {0}")]
    ExecutionLimitExceeded(String),
}

/// Convenience alias used throughout the engine.
pub type EngineResult<T> = Result<T, EngineError>;
