//! Background retry queue for failed persistence operations.
//!
//! When a persist operation fails after inline retries, the job is pushed
//! to an unbounded channel. A background tokio task drains this channel,
//! re-reads the current in-memory state, and retries the persistence call
//! with exponential backoff.

use dashmap::DashMap;
use super::types::*;
use std::sync::Arc;
use uuid::Uuid;
use tokio::sync::mpsc;

use crate::history::HistoryEntry;
use crate::persistence::WorkflowPersistence;

/// Maximum number of retry attempts per job before giving up.
const MAX_RETRIES: u32 = 50;

/// Initial backoff delay (doubles each attempt, capped at MAX_BACKOFF).
const INITIAL_BACKOFF_MS: u64 = 1_000;

/// Maximum backoff delay.
const MAX_BACKOFF_MS: u64 = 60_000;

/// Number of inline retry attempts (before queuing to background).
pub(crate) const INLINE_RETRIES: u32 = 2;

/// Delay between inline retries in milliseconds (doubles each attempt).
pub(crate) const INLINE_BACKOFF_MS: u64 = 50;

/// A persistence operation that can be retried.
#[derive(Debug, Clone)]
pub(crate) enum PersistJob {
    /// Re-read instance from InstanceStore and save to NATS.
    SaveInstance(Uuid),
    /// Re-read definition from DefinitionRegistry and save to NATS.
    SaveDefinition(Uuid),
    /// Re-read user task from pending_user_tasks and save to NATS.
    SaveUserTask(Uuid),
    /// Delete a user task from NATS.
    DeleteUserTask(Uuid),
    /// Re-read service task from pending_service_tasks and save to NATS.
    SaveServiceTask(Uuid),
    /// Delete a service task from NATS.
    DeleteServiceTask(Uuid),
    /// Re-read timer from pending_timers and save to NATS.
    SaveTimer(Uuid),
    /// Delete a timer from NATS.
    DeleteTimer(Uuid),
    /// Re-read message catch from pending_message_catches and save to NATS.
    SaveMessageCatch(Uuid),
    /// Delete a message catch from NATS.
    DeleteMessageCatch(Uuid),
    /// Append a history entry to NATS.
    AppendHistoryEntry(Box<HistoryEntry>),
}

impl std::fmt::Display for PersistJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SaveInstance(id) => write!(f, "SaveInstance({})", id),
            Self::SaveDefinition(id) => write!(f, "SaveDefinition({})", id),
            Self::SaveUserTask(id) => write!(f, "SaveUserTask({})", id),
            Self::DeleteUserTask(id) => write!(f, "DeleteUserTask({})", id),
            Self::SaveServiceTask(id) => write!(f, "SaveServiceTask({})", id),
            Self::DeleteServiceTask(id) => write!(f, "DeleteServiceTask({})", id),
            Self::SaveTimer(id) => write!(f, "SaveTimer({})", id),
            Self::DeleteTimer(id) => write!(f, "DeleteTimer({})", id),
            Self::SaveMessageCatch(id) => write!(f, "SaveMessageCatch({})", id),
            Self::DeleteMessageCatch(id) => write!(f, "DeleteMessageCatch({})", id),
            Self::AppendHistoryEntry(e) => write!(f, "AppendHistoryEntry({})", e.instance_id),
        }
    }
}

/// Sender half of the retry queue. Cheap to clone.
pub(crate) type RetryQueueTx = mpsc::UnboundedSender<PersistJob>;

/// Receiver half of the retry queue (consumed by the background task).
pub(crate) type RetryQueueRx = mpsc::UnboundedReceiver<PersistJob>;

/// Creates a new retry queue channel pair.
pub(crate) fn create_retry_queue() -> (RetryQueueTx, RetryQueueRx) {
    mpsc::unbounded_channel()
}

/// Spawns the background retry worker.
///
/// This task receives failed `PersistJob`s from the channel and retries them
/// with exponential backoff. It re-reads the current in-memory state from the
/// provided data sources (instance_store, definition_registry, pending maps)
/// so it always persists the *latest* state, not a stale snapshot.
///
/// The `persistence`, `instances`, and `definitions` parameters are shared
/// references from the WorkflowEngine.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_retry_worker(
    mut rx: RetryQueueRx,
    persistence: Arc<dyn WorkflowPersistence>,
    instances: crate::engine::instance_store::InstanceStore,
    definitions: crate::engine::registry::DefinitionRegistry,
    pending_user_tasks: Arc<DashMap<Uuid, PendingUserTask>>,
    pending_service_tasks: Arc<DashMap<Uuid, PendingServiceTask>>,
    pending_timers: Arc<DashMap<Uuid, PendingTimer>>,
    pending_message_catches: Arc<DashMap<Uuid, PendingMessageCatch>>,
    error_counter: Arc<std::sync::atomic::AtomicU64>,
) {
    tokio::spawn(async move {
        tracing::info!("Persistence retry worker started");

        while let Some(job) = rx.recv().await {
            let mut attempt = 0u32;
            let mut backoff_ms = INITIAL_BACKOFF_MS;

            loop {
                attempt += 1;
                let result = execute_job(
                    &job,
                    &persistence,
                    &instances,
                    &definitions,
                    &pending_user_tasks,
                    &pending_service_tasks,
                    &pending_timers,
                    &pending_message_catches,
                ).await;

                match result {
                    Ok(()) => {
                        tracing::info!(
                            "Retry succeeded for {} (attempt {})",
                            job, attempt
                        );
                        break;
                    }
                    Err(e) if attempt >= MAX_RETRIES => {
                        error_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::error!(
                            "PERMANENT PERSISTENCE FAILURE after {} retries for {}: {}",
                            MAX_RETRIES, job, e
                        );
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Retry {}/{} for {} failed: {} — backing off {}ms",
                            attempt, MAX_RETRIES, job, e, backoff_ms
                        );
                        tokio::time::sleep(
                            tokio::time::Duration::from_millis(backoff_ms)
                        ).await;
                        backoff_ms = (backoff_ms * 2).min(MAX_BACKOFF_MS);
                    }
                }
            }
        }

        tracing::warn!("Persistence retry worker stopped (channel closed)");
    });
}

/// Executes a single persist job against the persistence backend.
///
/// For "Save" jobs, re-reads the current state from in-memory stores.
/// If the entity no longer exists in memory (e.g. instance was deleted),
/// the job is silently skipped (returns Ok).
#[allow(clippy::too_many_arguments)]
async fn execute_job(
    job: &PersistJob,
    persistence: &Arc<dyn WorkflowPersistence>,
    instances: &crate::engine::instance_store::InstanceStore,
    definitions: &crate::engine::registry::DefinitionRegistry,
    pending_user_tasks: &Arc<DashMap<Uuid, PendingUserTask>>,
    pending_service_tasks: &Arc<DashMap<Uuid, PendingServiceTask>>,
    pending_timers: &Arc<DashMap<Uuid, PendingTimer>>,
    pending_message_catches: &Arc<DashMap<Uuid, PendingMessageCatch>>,
) -> Result<(), String> {
    match job {
        PersistJob::SaveInstance(id) => {
            if let Some(inst_arc) = instances.get(id).await {
                let inst = inst_arc.read().await;
                persistence.save_instance(&inst).await
                    .map_err(|e| e.to_string())
            } else {
                // Instance was deleted since the job was queued — skip.
                Ok(())
            }
        }
        PersistJob::SaveDefinition(id) => {
            if let Some(def) = definitions.get(id).await {
                persistence.save_definition(&def).await
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        PersistJob::SaveUserTask(id) => {
            if let Some(task_ref) = pending_user_tasks.get(id) {
                persistence.save_user_task(&task_ref).await
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        PersistJob::DeleteUserTask(id) => {
            persistence.delete_user_task(*id).await
                .map_err(|e| e.to_string())
        }
        PersistJob::SaveServiceTask(id) => {
            if let Some(task_ref) = pending_service_tasks.get(id) {
                persistence.save_service_task(&task_ref).await
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        PersistJob::DeleteServiceTask(id) => {
            persistence.delete_service_task(*id).await
                .map_err(|e| e.to_string())
        }
        PersistJob::SaveTimer(id) => {
            if let Some(timer_ref) = pending_timers.get(id) {
                persistence.save_timer(&timer_ref).await
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        PersistJob::DeleteTimer(id) => {
            persistence.delete_timer(*id).await
                .map_err(|e| e.to_string())
        }
        PersistJob::SaveMessageCatch(id) => {
            if let Some(catch_ref) = pending_message_catches.get(id) {
                persistence.save_message_catch(&catch_ref).await
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        }
        PersistJob::DeleteMessageCatch(id) => {
            persistence.delete_message_catch(*id).await
                .map_err(|e| e.to_string())
        }
        PersistJob::AppendHistoryEntry(entry) => {
            persistence.append_history_entry(entry).await
                .map_err(|e| e.to_string())
        }
    }
}
