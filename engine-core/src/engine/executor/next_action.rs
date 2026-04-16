use std::collections::VecDeque;

use uuid::Uuid;

use crate::domain::{EngineError, EngineResult, Token};
use crate::engine::WorkflowEngine;
use crate::runtime::*;

impl WorkflowEngine {
    /// Dispatches a single `NextAction` produced by `execute_step`.
    /// Returns `true` when the execution loop should stop (Terminate cleared the queue).
    pub(crate) async fn handle_next_action(
        &self,
        action: NextAction,
        instance_id: Uuid,
        token_id: Uuid,
        current_gateway_id: &str,
        queue: &mut VecDeque<Token>,
    ) -> EngineResult<()> {
        match action {
            NextAction::Continue(next_token) => {
                queue.push_back(next_token);
            }
            NextAction::ContinueMultiple(forked_tokens) => {
                let branch_count = forked_tokens.len();

                self.register_join_barrier_if_needed(
                    instance_id,
                    current_gateway_id,
                    branch_count,
                )
                .await?;

                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    if let Some(active) = inst
                        .active_tokens
                        .iter_mut()
                        .find(|at| at.token.id == token_id)
                    {
                        active.completed = true;
                    }

                    inst.state = InstanceState::ParallelExecution {
                        active_token_count: inst.active_tokens.len() + branch_count,
                    };
                    inst.current_node = current_gateway_id.to_string();
                }

                for (idx, fork_token) in forked_tokens.into_iter().enumerate() {
                    self.register_active_token(
                        instance_id,
                        current_gateway_id,
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
                self.emit_event(crate::engine::events::EngineEvent::TaskChanged);
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
                self.emit_event(crate::engine::events::EngineEvent::TaskChanged);
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
                let mut child_def_key = None;
                let all_defs = self.definitions.all();
                for (k, v) in &all_defs {
                    if v.id == called_element {
                        child_def_key = Some(*k);
                        break;
                    }
                }

                if let Some(child_key) = child_def_key {
                    let sub_instance_id = Uuid::new_v4();

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
                self.complete_branch_token(instance_id, token_id).await?;

                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    inst.tokens.remove(&token_id);
                }

                if self.all_tokens_completed(instance_id).await? {
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        inst.state = InstanceState::Completed;
                        inst.completed_at = Some(chrono::Utc::now());
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
                }
                self.persist_instance(instance_id).await;
            }
            NextAction::ErrorEnd { error_code } => {
                self.complete_branch_token(instance_id, token_id).await?;

                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    inst.tokens.remove(&token_id);
                }

                if self.all_tokens_completed(instance_id).await? {
                    // Schreiblock in eigenem Block — muss vor record_history_event freigegeben sein
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        inst.state = InstanceState::CompletedWithError {
                            error_code: error_code.clone(),
                        };
                        inst.completed_at = Some(chrono::Utc::now());
                        inst.audit_log.push(format!(
                            "💥 All tokens completed at Error End with code '{error_code}'"
                        ));
                    } // ← Write-Lock hier freigegeben
                    self.record_history_event(
                        instance_id,
                        crate::history::HistoryEventType::InstanceCompleted,
                        &format!("Process completed with error code '{error_code}'"),
                        crate::history::ActorType::Engine,
                        None,
                        None,
                    )
                    .await;
                }
                self.persist_instance(instance_id).await;
            }
            NextAction::EscalationEnd { escalation_code } => {
                self.complete_branch_token(instance_id, token_id).await?;

                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    inst.tokens.remove(&token_id);
                }

                if self.all_tokens_completed(instance_id).await? {
                    // Schreiblock in eigenem Block — muss vor record_history_event freigegeben sein
                    if let Some(inst_arc) = self.instances.get(&instance_id).await {
                        let mut inst = inst_arc.write().await;
                        inst.state = InstanceState::Completed;
                        inst.completed_at = Some(chrono::Utc::now());
                        inst.push_audit_log(format!(
                            "⚡ All tokens completed with escalation '{escalation_code}'"
                        ));
                    } // ← Write-Lock hier freigegeben
                    self.record_history_event(
                        instance_id,
                        crate::history::HistoryEventType::InstanceCompleted,
                        &format!("Process completed via escalation '{escalation_code}'"),
                        crate::history::ActorType::Engine,
                        None,
                        None,
                    )
                    .await;
                }
                self.persist_instance(instance_id).await;
            }
            NextAction::SpawnAndContinue { main, spawned } => {
                queue.push_back(main);
                for extra_token in spawned {
                    queue.push_back(extra_token);
                }
            }
            NextAction::Terminate => {
                self.pending_user_tasks
                    .retain(|_, t| t.instance_id != instance_id);
                self.pending_service_tasks
                    .retain(|_, t| t.instance_id != instance_id);
                self.pending_timers
                    .retain(|_, t| t.instance_id != instance_id);
                self.pending_message_catches
                    .retain(|_, t| t.instance_id != instance_id);

                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let mut inst = inst_arc.write().await;
                    for at in inst.active_tokens.iter_mut() {
                        at.completed = true;
                    }
                    inst.state = InstanceState::Completed;
                    inst.completed_at = Some(chrono::Utc::now());
                    inst.audit_log
                        .push("⛔ Process terminated. All tokens killed.".to_string());
                }

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
                        .find(|at| at.token.id == token_id)
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
        Ok(())
    }
}
