use std::collections::VecDeque;
use std::sync::Arc;

use uuid::Uuid;

use crate::condition::evaluate_condition;
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
            && attached_to == attached_to_node
            && (bound_err.is_none() || bound_err.as_deref() == Some(error_code))
        {
            return Some(node_id.clone());
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
        let mut step_count: u32 = 0;

        while let Some(mut token) = queue.pop_front() {
            step_count += 1;

            // Cooperative scheduling: yield to Tokio every N steps
            if step_count.is_multiple_of(crate::engine::types::YIELD_EVERY_N_STEPS) {
                tokio::task::yield_now().await;
            }

            // Hard abort: prevent infinite BPMN loops
            if step_count > crate::engine::types::MAX_EXECUTION_STEPS {
                tracing::error!(
                    "Instance {} exceeded {} execution steps — aborting (possible infinite loop)",
                    instance_id,
                    crate::engine::types::MAX_EXECUTION_STEPS
                );
                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    inst.state = InstanceState::CompletedWithError {
                        error_code: "EXECUTION_LIMIT_EXCEEDED".to_string(),
                    };
                    inst.push_audit_log(format!(
                        "ABORTED: Exceeded {} execution steps",
                        crate::engine::types::MAX_EXECUTION_STEPS
                    ));
                }
                self.persist_instance(instance_id).await;
                return Err(EngineError::ExecutionLimitExceeded(format!(
                    "Instance {} exceeded execution step limit ({})",
                    instance_id,
                    crate::engine::types::MAX_EXECUTION_STEPS
                )));
            }

            let old_snapshot = if let Some(lk) = self.instances.get(&instance_id).await {
                Some(crate::history::DiffSnapshot::from_instance(
                    &*lk.read().await,
                ))
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
                NextAction::Terminate => (
                    crate::history::HistoryEventType::BranchCompleted,
                    "Process terminated".to_string(),
                ),
                NextAction::WaitForEventGroup(_) => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Waiting for multiple alternative events".to_string(),
                ),
                NextAction::MultiInstanceFork { .. } => (
                    crate::history::HistoryEventType::TokenForked,
                    "Spawned Multi-Instance parallel tokens".to_string(),
                ),
                NextAction::MultiInstanceNext { .. } => (
                    crate::history::HistoryEventType::TokenAdvanced,
                    "Advanced to next Multi-Instance sequential iteration".to_string(),
                ),
            };

            self.record_history_event_from_snapshot(
                instance_id,
                event_type,
                &description,
                crate::history::ActorType::Engine,
                None,
                old_snapshot.as_ref(),
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
                    self.persist_instance(instance_id).await;
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
                    self.persist_instance(instance_id).await;
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
                    self.persist_instance(instance_id).await;
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
                    self.persist_instance(instance_id).await;
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
                    self.persist_instance(instance_id).await;
                }
                NextAction::WaitForCallActivity {
                    called_element,
                    token: call_token,
                } => {
                    // Start the child subprocess
                    let mut child_def_key = None;
                    let all_defs = self.definitions.all();
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
                    self.persist_instance(instance_id).await;
                }
                NextAction::Complete => {
                    self.complete_branch_token(instance_id, token.id).await?;

                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        inst.tokens.remove(&token.id);
                    }

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

                        // Parent resume happens after batch completes (see end of run_instance_batch)
                    }
                    self.persist_instance(instance_id).await;
                }
                NextAction::ErrorEnd { error_code } => {
                    self.complete_branch_token(instance_id, token.id).await?;

                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        inst.tokens.remove(&token.id);
                    }

                    if self.all_tokens_completed(instance_id).await?
                        && let Some(inst_arc) = self.instances.get(&instance_id).await
                    {
                        let mut inst = inst_arc.write().await;
                        inst.state = InstanceState::CompletedWithError {
                            error_code: error_code.clone(),
                        };
                        inst.audit_log.push(format!(
                            "💥 All tokens completed at Error End with code '{error_code}'"
                        ));
                    }
                    self.persist_instance(instance_id).await;
                }
                NextAction::Terminate => {
                    // Cancel ALL pending items for this instance
                    self.pending_user_tasks
                        .retain(|_, t| t.instance_id != instance_id);
                    self.pending_service_tasks
                        .retain(|_, t| t.instance_id != instance_id);
                    self.pending_timers
                        .retain(|_, t| t.instance_id != instance_id);
                    self.pending_message_catches
                        .retain(|_, t| t.instance_id != instance_id);

                    // Mark all active tokens as completed
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        for at in inst.active_tokens.iter_mut() {
                            at.completed = true;
                        }
                        inst.state = InstanceState::Completed;
                        inst.audit_log
                            .push("⛔ Process terminated. All tokens killed.".to_string());
                    }

                    // Clear the execution queue — no more tokens should be processed
                    queue.clear();

                    self.record_history_event(
                        instance_id,
                        crate::history::HistoryEventType::InstanceCompleted,
                        "Process terminated via TerminateEndEvent",
                        crate::history::ActorType::Engine,
                        None,
                        None,
                    )
                    .await;
                    self.persist_instance(instance_id).await;
                }
                NextAction::MultiInstanceFork { node_id, tokens } => {
                    let branch_count = tokens.len();
                    let join_id = format!("MI_JOIN_{node_id}");
                    let inst_arc = self
                        .instances
                        .get(&instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(instance_id))?;
                    {
                        let mut inst = inst_arc.write().await;
                        inst.join_barriers.insert(
                            join_id.clone(),
                            JoinBarrier {
                                gateway_node_id: join_id.clone(),
                                expected_count: branch_count,
                                arrived_tokens: Vec::new(),
                            },
                        );
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
                    }
                    for (idx, fork_token) in tokens.into_iter().enumerate() {
                        self.register_active_token(instance_id, &node_id, idx, &fork_token)
                            .await?;
                        queue.push_back(fork_token);
                    }
                }
                NextAction::MultiInstanceNext { token, .. } => {
                    queue.push_back(token);
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
                self.handle_start_event(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::EndEvent => {
                self.handle_end_event(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::TerminateEndEvent => {
                self.handle_terminate_end_event(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::ErrorEndEvent { error_code } => {
                self.handle_error_end_event(instance_id, token, &def_clone, &current_id, error_code)
                    .await
            }
            BpmnElement::UserTask(assignee) => {
                self.handle_user_task(instance_id, token, &def_clone, &current_id, assignee)
                    .await
            }
            BpmnElement::ScriptTask { script, .. } => {
                self.handle_script_task(instance_id, token, &def_clone, &current_id, script)
                    .await
            }
            BpmnElement::SendTask { message_name, .. } => {
                self.handle_send_task(instance_id, token, &def_clone, &current_id, message_name)
                    .await
            }
            BpmnElement::ServiceTask { topic, .. } => {
                self.handle_service_task(instance_id, token, &def_clone, &current_id, topic)
                    .await
            }
            BpmnElement::ParallelGateway => {
                self.handle_parallel_gateway(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::ExclusiveGateway { default } => {
                self.handle_exclusive_gateway(instance_id, token, &def_clone, &current_id, default)
                    .await
            }
            BpmnElement::InclusiveGateway => {
                self.handle_inclusive_gateway(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::ComplexGateway { default, .. } => {
                self.handle_complex_gateway(instance_id, token, &def_clone, &current_id, default)
                    .await
            }
            BpmnElement::EventBasedGateway => {
                self.handle_event_based_gateway(instance_id, token, &def_clone, &current_id)
                    .await
            }
            BpmnElement::TimerCatchEvent(timer_def) => {
                self.handle_timer_catch_event(instance_id, token, &current_id, timer_def)
                    .await
            }
            BpmnElement::MessageCatchEvent { message_name } => {
                self.handle_message_catch_event(instance_id, token, &current_id, message_name)
                    .await
            }
            BpmnElement::CallActivity { called_element } => {
                self.handle_call_activity(
                    instance_id,
                    token,
                    &def_clone,
                    &current_id,
                    called_element,
                )
                .await
            }
            BpmnElement::EmbeddedSubProcess { start_node_id } => {
                self.handle_embedded_sub_process(token, start_node_id).await
            }
            BpmnElement::SubProcessEndEvent { sub_process_id } => {
                self.handle_sub_process_end_event(token, &def_clone, sub_process_id)
                    .await
            }
            BpmnElement::BoundaryTimerEvent { .. }
            | BpmnElement::BoundaryMessageEvent { .. }
            | BpmnElement::BoundaryErrorEvent { .. } => {
                self.handle_boundary_event(instance_id, token, &def_clone, &current_id)
                    .await
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

        let mut condition_met = false;
        if let Some(BpmnElement::ComplexGateway {
            join_condition: Some(cond),
            ..
        }) = def.get_node(gateway_id)
        {
            let mut temp_vars = std::collections::HashMap::new();
            if let Some(barrier) = inst.join_barriers.get(gateway_id) {
                for t in &barrier.arrived_tokens {
                    temp_vars.extend(t.variables.clone());
                }
            }
            if evaluate_condition(cond, &temp_vars) {
                condition_met = true;
                inst.audit_log.push(format!(
                    "⟡ Complex gateway join condition met early at '{gateway_id}'"
                ));
            }
        }

        if current_arrived >= expected_count || condition_met {
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
        if !inst.tokens.is_empty() {
            return Ok(false);
        }
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
