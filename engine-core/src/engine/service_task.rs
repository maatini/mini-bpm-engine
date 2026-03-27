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

        for task in &mut self.pending_service_tasks {
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
        let idx = self
            .pending_service_tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

        let task = &self.pending_service_tasks[idx];

        // Verify lock ownership
        verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

        let task = self.pending_service_tasks.remove(idx);
        let instance_id = task.instance_id;

        // Merge variables into the token
        let mut token = task.token;
        for (k, v) in variables {
            token.variables.insert(k, v);
        }

        log::info!(
            "Instance {}: completed service task '{}' (task_id: {task_id})",
            instance_id, task.node_id
        );

        let inst = self
            .instances
            .get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        inst.audit_log.push(format!(
            "✅ Service task '{}' completed by worker '{}'",
            task.node_id, worker_id
        ));
        inst.state = InstanceState::Running;
        inst.variables = token.variables.clone();
        let def_key = inst.definition_key;

        // Advance token to the next node
        let def = self
            .definitions
            .get(&def_key)
            .ok_or(EngineError::NoSuchDefinition(def_key))?;
        let def = Arc::clone(def);

        // Run end scripts
        {
            let inst = self.instances.get_mut(&instance_id)
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            crate::script_runner::run_end_scripts(
                &self.script_engine,
                instance_id,
                &mut token,
                &def,
                &task.node_id,
                &mut inst.audit_log,
                &mut inst.variables,
            )?;
        }

        let next = super::resolve_next_target(&def, &task.node_id, &token.variables)?;

        token.current_node = next.clone();
        // Update instance current_node so UI highlights correctly
        let inst = self.instances.get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        inst.current_node = next;
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save token after service task: {}", e);
            }
        }

        self.remove_persisted_service_task(task_id).await;

        self.run_instance(instance_id, token).await
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
                .iter_mut()
                .find(|t| t.id == task_id)
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
                if let Some(inst) = self.instances.get_mut(&instance_id) {
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
                .iter_mut()
                .find(|t| t.id == task_id)
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
        let idx = self
            .pending_service_tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(EngineError::ServiceTaskNotFound(task_id))?;

        let task = &self.pending_service_tasks[idx];

        verify_lock_ownership(task_id, &task.worker_id, worker_id)?;

        let task = self.pending_service_tasks.remove(idx);
        let instance_id = task.instance_id;

        if let Some(inst) = self.instances.get_mut(&instance_id) {
            inst.audit_log.push(format!(
                "🚨 BPMN error '{}' thrown by worker '{}' at service task '{}'",
                error_code, worker_id, task.node_id
            ));
        }

        log::warn!(
            "Service task {task_id}: BPMN error '{error_code}' from worker '{worker_id}'"
        );
        
        self.remove_persisted_service_task(task_id).await;
        self.persist_instance(instance_id).await;

        Ok(())
    }
}
