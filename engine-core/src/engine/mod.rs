use std::sync::Arc;
use dashmap::DashMap;
use uuid::Uuid;

// Re-export model types used by test modules via `use super::*`
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
#[allow(unused_imports)]
use std::time::Duration;
#[cfg(test)]
#[allow(unused_imports)]
use crate::error::{EngineError, EngineResult};
#[cfg(test)]
#[allow(unused_imports)]
use crate::model::{BpmnElement, ProcessDefinition, Token, FileReference};

use crate::persistence::WorkflowPersistence;

pub mod types;
pub(crate) mod instance_store;
pub(crate) mod registry;
pub(crate) mod executor;
pub(crate) mod gateway;
pub(crate) mod boundary;
mod service_task;
mod persistence_ops;
mod timer_processor;
mod message_processor;
mod user_task;
mod process_start;
mod instance_ops;
mod definition_ops;
pub(crate) mod retry_queue;

pub use types::*;

/// The central workflow engine managing definitions, instances, and handlers.
pub struct WorkflowEngine {
    pub(crate) definitions: registry::DefinitionRegistry,
    pub(crate) instances: crate::engine::instance_store::InstanceStore,
    pub(crate) pending_user_tasks: Arc<DashMap<Uuid, PendingUserTask>>,
    pub(crate) pending_service_tasks: Arc<DashMap<Uuid, PendingServiceTask>>,
    pub(crate) pending_timers: Arc<DashMap<Uuid, PendingTimer>>,
    pub(crate) pending_message_catches: Arc<DashMap<Uuid, PendingMessageCatch>>,
    pub(crate) persistence: Option<Arc<dyn WorkflowPersistence>>,
    pub(crate) persistence_error_count: std::sync::atomic::AtomicU64,
    pub(crate) retry_tx: Option<retry_queue::RetryQueueTx>,
}

/// Creates a configured Rhai script engine.
/// Called per-evaluation to avoid Sync issues and lock contention.
pub(crate) fn create_script_engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(10_000);
    engine
}

impl WorkflowEngine {
    /// Creates a new, empty engine.
    pub fn new() -> Self {
        tracing::info!("WorkflowEngine initialized");

        Self {
            definitions: registry::DefinitionRegistry::new(),
            instances: crate::engine::instance_store::InstanceStore::new(),
            pending_user_tasks: Arc::new(DashMap::new()),
            pending_service_tasks: Arc::new(DashMap::new()),
            pending_timers: Arc::new(DashMap::new()),
            pending_message_catches: Arc::new(DashMap::new()),
            persistence: None,
            persistence_error_count: std::sync::atomic::AtomicU64::new(0),
            retry_tx: None,
        }
    }

    /// Creates a new engine equipped with the InMemoryPersistence backend.
    pub fn with_in_memory_persistence() -> Self {
        let p = Arc::new(crate::persistence_in_memory::InMemoryPersistence::new());
        Self::new().with_persistence(p)
    }

    /// Attaches a persistence layer to the engine.
    pub fn with_persistence(mut self, persistence: Arc<dyn WorkflowPersistence>) -> Self {
        let (tx, rx) = retry_queue::create_retry_queue();
        let error_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        retry_queue::spawn_retry_worker(
            rx,
            Arc::clone(&persistence),
            self.instances.clone(),
            self.definitions.clone(),
            Arc::clone(&self.pending_user_tasks),
            Arc::clone(&self.pending_service_tasks),
            Arc::clone(&self.pending_timers),
            Arc::clone(&self.pending_message_catches),
            error_counter,
        );

        self.persistence = Some(persistence);
        self.retry_tx = Some(tx);
        self
    }

    /// Sets the persistence layer (builder-style alternative to `with_persistence`).
    pub fn set_persistence(&mut self, persistence: Arc<dyn WorkflowPersistence>) {
        let (tx, rx) = retry_queue::create_retry_queue();
        let error_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

        retry_queue::spawn_retry_worker(
            rx,
            Arc::clone(&persistence),
            self.instances.clone(),
            self.definitions.clone(),
            Arc::clone(&self.pending_user_tasks),
            Arc::clone(&self.pending_service_tasks),
            Arc::clone(&self.pending_timers),
            Arc::clone(&self.pending_message_catches),
            error_counter,
        );

        self.persistence = Some(persistence);
        self.retry_tx = Some(tx);
    }

    /// Restores a process instance from persistence (e.g. on server startup).
    pub async fn restore_instance(&self, instance: ProcessInstance) {
        tracing::info!("Restored instance {} (def: {})", instance.id, instance.definition_key);
        self.instances.insert(instance.id, instance).await;
    }

    /// Restores a pending user task from persistence.
    pub fn restore_user_task(&self, task: PendingUserTask) {
        tracing::info!("Restored user task {} (instance: {})", task.task_id, task.instance_id);
        self.pending_user_tasks.insert(task.task_id, task);
    }

    /// Restores a pending service task from persistence.
    pub fn restore_service_task(&self, task: PendingServiceTask) {
        tracing::info!("Restored service task {} (instance: {})", task.id, task.instance_id);
        self.pending_service_tasks.insert(task.id, task);
    }

    /// Restores a pending timer from persistence (e.g. on server startup).
    pub fn restore_timer(&self, timer: PendingTimer) {
        tracing::info!("Restored timer {} (instance: {}, node: {})", timer.id, timer.instance_id, timer.node_id);
        self.pending_timers.insert(timer.id, timer);
    }

    /// Restores a pending message catch from persistence (e.g. on server startup).
    pub fn restore_message_catch(&self, catch: PendingMessageCatch) {
        tracing::info!("Restored message catch {} (instance: {}, message: {})", catch.id, catch.instance_id, catch.message_name);
        self.pending_message_catches.insert(catch.id, catch);
    }

    /// Helper to cancel any pending boundary timers attached to a task node that is being completed/aborted.
    pub(crate) async fn cancel_boundary_timers(&self, instance_id: Uuid, task_node_id: &str) {
        let def_key = if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let inst = inst_arc.read().await;
            inst.definition_key
        } else {
            return;
        };
        
        let bound_timers: Vec<String> = if let Some(def) = self.definitions.get(&def_key).await {
            def.nodes.iter()
                .filter_map(|(id, node)| {
                    if let crate::model::BpmnElement::BoundaryTimerEvent { attached_to, .. } = node {
                        if attached_to == task_node_id {
                            Some(id.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        
        // Collect timer IDs to delete from persistence
        let timer_ids_to_delete: std::collections::HashSet<Uuid> = self.pending_timers.iter()
            .filter(|r| r.instance_id == instance_id && bound_timers.contains(&r.node_id))
            .map(|r| r.id)
            .collect();
            
        self.pending_timers.retain(|_, t| !(t.instance_id == instance_id && bound_timers.contains(&t.node_id)));
        
        // Delete from persistence
        if let Some(persistence) = &self.persistence {
            for timer_id in timer_ids_to_delete {
                if let Err(e) = persistence.delete_timer(timer_id).await {
                    self.log_persistence_error(&format!("delete_boundary_timer({})", timer_id), e);
                }
            }
        }
    }

    /// Clears any pending wait states (timers, messages) associated with a specific token.
    /// Used by Event-Based Gateways to cancel alternative events when one fires.
    pub async fn clear_wait_states_for_token(&self, instance_id: Uuid, token_id: &Uuid) {
        // Collect timer IDs to delete
        let timers_to_delete: Vec<Uuid> = self.pending_timers.iter()
            .filter(|r| r.instance_id == instance_id && &r.token_id == token_id)
            .map(|r| r.id)
            .collect();

        // Collect message catch IDs to delete
        let messages_to_delete: Vec<Uuid> = self.pending_message_catches.iter()
            .filter(|r| r.instance_id == instance_id && &r.token_id == token_id)
            .map(|r| r.id)
            .collect();
            
        // Log to instance audit log
        if !timers_to_delete.is_empty() || !messages_to_delete.is_empty() {
            if let Some(inst_arc) = self.instances.get(&instance_id).await {
                let mut inst = inst_arc.write().await;
                if !timers_to_delete.is_empty() {
                    inst.audit_log.push(format!("⭮ Event-based gateway: {} alternative timer(s) cancelled", timers_to_delete.len()));
                }
                if !messages_to_delete.is_empty() {
                    inst.audit_log.push(format!("⭮ Event-based gateway: {} alternative message catch(es) cancelled", messages_to_delete.len()));
                }
            }
            
            // Add custom history trace
            self.record_history_event(
                instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                "Alternative EventBasedGateway paths cancelled",
                crate::history::ActorType::Engine,
                None,
                None
            ).await;
        }

        // Remove from DashMap
        self.pending_timers.retain(|_, t| !(t.instance_id == instance_id && &t.token_id == token_id));
        self.pending_message_catches.retain(|_, m| !(m.instance_id == instance_id && &m.token_id == token_id));

        // Delete from persistence
        if let Some(persistence) = &self.persistence {
            for timer_id in timers_to_delete {
                if let Err(e) = persistence.delete_timer(timer_id).await {
                    self.log_persistence_error(&format!("delete_timer({})", timer_id), e);
                }
            }
            for msg_id in messages_to_delete {
                if let Err(e) = persistence.delete_message_catch(msg_id).await {
                    self.log_persistence_error(&format!("delete_message_catch({})", msg_id), e);
                }
            }
        }
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;

#[cfg(test)]
mod stress_tests;
