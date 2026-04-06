use super::WorkflowEngine;
use crate::InstanceState;
use crate::error::{EngineError, EngineResult};

impl WorkflowEngine {
    pub async fn process_timers(&self) -> EngineResult<usize> {
        let now = chrono::Utc::now();
        let mut expired = Vec::new();

        for timer in self.pending_timers.iter() {
            if timer.expires_at <= now {
                expired.push(timer.id);
            }
        }

        let count = expired.len();
        for tid in expired {
            let timer = self
                .pending_timers
                .remove(&tid)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    EngineError::InvalidDefinition(format!("Timer {tid} disappeared"))
                })?;

            // Event-Based Gateway support: If this timer triggered, clear any sibling wait states
            self.clear_wait_states_for_token(timer.instance_id, &timer.token_id)
                .await;

            let old_state = if let Some(lk) = self.instances.get(&timer.instance_id).await {
                Some(lk.read().await.clone())
            } else {
                None
            };
            let def_key = {
                let inst_arc = self
                    .instances
                    .get(&timer.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.state = InstanceState::Running;
                inst.audit_log
                    .push(format!("⏱ Timer '{}' expired, resuming", timer.node_id));
                inst.definition_key
            };

            self.record_history_event(
                timer.instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                "Timer expired",
                crate::history::ActorType::Timer,
                None,
                old_state.as_ref(),
            )
            .await;

            if timer.token_id.is_nil() {
                // This is a Scope Event Listener (Event Sub-Process) trigger
                let child_bpmn_id = timer.node_id.clone();
                let child_def_key = {
                    let (k, _) = self
                        .definitions
                        .find_latest_by_bpmn_id(&child_bpmn_id)
                        .await
                        .ok_or_else(|| {
                            EngineError::InvalidDefinition(format!(
                                "Event Subprocess '{child_bpmn_id}' not found"
                            ))
                        })?;
                    k
                };

                // Get the instance variables to pass down
                let instance_vars = {
                    let inst_arc = self
                        .instances
                        .get(&timer.instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
                    let inst = inst_arc.read().await;
                    inst.variables.clone()
                };

                // Spawn the call activity loosely (it will track parent_instance_id automatically)
                let _child_id = self
                    .spawn_call_activity(
                        child_def_key,
                        timer.instance_id,
                        child_bpmn_id.clone(),
                        instance_vars,
                    )
                    .await?;

                self.remove_persisted_timer(tid).await;
                continue;
            }

            // Retrieve token from central store
            let mut token = {
                let inst_arc = self
                    .instances
                    .get(&timer.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.tokens.remove(&timer.token_id).ok_or_else(|| {
                    EngineError::InvalidDefinition(format!(
                        "Token {} not found in instance",
                        timer.token_id
                    ))
                })?
            };
            let def = self
                .definitions
                .get(&def_key)
                .await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(
                &def,
                &timer.node_id,
                &token.variables,
            )?;
            token.current_node = next.clone();

            {
                let inst_arc = self
                    .instances
                    .get(&timer.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(timer.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }

            self.remove_persisted_timer(tid).await;
            self.run_instance_batch(timer.instance_id, token).await?;

            // Re-schedule recurring timers
            if let Some(ref def) = timer.timer_def {
                if def.is_recurring() {
                    let should_repeat = match timer.remaining_repetitions {
                        Some(0) => false,
                        _ => true,
                    };
                    if should_repeat {
                        let now = chrono::Utc::now();
                        if let Some(next_expiry) = def.next_expiry(now) {
                            let new_remaining = timer.remaining_repetitions.map(|r| r.saturating_sub(1));
                            let new_pending = crate::engine::types::PendingTimer {
                                id: uuid::Uuid::new_v4(),
                                instance_id: timer.instance_id,
                                node_id: timer.node_id.clone(),
                                expires_at: next_expiry,
                                token_id: timer.token_id,
                                timer_def: Some(def.clone()),
                                remaining_repetitions: new_remaining,
                            };
                            self.pending_timers.insert(new_pending.id, new_pending);
                        }
                    }
                }
            }
        }

        Ok(count)
    }
}
