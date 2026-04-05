use std::collections::VecDeque;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::condition::evaluate_condition;
use crate::engine::boundary::setup_boundary_events;
use crate::engine::gateway::{
    execute_exclusive_gateway, execute_inclusive_gateway, execute_parallel_gateway,
};
use crate::engine::{WorkflowEngine, types::*};
use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, ListenerEvent, ProcessDefinition, Token};
use crate::script_runner;

// Helper function resolving next target
pub(crate) fn resolve_next_target(
    def: &ProcessDefinition,
    from: &str,
    variables: &std::collections::HashMap<String, serde_json::Value>,
) -> EngineResult<String> {
    def.next_nodes(from)
        .iter()
        .find(|f| {
            f.condition
                .as_ref()
                .map(|c| evaluate_condition(c, variables))
                .unwrap_or(true)
        })
        .map(|f| f.target.clone())
        .ok_or_else(|| {
            EngineError::InvalidDefinition(format!("No matching outgoing flow from '{from}'"))
        })
}

pub(crate) fn find_boundary_error_event(
    def: &ProcessDefinition,
    attached_to_node: &str,
    error_code: &str,
) -> Option<String> {
    def.nodes.iter().find_map(|(node_id, node)| {
        if let BpmnElement::BoundaryErrorEvent {
            attached_to,
            error_code: bound_err,
        } = node
        {
            if attached_to == attached_to_node
                && (bound_err.is_none() || bound_err.as_deref() == Some(error_code))
            {
                return Some(node_id.clone());
            }
        }
        None
    })
}

impl WorkflowEngine {
    /// Non-recursive batched execution loop.
    pub(crate) async fn run_instance_batch(
        &self,
        instance_id: Uuid,
        initial_token: Token,
    ) -> EngineResult<()> {
        let mut queue = VecDeque::new();
        queue.push_back(initial_token);

        while let Some(mut token) = queue.pop_front() {
            let old_state = if let Some(lk) = self.instances.get(&instance_id).await {
                Some(lk.read().await.clone())
            } else {
                None
            };
            let current_gateway_id = token.current_node.clone();

            let action = self.execute_step(instance_id, &mut token).await?;

            let (event_type, description) = match &action {
                NextAction::Continue(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Token advanced".to_string(),
                ),
                NextAction::ContinueMultiple(_) => (
                    crate::history::HistoryEventType::TokenForked,
                    "Token forked at gateway".to_string(),
                ),
                NextAction::WaitForJoin { .. } => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Token arrived at join".to_string(),
                ),
                NextAction::WaitForUser(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for user task".to_string(),
                ),
                NextAction::WaitForServiceTask(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for service task".to_string(),
                ),
                NextAction::WaitForTimer(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for timer".to_string(),
                ),
                NextAction::WaitForMessage(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for message".to_string(),
                ),
                NextAction::WaitForCallActivity { .. } => (
                    crate::history::HistoryEventType::CallActivityStarted,
                    "Spawned call activity".to_string(),
                ),
                NextAction::Complete => (
                    crate::history::HistoryEventType::BranchCompleted,
                    "Execution path completed".to_string(),
                ),
                NextAction::ErrorEnd { error_code } => (
                    crate::history::HistoryEventType::BranchCompleted,
                    format!("Execution path completed with error '{}'", error_code),
                ),
                NextAction::WaitForEventGroup(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for multiple alternative events".to_string(),
                ),
            };

            self.record_history_event(
                instance_id,
                event_type,
                &description,
                crate::history::ActorType::Engine,
                None,
                old_state.as_ref(),
            )
            .await;

            match action {
                NextAction::Continue(next_token) => {
                    queue.push_back(next_token);
                }
                NextAction::ContinueMultiple(forked_tokens) => {
                    let branch_count = forked_tokens.len();

                    self.register_join_barrier_if_needed(
                        instance_id,
                        &current_gateway_id,
                        branch_count,
                    )
                    .await?;

                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        // The original token has been consumed by the split gateway
                        if let Some(active) = inst
                            .active_tokens
                            .iter_mut()
                            .find(|at| at.token.id == token.id)
                        {
                            active.completed = true;
                        }

                        inst.state = InstanceState::ParallelExecution {
                            active_token_count: inst.active_tokens.len() + branch_count,
                        };
                        inst.current_node = current_gateway_id.clone();
                    }

                    for (idx, fork_token) in forked_tokens.into_iter().enumerate() {
                        self.register_active_token(
                            instance_id,
                            &current_gateway_id,
                            idx,
                            &fork_token,
                        )
                        .await?;
                        queue.push_back(fork_token);
                    }
                }
                NextAction::WaitForJoin {
                    gateway_id,
                    token: arrived_token,
                } => {
                    let merged = self
                        .arrive_at_join(instance_id, &gateway_id, arrived_token)
                        .await?;
                    if let Some(merged_token) = merged {
                        if let Some(inst_arc) = self.instances.get(&instance_id).await {
                            let mut inst = inst_arc.write().await;
                            inst.state = InstanceState::Running;
                            inst.current_node = gateway_id.clone();
                        }
                        queue.push_back(merged_token);
                    }
                }
                NextAction::WaitForUser(pending) => {
                    let task_id = pending.task_id;
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                            inst.state = InstanceState::WaitingOnUserTask { task_id };
                        }
                    }
                    self.pending_user_tasks.insert(task_id, pending);
                    self.persist_user_task(task_id).await;
                }
                NextAction::WaitForServiceTask(svc_task) => {
                    let task_id = svc_task.id;
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                            inst.state = InstanceState::WaitingOnServiceTask { task_id };
                        }
                    }
                    self.pending_service_tasks.insert(task_id, svc_task);
                    self.persist_service_task(task_id).await;
                }
                NextAction::WaitForTimer(pending) => {
                    let timer_id = pending.id;
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                            inst.state = InstanceState::WaitingOnTimer { timer_id };
                        }
                    }
                    self.pending_timers.insert(timer_id, pending);
                    self.persist_timer(timer_id).await;
                }
                NextAction::WaitForMessage(pending) => {
                    let message_id = pending.id;
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                            inst.state = InstanceState::WaitingOnMessage { message_id };
                        }
                    }
                    self.pending_message_catches.insert(message_id, pending);
                    self.persist_message_catch(message_id).await;
                }
                NextAction::WaitForEventGroup(actions) => {
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                            inst.state = InstanceState::WaitingOnEventBasedGateway;
                        }
                    }
                    for action in actions {
                        match action {
                            NextAction::WaitForTimer(pending) => {
                                let timer_id = pending.id;
                                self.pending_timers.insert(timer_id, pending);
                                self.persist_timer(timer_id).await;
                            }
                            NextAction::WaitForMessage(pending) => {
                                let message_id = pending.id;
                                self.pending_message_catches.insert(message_id, pending);
                                self.persist_message_catch(message_id).await;
                            }
                            _ => {} // EventBasedGateway validation ensures this won't happen
                        }
                    }
                }
                NextAction::WaitForCallActivity {
                    called_element,
                    token: call_token,
                } => {
                    // Start the child subprocess
                    let mut child_def_key = None;
                    let all_defs = self.definitions.all().await;
                    for (k, v) in &all_defs {
                        if v.id == called_element {
                            child_def_key = Some(*k);
                            break;
                        }
                    }

                    if let Some(child_key) = child_def_key {
                        // Put the parent sub process in waiting state

                        // We must first update the state so that the child can find our state if it finishes synchronously.
                        let sub_instance_id = Uuid::new_v4(); // placeholder, assigned by spawn and replaced

                        if let Some(inst_arc) = self.instances.get(&instance_id).await {
                            let mut inst = inst_arc.write().await;
                            if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                                inst.state = InstanceState::WaitingOnCallActivity {
                                    sub_instance_id,
                                    token: call_token.clone(),
                                };
                            }
                        }

                        tracing::info!(
                            "Instance {instance_id}: Triggering Call Activity '{}'",
                            called_element
                        );

                        // Now we need to start the child instance asynchronously or recursively.
                        // Actually, we can just call an internal start mechanism that accepts parent_id.
                        // We will add spawn_call_activity helper on WorkflowEngine.
                        match self
                            .spawn_call_activity(
                                child_key,
                                instance_id,
                                call_token.current_node.clone(),
                                call_token.variables.clone(),
                            )
                            .await
                        {
                            Ok(spawned_id) => {
                                // Update the actual spawned ID
                                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                                    let mut inst = inst_arc.write().await;
                                    if let InstanceState::WaitingOnCallActivity {
                                        sub_instance_id: ref mut sub,
                                        ..
                                    } = inst.state
                                    {
                                        *sub = spawned_id;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to start call activity '{}': {}",
                                    called_element,
                                    e
                                );
                                // Optional: we could throw a BPMN error or mark as crashed
                            }
                        }
                    } else {
                        tracing::error!(
                            "Call Activity target '{}' not found deployed.",
                            called_element
                        );
                    }
                }
                NextAction::Complete => {
                    self.complete_branch_token(instance_id, token.id).await?;
                    if self.all_tokens_completed(instance_id).await? {
                        if let Some(inst_arc) = self.instances.get(&instance_id).await {
                            let mut inst = inst_arc.write().await;
                            inst.state = InstanceState::Completed;
                            inst.audit_log.push(
                                "⏹ All tokens completed. Process fully completed.".to_string(),
                            );
                        }
                        self.record_history_event(
                            instance_id,
                            crate::history::HistoryEventType::InstanceCompleted,
                            "Process fully completed",
                            crate::history::ActorType::Engine,
                            None,
                            None,
                        )
                        .await;

                        // Check if we need to resume a parent instance (Call Activity)
                        // Note: we can't do this while borrowing `self.instances`, so we do this here.
                        // Actually wait we need to avoid borrowing conflicts!
                        // It's safe to just call the method here.
                    }
                }
                NextAction::ErrorEnd { error_code } => {
                    self.complete_branch_token(instance_id, token.id).await?;
                    if self.all_tokens_completed(instance_id).await? {
                        if let Some(inst_arc) = self.instances.get(&instance_id).await {
                            let mut inst = inst_arc.write().await;
                            inst.state = InstanceState::CompletedWithError {
                                error_code: error_code.clone(),
                            };
                            inst.audit_log.push(format!(
                                "💥 All tokens completed at Error End with code '{error_code}'"
                            ));
                        }
                    }
                }
            }
        } // end while

        // Flush persistence for the entire batch
        self.persist_instance(instance_id).await;

        // After batch finishes for this instance, if it completed, check parent
        let mut completed = false;
        let mut error_code_to_propagate = None;
        if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let inst = inst_arc.read().await;
            if matches!(inst.state, InstanceState::Completed) {
                completed = true;
            } else if let InstanceState::CompletedWithError { error_code } = &inst.state {
                completed = true;
                error_code_to_propagate = Some(error_code.clone());
            }
        }
        if completed {
            self.resume_parent_if_needed(instance_id, error_code_to_propagate)
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn execute_step(
        &self,
        instance_id: Uuid,
        token: &mut Token,
    ) -> EngineResult<NextAction> {
        let def_key = {
            let instance_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let instance = instance_arc.read().await;
            instance.definition_key
        };

        let def = self
            .definitions
            .get(&def_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(def_key))?;

        let current_id = token.current_node.clone();
        let element = def
            .get_node(&current_id)
            .ok_or_else(|| EngineError::NoSuchNode(current_id.clone()))?
            .clone();

        let def_clone = Arc::clone(&def);

        let mut start_audits = Vec::new();
        let script_engine = crate::engine::create_script_engine();
        script_runner::run_node_scripts(
            &script_engine,
            instance_id,
            token,
            &def_clone,
            &current_id,
            ListenerEvent::Start,
            &mut start_audits,
        )?;
        if let Some(inst_arc) = self.instances.get(&instance_id).await {
            let mut inst = inst_arc.write().await;
            inst.audit_log.append(&mut start_audits);
            // Only sync variables if a script listener potentially modified them
            if def_clone.listeners.contains_key(&current_id) {
                inst.variables = token.variables.clone();
            }
        }

        match &element {
            BpmnElement::StartEvent
            | BpmnElement::TimerStartEvent(_)
            | BpmnElement::MessageStartEvent { .. } => {
                tracing::debug!("Passing through start event '{current_id}'");
                let next = resolve_next_target(&def_clone, &current_id, &token.variables)?;
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                token.current_node = next.clone();
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
                Ok(NextAction::Continue(token.clone()))
            }

            BpmnElement::EndEvent => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.audit_log
                    .push(format!("⏹ Process completed at end event '{current_id}'"));
                tracing::info!("Instance {instance_id}: reached end event '{current_id}'");
                Ok(NextAction::Complete)
            }

            BpmnElement::UserTask(assignee) => {
                let pending_timers =
                    setup_boundary_events(&def_clone, &current_id, instance_id, token);
                for t in pending_timers {
                    self.pending_timers.insert(t.id, t);
                }

                let pending = PendingUserTask {
                    task_id: Uuid::new_v4(),
                    instance_id,
                    node_id: current_id.clone(),
                    assignee: assignee.clone(),
                    token_id: token.id,
                    created_at: Utc::now(),
                };

                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.tokens.insert(token.id, token.clone());
                inst.audit_log.push(format!(
                    "👤 User task '{current_id}' assigned to '{assignee}' — waiting (task_id: {})",
                    pending.task_id
                ));
                tracing::info!(
                    "Instance {instance_id}: user task '{current_id}' pending for '{assignee}'"
                );

                Ok(NextAction::WaitForUser(pending))
            }

            BpmnElement::ServiceTask { topic } => {
                let pending_timers =
                    setup_boundary_events(&def_clone, &current_id, instance_id, token);
                for t in pending_timers {
                    self.pending_timers.insert(t.id, t);
                }

                let svc_task = PendingServiceTask {
                    id: Uuid::new_v4(),
                    instance_id,
                    definition_key: def_key,
                    node_id: current_id.clone(),
                    topic: topic.clone(),
                    token_id: token.id,
                    variables_snapshot: token.variables.clone(),
                    created_at: Utc::now(),
                    worker_id: None,
                    lock_expiration: None,
                    retries: 3,
                    error_message: None,
                    error_details: None,
                };

                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.tokens.insert(token.id, token.clone());
                inst.audit_log.push(format!(
                    "🔗 Service task '{current_id}' created for topic '{topic}' (task_id: {})",
                    svc_task.id
                ));
                tracing::info!(
                    "Instance {instance_id}: service task '{current_id}' pending for topic '{topic}'"
                );

                Ok(NextAction::WaitForServiceTask(svc_task))
            }

            BpmnElement::ParallelGateway => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let action = execute_parallel_gateway(&def_clone, &current_id, token)?;
                if let NextAction::ContinueMultiple(ref f) = action {
                    let inst_arc = self
                        .instances
                        .get(&instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(instance_id))?;
                    let mut inst = inst_arc.write().await;
                    inst.current_node = current_id.clone();
                    inst.audit_log.push(format!(
                        "■ Parallel gateway '{current_id}' → forked to {} path(s)",
                        f.len()
                    ));
                }
                Ok(action)
            }

            BpmnElement::ExclusiveGateway { default } => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let action = execute_exclusive_gateway(&def_clone, &current_id, token, default)?;
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.audit_log.push(format!(
                    "◆ Exclusive gateway '{current_id}' → took path to '{}'",
                    token.current_node
                ));
                inst.current_node = token.current_node.clone();
                Ok(action)
            }

            BpmnElement::InclusiveGateway => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let action = execute_inclusive_gateway(&def_clone, &current_id, token)?;
                if let NextAction::ContinueMultiple(ref f) = action {
                    let inst_arc = self
                        .instances
                        .get(&instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(instance_id))?;
                    let mut inst = inst_arc.write().await;
                    inst.current_node = current_id.clone();
                    inst.audit_log.push(format!(
                        "◇ Inclusive gateway '{current_id}' → forked to {} path(s)",
                        f.len()
                    ));
                }
                Ok(action)
            }

            BpmnElement::EventBasedGateway => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let mut actions = Vec::new();
                for sf in def_clone.next_nodes(&current_id) {
                    let target_node = sf.target.clone();
                    if let Some(target_element) = def_clone.get_node(&target_node) {
                        match target_element {
                            BpmnElement::TimerCatchEvent(dur) => {
                                let pending = PendingTimer {
                                    id: Uuid::new_v4(),
                                    instance_id,
                                    node_id: target_node.clone(),
                                    expires_at: Utc::now()
                                        + chrono::Duration::from_std(*dur)
                                            .unwrap_or(chrono::Duration::seconds(0)),
                                    token_id: token.id,
                                };
                                actions.push(NextAction::WaitForTimer(pending));
                            }
                            BpmnElement::MessageCatchEvent { message_name } => {
                                let pending = PendingMessageCatch {
                                    id: Uuid::new_v4(),
                                    instance_id,
                                    node_id: target_node.clone(),
                                    message_name: message_name.clone(),
                                    token_id: token.id,
                                };
                                actions.push(NextAction::WaitForMessage(pending));
                            }
                            _ => {
                                return Err(EngineError::InvalidDefinition(format!(
                                    "EventBasedGateway target '{}' is not a catch event",
                                    target_node
                                )));
                            }
                        }
                    }
                }

                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.tokens.insert(token.id, token.clone());
                inst.audit_log.push(format!(
                    "⭮ Event-based gateway '{current_id}' waiting for {} alternative events",
                    actions.len()
                ));
                Ok(NextAction::WaitForEventGroup(actions))
            }

            BpmnElement::TimerCatchEvent(dur) => {
                let pending = PendingTimer {
                    id: Uuid::new_v4(),
                    instance_id,
                    node_id: current_id.clone(),
                    expires_at: Utc::now()
                        + chrono::Duration::from_std(*dur).unwrap_or(chrono::Duration::seconds(0)),
                    token_id: token.id,
                };
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.tokens.insert(token.id, token.clone());
                inst.audit_log
                    .push(format!("⏱ Timer catch event '{current_id}' — waiting"));
                Ok(NextAction::WaitForTimer(pending))
            }

            BpmnElement::MessageCatchEvent { message_name } => {
                let pending = PendingMessageCatch {
                    id: Uuid::new_v4(),
                    instance_id,
                    node_id: current_id.clone(),
                    message_name: message_name.clone(),
                    token_id: token.id,
                };
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.tokens.insert(token.id, token.clone());
                inst.audit_log.push(format!(
                    "✉️ Message catch event '{current_id}' waiting for '{message_name}'"
                ));
                Ok(NextAction::WaitForMessage(pending))
            }

            BpmnElement::CallActivity { called_element }
            | BpmnElement::SubProcess { called_element } => {
                let pending_timers =
                    setup_boundary_events(&def_clone, &current_id, instance_id, token);
                for t in pending_timers {
                    self.pending_timers.insert(t.id, t);
                }

                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.audit_log.push(format!(
                    "🔗 Call Activity/Sub Process '{current_id}' invoking '{called_element}'"
                ));
                tracing::info!(
                    "Instance {instance_id}: '{current_id}' invoking '{called_element}'"
                );

                Ok(NextAction::WaitForCallActivity {
                    called_element: called_element.clone(),
                    token: token.clone(),
                })
            }

            BpmnElement::ErrorEndEvent { error_code } => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = current_id.clone();
                inst.audit_log.push(format!(
                    "💥 Process completed at error end '{current_id}' with error '{error_code}'"
                ));
                Ok(NextAction::ErrorEnd {
                    error_code: error_code.clone(),
                })
            }

            BpmnElement::BoundaryTimerEvent { .. } | BpmnElement::BoundaryErrorEvent { .. } => {
                let next = resolve_next_target(&def_clone, &current_id, &token.variables)?;
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)
                    .await?;
                token.current_node = next.clone();
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
                Ok(NextAction::Continue(token.clone()))
            }
        }
    }

    // ----- Helper: Token-Registry & Parallel Execution ---------------------

    pub(crate) async fn register_join_barrier_if_needed(
        &self,
        instance_id: Uuid,
        split_gateway_id: &str,
        branch_count: usize,
    ) -> EngineResult<()> {
        let def_key_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let def_key = def_key_arc.read().await.definition_key;
        let def = self
            .definitions
            .get(&def_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(def_key))?
            .clone();

        if let Some(join_id) = self.find_downstream_join(&def, split_gateway_id) {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.join_barriers.insert(
                join_id.clone(),
                JoinBarrier {
                    gateway_node_id: join_id.clone(),
                    expected_count: branch_count,
                    arrived_tokens: Vec::new(),
                },
            );
            tracing::debug!(
                "Registered JoinBarrier for join '{join_id}' (expected: {branch_count})"
            );
        }
        Ok(())
    }

    fn same_gateway_type(a: &crate::model::BpmnElement, b: &crate::model::BpmnElement) -> bool {
        matches!(
            (a, b),
            (
                crate::model::BpmnElement::ExclusiveGateway { .. },
                crate::model::BpmnElement::ExclusiveGateway { .. }
            ) | (
                crate::model::BpmnElement::InclusiveGateway,
                crate::model::BpmnElement::InclusiveGateway
            ) | (
                crate::model::BpmnElement::ParallelGateway,
                crate::model::BpmnElement::ParallelGateway
            )
        )
    }

    pub(crate) fn find_downstream_join(
        &self,
        def: &ProcessDefinition,
        start_node: &str,
    ) -> Option<String> {
        let split_element = def.nodes.get(start_node)?;
        let mut visited = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<(String, usize)> =
            std::collections::VecDeque::new(); // (node_id, depth)

        for flow in def.next_nodes(start_node) {
            queue.push_back((flow.target.clone(), 1));
        }

        while let Some((node, depth)) = queue.pop_front() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());

            if let Some(element) = def.nodes.get(&node) {
                if def.is_join_gateway(&node) && Self::same_gateway_type(split_element, element) {
                    if depth == 1 {
                        return Some(node.clone());
                    }
                    for flow in def.next_nodes(&node) {
                        queue.push_back((flow.target.clone(), depth - 1));
                    }
                    continue;
                }

                if def.is_split_gateway(&node) && Self::same_gateway_type(split_element, element) {
                    for flow in def.next_nodes(&node) {
                        queue.push_back((flow.target.clone(), depth + 1));
                    }
                    continue;
                }
            }

            for flow in def.next_nodes(&node) {
                queue.push_back((flow.target.clone(), depth));
            }
        }
        None
    }

    pub(crate) async fn register_active_token(
        &self,
        instance_id: Uuid,
        fork_id: &str,
        branch_index: usize,
        token: &Token,
    ) -> EngineResult<()> {
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.active_tokens.push(ActiveToken {
            token: token.clone(),
            fork_id: Some(fork_id.to_string()),
            branch_index,
            completed: false,
        });
        Ok(())
    }

    pub(crate) async fn arrive_at_join(
        &self,
        instance_id: Uuid,
        gateway_id: &str,
        token: Token,
    ) -> EngineResult<Option<Token>> {
        let def_key_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let def_key = def_key_arc.read().await.definition_key;
        let def = self
            .definitions
            .get(&def_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(def_key))?
            .clone();

        let structural_expected = def.incoming_flow_count(gateway_id);
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;

        let expected_count;
        let current_arrived;

        {
            let barrier = inst
                .join_barriers
                .entry(gateway_id.to_string())
                .or_insert_with(|| JoinBarrier {
                    gateway_node_id: gateway_id.to_string(),
                    expected_count: structural_expected,
                    arrived_tokens: Vec::new(),
                });
            expected_count = barrier.expected_count;
            barrier.arrived_tokens.push(token.clone());
            current_arrived = barrier.arrived_tokens.len();
        }

        inst.audit_log.push(format!(
            "➔ Token arrived at join '{}' ({}/{})",
            gateway_id, current_arrived, expected_count
        ));

        if current_arrived >= expected_count {
            let all_tokens = inst
                .join_barriers
                .remove(gateway_id)
                .ok_or_else(|| {
                    EngineError::InvalidDefinition(format!(
                        "Join barrier for gateway '{}' not found in instance {}",
                        gateway_id, instance_id
                    ))
                })?
                .arrived_tokens;

            for t in &all_tokens {
                if let Some(active) = inst.active_tokens.iter_mut().find(|at| at.token.id == t.id) {
                    active.completed = true;
                }
            }

            let mut merged_vars = std::collections::HashMap::new();
            for t in &all_tokens {
                merged_vars.extend(t.variables.clone());
            }

            let mut merged_token = Token::with_variables(gateway_id, merged_vars);
            merged_token.is_merged = true;
            inst.audit_log.push(format!(
                "🔗 Join '{}' completed. Tokens merged.",
                gateway_id
            ));

            drop(inst);

            self.record_history_event(
                instance_id,
                crate::history::HistoryEventType::TokenJoined,
                &format!("Joined {} tokens at '{}'", current_arrived, gateway_id),
                crate::history::ActorType::Engine,
                None,
                None,
            )
            .await;

            Ok(Some(merged_token))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn complete_branch_token(
        &self,
        instance_id: Uuid,
        token_id: Uuid,
    ) -> EngineResult<()> {
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        if let Some(active) = inst
            .active_tokens
            .iter_mut()
            .find(|at| at.token.id == token_id)
        {
            active.completed = true;
        }
        Ok(())
    }

    pub(crate) async fn all_tokens_completed(&self, instance_id: Uuid) -> EngineResult<bool> {
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let inst = inst_arc.read().await;
        if inst.active_tokens.is_empty() {
            // Linear flow
            return Ok(true);
        }
        Ok(inst.active_tokens.iter().all(|t| t.completed))
    }

    /// Helper: runs End scripts, commits variables to instance state.
    pub(crate) async fn run_end_scripts(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def: &ProcessDefinition,
        node_id: &str,
    ) -> EngineResult<()> {
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        let crate::ProcessInstance {
            audit_log,
            variables,
            ..
        } = &mut *inst;
        let script_engine = crate::engine::create_script_engine();
        crate::script_runner::run_end_scripts(
            &script_engine,
            instance_id,
            token,
            def,
            node_id,
            audit_log,
            variables,
        )
    }
}
