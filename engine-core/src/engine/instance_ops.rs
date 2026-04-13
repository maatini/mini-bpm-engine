use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::FileReference;
use crate::domain::{EngineError, EngineResult};
use crate::persistence::CompletedInstanceQuery;

use super::WorkflowEngine;
use crate::runtime::{
    EngineStats, InstanceState, PendingMessageCatch, PendingServiceTask, PendingTimer,
    PendingUserTask, ProcessInstance,
};

impl WorkflowEngine {
    /// Returns summary statistics for monitoring dashboards.
    pub async fn get_stats(&self) -> EngineStats {
        let all_insts = self.instances.all().await;
        let mut running = 0;
        let mut comp = 0;
        let mut w_user = 0;
        let mut w_serv = 0;
        for lk in all_insts.values() {
            let st = &lk.read().await.state;
            match st {
                InstanceState::Running => running += 1,
                InstanceState::Completed | InstanceState::CompletedWithError { .. } => comp += 1,
                InstanceState::WaitingOnUserTask { .. } => w_user += 1,
                InstanceState::WaitingOnServiceTask { .. } => w_serv += 1,
                _ => {}
            }
        }
        EngineStats {
            definitions_count: self.definitions.len(),
            instances_total: all_insts.len(),
            instances_running: running,
            instances_completed: comp,
            instances_waiting_user: w_user,
            instances_waiting_service: w_serv,
            pending_user_tasks: self.pending_user_tasks.len(),
            pending_service_tasks: self.pending_service_tasks.len(),
            pending_timers: self.pending_timers.len(),
            pending_message_catches: self.pending_message_catches.len(),
            persistence_errors: self
                .persistence_error_count
                .load(std::sync::atomic::Ordering::Relaxed),
            pending_retry_jobs: 0, // mpsc unbounded channel has no len(); always 0 in stats for now
        }
    }

    /// Returns the state of a process instance (checks archive if not in active map).
    pub async fn get_instance_state(&self, instance_id: Uuid) -> EngineResult<InstanceState> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            return Ok(i_arc.read().await.state.clone());
        }
        // Fall back to archived instances
        let inst = self.get_instance_or_archived(instance_id).await?;
        Ok(inst.state)
    }

    /// Returns the audit log of a process instance (checks archive if not in active map).
    pub async fn get_audit_log(&self, instance_id: Uuid) -> EngineResult<Vec<String>> {
        if let Some(i_arc) = self.instances.get(&instance_id).await {
            return Ok(i_arc.read().await.audit_log.clone());
        }
        let inst = self.get_instance_or_archived(instance_id).await?;
        Ok(inst.audit_log)
    }

    /// Returns all currently pending user tasks.
    pub fn get_pending_user_tasks(&self) -> Vec<PendingUserTask> {
        self.pending_user_tasks
            .iter()
            .map(|it| it.value().clone())
            .collect()
    }

    /// Returns all pending service tasks (for debugging / admin).
    pub fn get_pending_service_tasks(&self) -> Vec<PendingServiceTask> {
        self.pending_service_tasks
            .iter()
            .map(|it| it.value().clone())
            .collect()
    }

    /// Returns all currently pending timers.
    pub fn get_pending_timers(&self) -> Vec<PendingTimer> {
        self.pending_timers
            .iter()
            .map(|it| it.value().clone())
            .collect()
    }

    /// Returns all currently pending message catch events.
    pub fn get_pending_message_catches(&self) -> Vec<PendingMessageCatch> {
        self.pending_message_catches
            .iter()
            .map(|it| it.value().clone())
            .collect()
    }

    /// Query archived (completed) instances via persistence.
    pub async fn query_completed_instances(
        &self,
        query: CompletedInstanceQuery,
    ) -> EngineResult<Vec<ProcessInstance>> {
        if let Some(p) = &self.persistence {
            p.query_completed_instances(query).await
        } else {
            Ok(vec![])
        }
    }

    /// Load a single instance by ID — checks active instances first, then archive.
    pub async fn get_instance_or_archived(&self, id: Uuid) -> EngineResult<ProcessInstance> {
        // Check active instances first
        if let Some(inst_arc) = self.instances.get(&id).await {
            return Ok(inst_arc.read().await.clone());
        }
        // Fall back to archived instances
        if let Some(p) = &self.persistence
            && let Some(inst) = p.get_completed_instance(&id.to_string()).await?
        {
            return Ok(inst);
        }
        Err(EngineError::NoSuchInstance(id))
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

    /// Returns full details for a single process instance (checks archive if not in active map).
    pub async fn get_instance_details(&self, id: Uuid) -> EngineResult<ProcessInstance> {
        self.get_instance_or_archived(id).await
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
        let old_state = if let Some(lk) = self.instances.get(&instance_id).await {
            Some(lk.read().await.clone())
        } else {
            None
        };

        let updated_vars = {
            let instance_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
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

            instance.push_audit_log(format!(
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
            let instance_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
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
            old_state.as_ref(),
        )
        .await;

        self.persist_instance(instance_id).await;

        Ok(())
    }

    /// Suspends a running process instance.
    ///
    /// While suspended, timers won't fire and task completions are rejected.
    /// The previous state is stored inside the `Suspended` variant so that
    /// `resume_instance` can restore it.
    pub async fn suspend_instance(&self, instance_id: Uuid) -> EngineResult<()> {
        let old_state = {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let inst = inst_arc.read().await;
            Some(inst.clone())
        };

        {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;

            // Cannot suspend an already-completed or already-suspended instance
            match &inst.state {
                InstanceState::Completed | InstanceState::CompletedWithError { .. } => {
                    return Err(EngineError::AlreadyCompleted);
                }
                InstanceState::Suspended { .. } => {
                    return Err(EngineError::InstanceSuspended(instance_id));
                }
                _ => {}
            }

            let previous = inst.state.clone();
            inst.state = InstanceState::Suspended {
                previous_state: Box::new(previous),
            };
            inst.push_audit_log("⏸ Instance suspended".to_string());
        }

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceSuspended,
            "Instance suspended",
            crate::history::ActorType::User,
            None,
            old_state.as_ref(),
        )
        .await;

        self.persist_instance(instance_id).await;

        tracing::info!("Instance {instance_id}: suspended");
        Ok(())
    }

    /// Resumes a previously suspended instance, restoring its prior state.
    pub async fn resume_instance(&self, instance_id: Uuid) -> EngineResult<()> {
        let old_state = {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let inst = inst_arc.read().await;
            Some(inst.clone())
        };

        {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;

            match inst.state.clone() {
                InstanceState::Suspended { previous_state } => {
                    inst.state = *previous_state;
                }
                _ => {
                    return Err(EngineError::InvalidDefinition(
                        "Instance is not suspended".into(),
                    ));
                }
            }

            inst.push_audit_log("▶ Instance resumed".to_string());
        }

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceResumed,
            "Instance resumed",
            crate::history::ActorType::User,
            None,
            old_state.as_ref(),
        )
        .await;

        self.persist_instance(instance_id).await;

        tracing::info!("Instance {instance_id}: resumed");
        Ok(())
    }

    /// Moves the active token to a different node in the process definition.
    ///
    /// This is equivalent to Camunda's "Modify Process Instance" — one of the
    /// most powerful admin/ops tools. It:
    /// 1. Validates that the target node exists in the definition.
    /// 2. Cancels all pending wait states (user tasks, service tasks, timers,
    ///    message catches) for this instance.
    /// 3. Creates a fresh token at the target node.
    /// 4. Optionally merges additional variables.
    /// 5. Starts execution from the target node via `run_instance_batch`.
    pub async fn move_token(
        &self,
        instance_id: Uuid,
        target_node_id: &str,
        variables: HashMap<String, Value>,
        cancel_current: bool,
    ) -> EngineResult<()> {
        // --- 1. Validate instance exists and is not completed ---
        let (def_key, old_current_node) = {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let inst = inst_arc.read().await;

            match &inst.state {
                InstanceState::Completed | InstanceState::CompletedWithError { .. } => {
                    return Err(EngineError::AlreadyCompleted);
                }
                InstanceState::Suspended { .. } => {
                    return Err(EngineError::InstanceSuspended(instance_id));
                }
                _ => {}
            }

            (inst.definition_key, inst.current_node.clone())
        };

        // --- 2. Validate target node exists in definition ---
        let def = self
            .definitions
            .get(&def_key)
            .ok_or(EngineError::NoSuchDefinition(def_key))?;

        if !def.nodes.contains_key(target_node_id) {
            return Err(EngineError::NoSuchNode(target_node_id.to_string()));
        }

        let old_state = if let Some(lk) = self.instances.get(&instance_id).await {
            Some(lk.read().await.clone())
        } else {
            None
        };

        // --- 3. Cancel all pending wait states if requested ---
        if cancel_current {
            // Remove pending user tasks
            let user_task_ids: Vec<Uuid> = self
                .pending_user_tasks
                .iter()
                .filter(|t| t.instance_id == instance_id)
                .map(|t| t.task_id)
                .collect();
            for tid in &user_task_ids {
                self.pending_user_tasks.remove(tid);
                if let Some(p) = &self.persistence {
                    let _ = p.delete_user_task(*tid).await;
                }
            }

            // Remove pending service tasks
            let service_task_ids: Vec<Uuid> = self
                .pending_service_tasks
                .iter()
                .filter(|t| t.instance_id == instance_id)
                .map(|t| t.id)
                .collect();
            for tid in &service_task_ids {
                self.pending_service_tasks.remove(tid);
                if let Some(p) = &self.persistence {
                    let _ = p.delete_service_task(*tid).await;
                }
            }

            // Remove pending timers
            let timer_ids: Vec<Uuid> = self
                .pending_timers
                .iter()
                .filter(|t| t.instance_id == instance_id)
                .map(|t| t.id)
                .collect();
            for tid in &timer_ids {
                self.pending_timers.remove(tid);
                if let Some(p) = &self.persistence {
                    let _ = p.delete_timer(*tid).await;
                }
            }

            // Remove pending message catches
            let msg_ids: Vec<Uuid> = self
                .pending_message_catches
                .iter()
                .filter(|t| t.instance_id == instance_id)
                .map(|t| t.id)
                .collect();
            for tid in &msg_ids {
                self.pending_message_catches.remove(tid);
                if let Some(p) = &self.persistence {
                    let _ = p.delete_message_catch(*tid).await;
                }
            }
        }

        // --- 4. Create a fresh token at the target node ---
        let mut token_vars = {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let inst = inst_arc.read().await;
            inst.variables.clone()
        };
        // Merge provided variables
        for (k, v) in variables {
            if v.is_null() {
                token_vars.remove(&k);
            } else {
                token_vars.insert(k, v);
            }
        }

        let token = crate::domain::Token {
            id: Uuid::new_v4(),
            current_node: target_node_id.to_string(),
            variables: token_vars.clone(),
            is_merged: false,
        };

        // --- 5. Reset instance state ---
        {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;

            // Clear old tokens and active_tokens
            inst.tokens.clear();
            inst.active_tokens.clear();
            inst.join_barriers.clear();
            inst.multi_instance_state.clear();

            // Insert the new token
            inst.tokens.insert(token.id, token.clone());

            // Update instance state
            inst.state = InstanceState::Running;
            inst.current_node = target_node_id.to_string();

            // Sync variables to instance level
            inst.variables = token_vars;

            inst.push_audit_log(format!(
                "🎯 Token moved: '{}' → '{}'",
                old_current_node, target_node_id
            ));
        }

        // --- 6. Record history ---
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::TokenMoved,
            &format!(
                "Token manually moved from '{}' to '{}'",
                old_current_node, target_node_id
            ),
            crate::history::ActorType::User,
            None,
            old_state.as_ref(),
        )
        .await;

        self.persist_instance(instance_id).await;

        tracing::info!(
            "Instance {}: token moved from '{}' to '{}'",
            instance_id,
            old_current_node,
            target_node_id
        );

        // --- 7. Start execution from target node ---
        self.run_instance_batch(instance_id, token).await
    }

    /// Deletes a process instance and cleans up associated pending tasks.
    pub async fn delete_instance(&self, instance_id: Uuid) -> EngineResult<()> {
        let removed_inst_arc = self
            .instances
            .remove(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let removed_inst = removed_inst_arc.read().await.clone();

        if let Some(ref persistence) = self.persistence {
            // Delete associated files
            for value in removed_inst.variables.values() {
                if let Some(file_ref) = FileReference::from_variable_value(value) {
                    let _ = persistence.delete_file(&file_ref.object_key).await;
                }
            }

            // Delete associated user tasks from persistence
            for task in self
                .pending_user_tasks
                .iter()
                .filter(|t| t.instance_id == instance_id)
            {
                let _ = persistence.delete_user_task(task.task_id).await;
            }
            // Delete associated service tasks from persistence
            for task in self
                .pending_service_tasks
                .iter()
                .filter(|t| t.instance_id == instance_id)
            {
                let _ = persistence.delete_service_task(task.id).await;
            }
            // Delete associated timers from persistence
            for timer in self
                .pending_timers
                .iter()
                .filter(|t| t.instance_id == instance_id)
            {
                let _ = persistence.delete_timer(timer.id).await;
            }
            // Delete associated message catches from persistence
            for catch in self
                .pending_message_catches
                .iter()
                .filter(|t| t.instance_id == instance_id)
            {
                let _ = persistence.delete_message_catch(catch.id).await;
            }
            // Delete instance from persistence
            persistence
                .delete_instance(&instance_id.to_string())
                .await?;
        }

        // Clean up pending user tasks in memory
        self.pending_user_tasks
            .retain(|_, t| t.instance_id != instance_id);

        // Clean up pending service tasks in memory
        self.pending_service_tasks
            .retain(|_, t| t.instance_id != instance_id);

        // Clean up pending timers in memory
        self.pending_timers
            .retain(|_, t| t.instance_id != instance_id);

        // Clean up pending message catches in memory
        self.pending_message_catches
            .retain(|_, t| t.instance_id != instance_id);

        Ok(())
    }
}
