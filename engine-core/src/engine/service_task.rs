//! Service task operations (Camunda-style fetch-and-lock pattern).
//!
//! These are `impl WorkflowEngine` methods extracted into a separate file
//! for maintainability. The public API is unchanged.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};

use super::{PendingServiceTask, InstanceState, WorkflowEngine};

/// Verifies that the given worker holds the lock on an service task.
///
/// Returns `Ok(())` if `locked_worker` matches `worker_id`.
/// Returns an error if the task is locked by a different worker or not locked at all.
fn verify_lock_ownership(
    task_id: Uuid,
    locked_worker: &Option<String>,
    worker_id: &str,
) -> EngineResult<()> {
    match locked_worker {
        Some(locked_by) if locked_by != worker_id => {
            Err(EngineError::ServiceTaskLocked {
                task_id,
                worker_id: locked_by.clone(),
            })
        }
        None => Err(EngineError::ServiceTaskNotLocked(task_id)),
        _ => Ok(()),
    }
}

impl WorkflowEngine {
    /// Fetches and locks service tasks matching the requested topics.
    ///
    /// Returns up to `max_tasks` unlocked tasks whose topic appears in
    /// `topics`. Each returned task is locked for `lock_duration` seconds
    /// and assigned to `worker_id`.
    pub async fn fetch_and_lock_service_tasks(
        &mut self,
        worker_id: &str,
        max_tasks: usize,
        topics: &[String],
        lock_duration: i64,
    ) -> Vec<PendingServiceTask> {
        let now = Utc::now();
        let mut result = Vec::new();
        let mut to_persist = Vec::new();

        for task in self.pending_service_tasks.values_mut() {
            if result.len() >= max_tasks {
                break;
            }

            // Skip tasks whose topic is not requested
            if !topics.contains(&task.topic) {
                continue;
            }

            // Skip tasks that are already locked and not expired
            if let Some(expiration) = task.lock_expiration {
                if expiration > now {
                    continue;
                }
                // Lock expired — release it
                log::info!("Service task {}: lock expired, releasing", task.id);
            }

            // Lock the task
            task.worker_id = Some(worker_id.to_string());
            task.lock_expiration =
                Some(now + TimeDelta::seconds(lock_duration));

            log::info!(
                "Service task {} locked by worker '{}' for {}s",
                task.id, worker_id, lock_duration
            );

            result.push(task.clone());
            to_persist.push(task.id);
        }

        for id in to_persist {
            self.persist_service_task(id).await;
        }

        result
    }

    /// Completes an service task, advancing the process instance.
    ///
    /// The task must be locked by `worker_id`. Optional variables are merged.
    pub async fn complete_service_task(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        variables: HashMap<String, Value>,
    ) -> EngineResult<()> {
        let task = self
            .pending_service_tasks
            .get(&task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

        // Verify lock ownership
        verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

        let task = self.pending_service_tasks.remove(&task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;
        let instance_id = task.instance_id;

        let old_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };

        // Retrieve token from central store and merge variables
        let mut token = {
            let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.tokens.remove(&task.token_id)
                .ok_or_else(|| EngineError::InvalidDefinition(format!("Token {} not found in instance", task.token_id)))?
        };
        for (k, v) in variables {
            token.variables.insert(k, v);
        }
        
        self.cancel_boundary_timers(instance_id, &task.node_id).await;

        log::info!(
            "Instance {}: completed service task '{}' (task_id: {task_id})",
            instance_id, task.node_id
        );

        let def_key = {
            let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.audit_log.push(format!(
                "✅ Service task '{}' completed by worker '{}'",
                task.node_id, worker_id
            ));
            if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                inst.state = InstanceState::Running;
            }
            inst.variables = token.variables.clone();
            inst.definition_key
        };

        // Advance token to the next node
        let def = self
            .definitions
            .get(&def_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(def_key))?;
        let def = Arc::clone(&def);

        // Run end scripts
        {
            let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            let crate::ProcessInstance { audit_log, variables, .. } = &mut *inst;
            crate::script_runner::run_end_scripts(
                &self.script_engine,
                instance_id,
                &mut token,
                &def,
                &task.node_id,
                audit_log,
                variables,
            )?;
        }

        let next = crate::engine::executor::resolve_next_target(&def, &task.node_id, &token.variables)?;

        token.current_node = next.clone();
        // Update instance current_node so UI highlights correctly
        let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        {
            let mut inst = inst_arc.write().await;
            inst.current_node = next;
        }


        self.remove_persisted_service_task(task_id).await;

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::TaskCompleted,
            &format!("Service task '{}' completed", task.node_id),
            crate::history::ActorType::ServiceWorker,
            Some(worker_id.to_string()),
            old_state.as_ref()
        ).await;

        self.run_instance_batch(instance_id, token).await
    }

    /// Reports a failure for an service task.
    ///
    /// Decrements retries. When retries reach 0, the task becomes an incident.
    pub async fn fail_service_task(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        retries: Option<i32>,
        error_message: Option<String>,
        error_details: Option<String>,
    ) -> EngineResult<()> {
        let instance_id = {
            let task = self
                .pending_service_tasks
                .get_mut(&task_id)
                .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

            // Verify lock ownership
            verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

            // Update retries
            let new_retries = retries.unwrap_or(task.retries - 1);
            task.retries = new_retries;
            task.error_message = error_message.clone();
            task.error_details = error_details.clone();

            // Release the lock so it can be retried (or becomes incident)
            task.worker_id = None;
            task.lock_expiration = None;
            
            let instance_id = task.instance_id;
            let node_id = task.node_id.clone();

            if new_retries <= 0 {
                // Incident: log and record on the instance
                if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let mut inst = inst_arc.write().await;
                    let msg = error_message.unwrap_or_else(|| "Unknown error".into());
                    inst.audit_log.push(format!(
                        "🚨 INCIDENT: Service task '{}' failed with 0 retries — {}",
                        node_id, msg
                    ));
                }
                log::warn!(
                    "Service task {task_id}: incident created (retries exhausted)"
                );
            } else {
                log::info!(
                    "Service task {task_id}: failed, {} retries remaining",
                    new_retries
                );
            }
            instance_id
        };

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::Error,
            &format!("Service task '{}' failed", task_id),
            crate::history::ActorType::ServiceWorker,
            Some(worker_id.to_string()),
            None // State variables didn't fundamentally change, but it's an error record
        ).await;

        self.persist_service_task(task_id).await;
        self.persist_instance(instance_id).await;

        Ok(())
    }

    /// Extends the lock on an service task.
    pub async fn extend_lock(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        additional_duration: i64,
    ) -> EngineResult<()> {
        {
            let task = self
                .pending_service_tasks
                .get_mut(&task_id)
                .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

            verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

            task.lock_expiration =
                Some(Utc::now() + TimeDelta::seconds(additional_duration));

            log::info!(
                "Service task {task_id}: lock extended by {additional_duration}s"
            );
        }

        self.persist_service_task(task_id).await;

        Ok(())
    }

    /// Handles a BPMN error for an service task.
    ///
    /// Simple implementation: logs the error and creates an incident-style
    /// audit entry. The task is removed from the pending queue.
    pub async fn handle_bpmn_error(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        error_code: &str,
    ) -> EngineResult<()> {
        let task = self
            .pending_service_tasks
            .get(&task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

        verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

        let task = self.pending_service_tasks.remove(&task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;
        let instance_id = task.instance_id;

        let def_key = {
            let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let inst = inst_arc.read().await;
            inst.definition_key
        };
        
        self.cancel_boundary_timers(instance_id, &task.node_id).await;
        
        let mut target_boundary = None;
        if let Some(def) = self.definitions.get(&def_key).await {
            for (node_id, node) in &def.nodes {
                if let crate::model::BpmnElement::BoundaryErrorEvent { attached_to, error_code: bound_err } = node {
                    if attached_to == &task.node_id && (bound_err.is_none() || bound_err.as_deref() == Some(error_code)) {
                        target_boundary = Some(node_id.clone());
                        break;
                    }
                }
            }
        }
        
        if let Some(boundary_id) = target_boundary {
            let old_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };
            {
                let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.audit_log.push(format!("💥 BPMN Error '{error_code}' caught by boundary event '{boundary_id}'"));
                inst.state = InstanceState::Running;
            }
            
            self.record_history_event(
                instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                &format!("Error '{error_code}' caught"),
                crate::history::ActorType::Engine,
                None,
                old_state.as_ref()
            ).await;
            
            // Retrieve token from central store
            let mut token = {
                let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.tokens.remove(&task.token_id)
                    .ok_or_else(|| EngineError::InvalidDefinition(format!("Token {} not found in instance", task.token_id)))?
            };
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(&def, &boundary_id, &token.variables)?;
            
            token.current_node = next.clone();
            {
                let inst_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }
            
            self.remove_persisted_service_task(task_id).await;
            self.persist_instance(instance_id).await;
            self.run_instance_batch(instance_id, token).await?;
            return Ok(());
        }

        // If no boundary event found, just log it as an unhandled error/incident.
        if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let mut inst = inst_arc.write().await;
            inst.audit_log.push(format!(
                "🚨 BPMN error '{}' thrown by worker '{}' at service task '{}' (No boundary event caught it)",
                error_code, worker_id, task.node_id
            ));
        }

        log::warn!(
            "Service task {task_id}: unhandled BPMN error '{error_code}' from worker '{worker_id}'"
        );
        
        self.remove_persisted_service_task(task_id).await;
        self.persist_instance(instance_id).await;

        Ok(())
    }
}
