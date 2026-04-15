use crate::domain::TimerDefinition;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

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
    #[serde(default)]
    pub business_key: Option<String>,
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
    #[serde(default)]
    pub business_key: Option<String>,
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
