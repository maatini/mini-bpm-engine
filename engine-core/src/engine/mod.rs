use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

// Re-export model types used by test modules via `use super::*`
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

pub use types::*;

/// The central workflow engine managing definitions, instances, and handlers.
pub struct WorkflowEngine {
    pub(crate) definitions: registry::DefinitionRegistry,
    pub(crate) instances: crate::engine::instance_store::InstanceStore,
    pub(crate) pending_user_tasks: HashMap<Uuid, PendingUserTask>,
    pub(crate) pending_service_tasks: HashMap<Uuid, PendingServiceTask>,
    pub(crate) pending_timers: HashMap<Uuid, PendingTimer>,
    pub(crate) pending_message_catches: HashMap<Uuid, PendingMessageCatch>,
    pub(crate) persistence: Option<Arc<dyn WorkflowPersistence>>,
    pub(crate) script_engine: rhai::Engine,
    pub(crate) persistence_error_count: std::sync::atomic::AtomicU64,
}

impl WorkflowEngine {
    /// Creates a new, empty engine.
    pub fn new() -> Self {
        log::info!("WorkflowEngine initialized");
        let mut script_engine = rhai::Engine::new();
        script_engine.set_max_operations(10_000); // Prevent infinite loops

        Self {
            definitions: registry::DefinitionRegistry::new(),
            instances: crate::engine::instance_store::InstanceStore::new(),
            pending_user_tasks: HashMap::new(),
            pending_service_tasks: HashMap::new(),
            pending_timers: HashMap::new(),
            pending_message_catches: HashMap::new(),
            persistence: None,
            script_engine,
            persistence_error_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Creates a new engine equipped with the InMemoryPersistence backend.
    pub fn with_in_memory_persistence() -> Self {
        let p = Arc::new(crate::persistence_in_memory::InMemoryPersistence::new());
        Self::new().with_persistence(p)
    }

    /// Attaches a persistence layer to the engine.
    pub fn with_persistence(mut self, persistence: Arc<dyn WorkflowPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Sets the persistence layer (builder-style alternative to `with_persistence`).
    pub fn set_persistence(&mut self, persistence: Arc<dyn WorkflowPersistence>) {
        self.persistence = Some(persistence);
    }

    /// Restores a process instance from persistence (e.g. on server startup).
    pub async fn restore_instance(&mut self, instance: ProcessInstance) {
        log::info!("Restored instance {} (def: {})", instance.id, instance.definition_key);
        self.instances.insert(instance.id, instance).await;
    }

    /// Restores a pending user task from persistence.
    pub fn restore_user_task(&mut self, task: PendingUserTask) {
        log::info!("Restored user task {} (instance: {})", task.task_id, task.instance_id);
        self.pending_user_tasks.insert(task.task_id, task);
    }

    /// Restores a pending service task from persistence.
    pub fn restore_service_task(&mut self, task: PendingServiceTask) {
        log::info!("Restored service task {} (instance: {})", task.id, task.instance_id);
        self.pending_service_tasks.insert(task.id, task);
    }

    /// Restores a pending timer from persistence (e.g. on server startup).
    pub fn restore_timer(&mut self, timer: PendingTimer) {
        log::info!("Restored timer {} (instance: {}, node: {})", timer.id, timer.instance_id, timer.node_id);
        self.pending_timers.insert(timer.id, timer);
    }

    /// Restores a pending message catch from persistence (e.g. on server startup).
    pub fn restore_message_catch(&mut self, catch: PendingMessageCatch) {
        log::info!("Restored message catch {} (instance: {}, message: {})", catch.id, catch.instance_id, catch.message_name);
        self.pending_message_catches.insert(catch.id, catch);
    }

    /// Helper to cancel any pending boundary timers attached to a task node that is being completed/aborted.
    pub(crate) async fn cancel_boundary_timers(&mut self, instance_id: Uuid, task_node_id: &str) {
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
        let timer_ids_to_delete: std::collections::HashSet<Uuid> = self.pending_timers.values()
            .filter(|t| t.instance_id == instance_id && bound_timers.contains(&t.node_id))
            .map(|t| t.id)
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
