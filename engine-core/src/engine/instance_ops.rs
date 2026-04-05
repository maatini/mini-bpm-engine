use std::collections::HashMap;
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::FileReference;

use super::{InstanceState, ProcessInstance, WorkflowEngine, PendingUserTask, PendingServiceTask, EngineStats};

impl WorkflowEngine {
    /// Returns summary statistics for monitoring dashboards.
    pub async fn get_stats(&self) -> EngineStats {
        let all_insts = self.instances.all().await;
        let mut running = 0; let mut comp = 0; let mut w_user = 0; let mut w_serv = 0;
        for lk in all_insts.values() {
            let st = &lk.read().await.state;
            match st {
                InstanceState::Running => running += 1,
                InstanceState::Completed | InstanceState::CompletedWithError { .. } => comp += 1,
                InstanceState::WaitingOnUserTask{..} => w_user += 1,
                InstanceState::WaitingOnServiceTask{..} => w_serv += 1,
                _ => {}
            }
        }
        EngineStats {
            definitions_count: self.definitions.len().await,
            instances_total: all_insts.len(),
            instances_running: running,
            instances_completed: comp,
            instances_waiting_user: w_user,
            instances_waiting_service: w_serv,
            pending_user_tasks: self.pending_user_tasks.len(),
            pending_service_tasks: self.pending_service_tasks.len(),
            pending_timers: self.pending_timers.len(),
            pending_message_catches: self.pending_message_catches.len(),
            persistence_errors: self.persistence_error_count.load(std::sync::atomic::Ordering::Relaxed),
            pending_retry_jobs: 0, // mpsc unbounded channel has no len(); always 0 in stats for now
        }
    }

    /// Returns the state of a process instance.
    pub async fn get_instance_state(&self, instance_id: Uuid) -> EngineResult<InstanceState> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            Ok(i_arc.read().await.state.clone())
        } else {
            Err(EngineError::NoSuchInstance(instance_id))
        }
    }

    /// Returns the audit log of a process instance.
    pub async fn get_audit_log(&self, instance_id: Uuid) -> EngineResult<Vec<String>> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            Ok(i_arc.read().await.audit_log.clone())
        } else {
            Err(EngineError::NoSuchInstance(instance_id))
        }
    }

    /// Returns all currently pending user tasks.
    pub fn get_pending_user_tasks(&self) -> Vec<PendingUserTask> {
        self.pending_user_tasks.iter().map(|it| it.value().clone()).collect()
    }

    /// Returns all pending service tasks (for debugging / admin).
    pub fn get_pending_service_tasks(&self) -> Vec<PendingServiceTask> {
        self.pending_service_tasks.iter().map(|it| it.value().clone()).collect()
    }

    /// Returns a list of all process instances (cloned).
    pub async fn list_instances(&self) -> Vec<ProcessInstance> {
        let all = self.instances.all().await;
        let mut out = Vec::with_capacity(all.len());
        for lk in all.values() {
            out.push(lk.read().await.clone());
        }
        out
    }

    /// Returns full details for a single process instance.
    pub async fn get_instance_details(&self, id: Uuid) -> EngineResult<ProcessInstance> {
        if let Some(i_arc) = self.instances.get(&id).await {
            Ok(i_arc.read().await.clone())
        } else {
            Err(EngineError::NoSuchInstance(id))
        }
    }

    /// Updates variables on a running process instance.
    ///
    /// - Keys with non-null values are created or overwritten.
    /// - Keys with `Value::Null` are removed from the instance variables.
    pub async fn update_instance_variables(
        &self,
        instance_id: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<()> {
        let old_state = if let Some(lk) = self.instances.get(&instance_id).await { Some(lk.read().await.clone()) } else { None };

        let updated_vars = {
            let instance_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut instance = instance_arc.write().await;

            let mut added: usize = 0;
            let mut modified: usize = 0;
            let mut deleted: usize = 0;

            for (key, value) in variables {
                if value.is_null() {
                    // Delete
                    if instance.variables.remove(&key).is_some() {
                        deleted += 1;
                    }
                } else {
                    match instance.variables.entry(key) {
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            // Update existing
                            e.insert(value);
                            modified += 1;
                        }
                        std::collections::hash_map::Entry::Vacant(e) => {
                            // Create new
                            e.insert(value);
                            added += 1;
                        }
                    }
                }
            }

            instance.audit_log.push(format!(
                "Variables updated: +{added} ~{modified} -{deleted}"
            ));

            tracing::info!(
                "Instance {}: variables updated (+{added} ~{modified} -{deleted})",
                instance_id
            );
            
            instance.variables.clone()
        };

        // With centralized tokens, we also update instance.tokens so that
        // when a pending task is completed, it picks up the latest variables.
        {
            let instance_arc = self.instances.get(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut instance = instance_arc.write().await;
            for token in instance.tokens.values_mut() {
                for (key, value) in &updated_vars {
                    if value.is_null() {
                        token.variables.remove(key);
                    } else {
                        token.variables.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::VariableUpdated,
            "Variables updated directly",
            crate::history::ActorType::User, // API call
            None,
            old_state.as_ref()
        ).await;

        self.persist_instance(instance_id).await;

        Ok(())
    }

    /// Deletes a process instance and cleans up associated pending tasks.
    pub async fn delete_instance(&self, instance_id: Uuid) -> EngineResult<()> {
        let removed_inst_arc = self.instances.remove(&instance_id).await.ok_or(EngineError::NoSuchInstance(instance_id))?;
        let removed_inst = removed_inst_arc.read().await.clone();

        if let Some(ref persistence) = self.persistence {
            // Delete associated files
            for value in removed_inst.variables.values() {
                if let Some(file_ref) = FileReference::from_variable_value(value) {
                    let _ = persistence.delete_file(&file_ref.object_key).await;
                }
            }

            // Delete associated user tasks from persistence
            for task in self.pending_user_tasks.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_user_task(task.task_id).await;
            }
            // Delete associated service tasks from persistence
            for task in self.pending_service_tasks.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_service_task(task.id).await;
            }
            // Delete associated timers from persistence
            for timer in self.pending_timers.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_timer(timer.id).await;
            }
            // Delete associated message catches from persistence
            for catch in self.pending_message_catches.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_message_catch(catch.id).await;
            }
            // Delete instance from persistence
            persistence.delete_instance(&instance_id.to_string()).await?;
        }

        // Clean up pending user tasks in memory
        self.pending_user_tasks.retain(|_, t| t.instance_id != instance_id);
        
        // Clean up pending service tasks in memory
        self.pending_service_tasks.retain(|_, t| t.instance_id != instance_id);

        // Clean up pending timers in memory
        self.pending_timers.retain(|_, t| t.instance_id != instance_id);

        // Clean up pending message catches in memory
        self.pending_message_catches.retain(|_, t| t.instance_id != instance_id);

        Ok(())
    }
}
