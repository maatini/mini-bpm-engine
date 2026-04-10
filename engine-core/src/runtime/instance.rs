
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;
use crate::domain::{FileReference, Token};
use crate::runtime::{PendingUserTask, PendingServiceTask, PendingTimer, PendingMessageCatch};

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
    /// Ends the current path with an escalation code (non-fatal, propagates to parent).
    EscalationEnd { escalation_code: String },
    /// Spawns extra tokens (e.g. non-interrupting escalation handler) while the main token continues.
    SpawnAndContinue {
        main: Token,
        spawned: Vec<Token>,
    },
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
    /// Instance is suspended – no timers fire, no tasks can be completed.
    /// Stores the state the instance was in before suspension so it can be
    /// restored on resume.
    Suspended {
        previous_state: Box<InstanceState>,
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

/// Tracks a successfully completed compensatable activity and its handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationRecord {
    /// The BPMN node ID of the activity that completed.
    pub activity_id: String,
    /// The BPMN node ID of the compensation handler (connected from BoundaryCompensationEvent).
    pub handler_node_id: String,
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
    /// LIFO log of completed compensatable activities and their handlers.
    #[serde(default)]
    pub compensation_log: Vec<CompensationRecord>,
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
        if self.audit_log.len() > crate::runtime::MAX_AUDIT_LOG_ENTRIES {
            let overflow = self.audit_log.len() - crate::runtime::MAX_AUDIT_LOG_ENTRIES;
            self.audit_log.drain(0..overflow);
        }
    }

    /// Appends multiple entries to the audit log, enforcing MAX_AUDIT_LOG_ENTRIES limit.
    pub fn append_audit_log(&mut self, entries: &mut Vec<String>) {
        self.audit_log.append(entries);
        if self.audit_log.len() > crate::runtime::MAX_AUDIT_LOG_ENTRIES {
            let overflow = self.audit_log.len() - crate::runtime::MAX_AUDIT_LOG_ENTRIES;
            self.audit_log.drain(0..overflow);
        }
    }
}
