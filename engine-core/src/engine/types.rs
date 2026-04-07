use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::model::{FileReference, Token};
use crate::timer_definition::TimerDefinition;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of audit log entries retained in-memory per instance.
/// Older entries are available via the History API.
pub const MAX_AUDIT_LOG_ENTRIES: usize = 200;

/// Yield to the Tokio scheduler every N execution steps to prevent
/// thread starvation on long-running or looping BPMN processes.
pub const YIELD_EVERY_N_STEPS: u32 = 64;

/// Hard limit on execution steps per `run_instance_batch` call.
/// Prevents infinite BPMN loops from blocking the engine indefinitely.
pub const MAX_EXECUTION_STEPS: u32 = 10_000;

/// Maximum serialized size for a single ProcessInstance KV entry (900 KB).
/// NATS default max_payload is 1 MB; we leave headroom for protocol overhead.
pub const MAX_INSTANCE_PAYLOAD_BYTES: usize = 900 * 1024;

// ---------------------------------------------------------------------------
// Pending user task
// ---------------------------------------------------------------------------

/// A user task that is waiting for external completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUserTask {
    pub task_id: Uuid,
    pub instance_id: Uuid,
    pub node_id: String,
    pub assignee: String,
    /// Reference to the token stored in ProcessInstance.tokens
    pub token_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// External task item (Camunda-style)
// ---------------------------------------------------------------------------

/// A service task that can be fetched and completed by remote workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingServiceTask {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub definition_key: Uuid,
    pub node_id: String,
    pub topic: String,
    /// Reference to the token stored in ProcessInstance.tokens
    pub token_id: Uuid,
    /// Snapshot of variables at task creation (for worker fetch-and-lock API).
    /// This is a read-only copy; the authoritative variables live in instance.tokens.
    pub variables_snapshot: HashMap<String, Value>,
    pub created_at: DateTime<Utc>,
    /// The worker that currently holds the lock (None = unlocked).
    pub worker_id: Option<String>,
    /// When the lock expires (None = not locked).
    pub lock_expiration: Option<DateTime<Utc>>,
    /// Remaining retries before an incident is created.
    pub retries: i32,
    /// Error message from the last failure.
    pub error_message: Option<String>,
    /// Detailed error information from the last failure.
    pub error_details: Option<String>,
}

// ---------------------------------------------------------------------------
// Pending Timers and Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTimer {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub node_id: String,
    pub expires_at: DateTime<Utc>,
    /// Reference to the token stored in ProcessInstance.tokens
    pub token_id: Uuid,
    /// Original timer definition — needed for recurring timers to
    /// compute the next expiry after each trigger.
    #[serde(default)]
    pub timer_def: Option<TimerDefinition>,
    /// Remaining repetitions for RepeatingInterval timers.
    /// None = check timer_def, Some(0) = do not repeat.
    #[serde(default)]
    pub remaining_repetitions: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessageCatch {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub node_id: String,
    pub message_name: String,
    /// Reference to the token stored in ProcessInstance.tokens
    pub token_id: Uuid,
}

// ---------------------------------------------------------------------------
// Next action (execution result)
// ---------------------------------------------------------------------------

/// The result of executing a single step in the process.
#[derive(Debug, Serialize, Deserialize)]
pub enum NextAction {
    /// The token should continue to the next node.
    Continue(Token),
    /// Multiple tokens should continue (inclusive gateway fork).
    ContinueMultiple(Vec<Token>),
    /// The engine must pause — a user task is pending.
    WaitForUser(PendingUserTask),
    /// The engine must pause — an external task is pending.
    WaitForServiceTask(PendingServiceTask),
    /// Token arrived at a join gateway but must wait for sibling tokens.
    WaitForJoin { gateway_id: String, token: Token },
    /// The engine must pause — a timer is pending.
    WaitForTimer(PendingTimer),
    /// The engine must pause — a message catch is pending.
    WaitForMessage(PendingMessageCatch),
    /// The engine must pause at an event-based gateway, registering multiple event listeners.
    WaitForEventGroup(Vec<NextAction>),
    /// The process reached an end event.
    Complete,
    /// Ends the current process instance with an error code (for error propagation).
    ErrorEnd { error_code: String },
    /// Terminate End Event: kill all active tokens and complete the instance immediately.
    Terminate,
    /// The engine must pause — a call activity (sub-process) is pending.
    WaitForCallActivity {
        called_element: String,
        token: Token,
    },
    /// Multi-instance parallel: spawn N tokens that each execute the same task.
    MultiInstanceFork { node_id: String, tokens: Vec<Token> },
    /// Multi-instance sequential: re-execute the same task node.
    MultiInstanceNext { node_id: String, token: Token },
}

// ---------------------------------------------------------------------------
// Instance state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiInstanceProgress {
    pub node_id: String,
    pub total: usize,
    pub completed: usize,
    /// For sequential MI: the token that will re-execute the task
    pub sequential_token: Option<Token>,
}

/// The state of a process instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InstanceState {
    Running,
    WaitingOnUserTask {
        task_id: Uuid,
    },
    WaitingOnServiceTask {
        task_id: Uuid,
    },
    WaitingOnTimer {
        timer_id: Uuid,
    },
    WaitingOnMessage {
        message_id: Uuid,
    },
    WaitingOnEventBasedGateway,
    /// Multiple tokens are active; some may be waiting, some running.
    ParallelExecution {
        active_token_count: usize,
    },
    WaitingOnCallActivity {
        sub_instance_id: Uuid,
        token: Token,
    },
    Completed,
    /// Process ended in an ErrorEndEvent.
    CompletedWithError {
        error_code: String,
    },
}

// ---------------------------------------------------------------------------
// Process instance
// ---------------------------------------------------------------------------

/// A token actively traveling through the process graph.
/// Part of the Token-Registry on ProcessInstance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveToken {
    pub token: Token,
    /// ID of the fork gateway that spawned this token (None for the root token).
    pub fork_id: Option<String>,
    /// Index within the fork (0, 1, 2, ...) for deterministic ordering.
    pub branch_index: usize,
    /// Whether this token has completed (reached EndEvent or joined).
    pub completed: bool,
}

/// Synchronization barrier at a converging gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinBarrier {
    pub gateway_node_id: String,
    /// Number of tokens that must arrive before the join fires.
    pub expected_count: usize,
    /// Tokens that have arrived so far.
    pub arrived_tokens: Vec<Token>,
}

/// A live process instance tracked by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInstance {
    pub id: Uuid,
    pub definition_key: Uuid,
    pub business_key: String,
    #[serde(default)]
    pub parent_instance_id: Option<Uuid>,
    pub state: InstanceState,
    pub current_node: String,
    pub audit_log: Vec<String>,
    /// Current process variables (synced from the executing token).
    pub variables: HashMap<String, Value>,
    /// Central token store — the single source of truth for all active tokens.
    /// PendingTasks reference tokens by UUID instead of owning copies.
    #[serde(default)]
    pub tokens: HashMap<Uuid, Token>,
    /// All currently active tokens (Token-Registry).
    #[serde(default)]
    pub active_tokens: Vec<ActiveToken>,
    /// Join barriers waiting for tokens at converging gateways.
    #[serde(default)]
    pub join_barriers: HashMap<String, JoinBarrier>,
    /// Progress tracking for Multi-Instance tasks.
    #[serde(default)]
    pub multi_instance_state: HashMap<String, MultiInstanceProgress>,
}

impl ProcessInstance {
    /// Returns a typed FileReference if the variable exists and has type "file".
    pub fn get_file_reference(&self, var_name: &str) -> Option<FileReference> {
        self.variables
            .get(var_name)
            .and_then(FileReference::from_variable_value)
    }

    /// Returns all variable names that contain file references.
    pub fn file_variable_names(&self) -> Vec<String> {
        self.variables
            .iter()
            .filter(|(_, v)| FileReference::from_variable_value(v).is_some())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Pushes an entry to the audit log, enforcing MAX_AUDIT_LOG_ENTRIES limit.
    pub fn push_audit_log(&mut self, entry: String) {
        self.audit_log.push(entry);
        if self.audit_log.len() > crate::engine::types::MAX_AUDIT_LOG_ENTRIES {
            let overflow = self.audit_log.len() - crate::engine::types::MAX_AUDIT_LOG_ENTRIES;
            self.audit_log.drain(0..overflow);
        }
    }

    /// Appends multiple entries to the audit log, enforcing MAX_AUDIT_LOG_ENTRIES limit.
    pub fn append_audit_log(&mut self, entries: &mut Vec<String>) {
        self.audit_log.append(entries);
        if self.audit_log.len() > crate::engine::types::MAX_AUDIT_LOG_ENTRIES {
            let overflow = self.audit_log.len() - crate::engine::types::MAX_AUDIT_LOG_ENTRIES;
            self.audit_log.drain(0..overflow);
        }
    }
}

/// Summary statistics for engine monitoring.
#[derive(Debug, Clone, Serialize)]
pub struct EngineStats {
    pub definitions_count: usize,
    pub instances_total: usize,
    pub instances_running: usize,
    pub instances_completed: usize,
    pub instances_waiting_user: usize,
    pub instances_waiting_service: usize,
    pub pending_user_tasks: usize,
    pub pending_service_tasks: usize,
    pub pending_timers: usize,
    pub pending_message_catches: usize,
    /// Number of persistence write failures since engine start.
    pub persistence_errors: u64,
    /// Number of pending retry jobs in the background queue (0 = healthy).
    pub pending_retry_jobs: usize,
}
