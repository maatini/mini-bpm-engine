use super::WorkflowEngine;
use crate::InstanceState;
use crate::error::{EngineError, EngineResult};
use crate::model::BpmnElement;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

impl WorkflowEngine {
    pub async fn correlate_message(
        &self,
        message_name: String,
        business_key: Option<String>,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Vec<Uuid>> {
        let mut affected_instances = Vec::new();
        let mut to_resume = Vec::new();

        for catch in self.pending_message_catches.iter() {
            if catch.message_name == message_name
                && let Some(inst_arc) = self.instances.get(&catch.instance_id).await
            {
                let inst = inst_arc.read().await;
                if let Some(ref bk) = business_key
                    && &inst.business_key != bk
                {
                    continue;
                }
                to_resume.push(catch.id);
                affected_instances.push(catch.instance_id);
            }
        }

        for catch_id in to_resume {
            let catch = self
                .pending_message_catches
                .remove(&catch_id)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    EngineError::InvalidDefinition(format!("Message catch {catch_id} disappeared"))
                })?;

            // Event-Based Gateway support: If this message catch triggered, clear any sibling wait states
            self.clear_wait_states_for_token(catch.instance_id, &catch.token_id)
                .await;

            if catch.token_id.is_nil() {
                // This is a Scope Event Listener (Event Sub-Process) trigger
                let child_bpmn_id = catch.node_id.clone();
                let child_def_key = {
                    let (k, _) = self
                        .definitions
                        .find_latest_by_bpmn_id(&child_bpmn_id)
                        .ok_or_else(|| {
                            EngineError::InvalidDefinition(format!(
                                "Event Subprocess '{child_bpmn_id}' not found"
                            ))
                        })?;
                    k
                };

                // Get the instance variables to pass down, merged with correlation variables
                let mut instance_vars = {
                    let inst_arc = self
                        .instances
                        .get(&catch.instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                    let inst = inst_arc.read().await;
                    inst.variables.clone()
                };
                instance_vars.extend(variables.clone());

                // Spawn the call activity loosely
                let _child_id = self
                    .spawn_call_activity(
                        child_def_key,
                        catch.instance_id,
                        child_bpmn_id.clone(),
                        instance_vars,
                    )
                    .await?;

                self.remove_persisted_message_catch(catch_id).await;
                continue;
            }

            let def_key = {
                let inst_arc = self
                    .instances
                    .get(&catch.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                inst_arc.read().await.definition_key
            };

            let def = self
                .definitions
                .get(&def_key)
                .ok_or(EngineError::NoSuchDefinition(def_key))?;

            let mut is_non_interrupting = false;
            if let Some(crate::model::BpmnElement::BoundaryMessageEvent {
                cancel_activity: false,
                ..
            }) = def.nodes.get(&catch.node_id)
            {
                is_non_interrupting = true;
            }

            // Retrieve token from central store
            let mut token = {
                let inst_arc = self
                    .instances
                    .get(&catch.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;

                if is_non_interrupting {
                    let mut original = inst
                        .tokens
                        .get(&catch.token_id)
                        .ok_or_else(|| {
                            EngineError::InvalidDefinition(format!(
                                "Token {} not found in instance",
                                catch.token_id
                            ))
                        })?
                        .clone();

                    original.id = uuid::Uuid::new_v4();
                    inst.tokens.insert(original.id, original.clone());

                    let active = crate::engine::types::ActiveToken {
                        token: original.clone(),
                        completed: false,
                        fork_id: Some(original.current_node.clone()),
                        branch_index: inst.active_tokens.len(),
                    };
                    inst.active_tokens.push(active);

                    if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                        inst.state = InstanceState::ParallelExecution {
                            active_token_count: inst.tokens.len(),
                        };
                    }

                    original
                } else {
                    inst.tokens.remove(&catch.token_id).ok_or_else(|| {
                        EngineError::InvalidDefinition(format!(
                            "Token {} not found in instance",
                            catch.token_id
                        ))
                    })?
                }
            };

            if !is_non_interrupting {
                let node_to_cancel = &token.current_node;

                let mut to_remove_ut = Vec::new();
                for r in self.pending_user_tasks.iter() {
                    if r.value().instance_id == catch.instance_id
                        && r.value().node_id == *node_to_cancel
                        && r.value().token_id == catch.token_id
                    {
                        to_remove_ut.push(*r.key());
                    }
                }
                for id in to_remove_ut {
                    self.pending_user_tasks.remove(&id);
                    self.remove_persisted_user_task(id).await;
                }

                let mut to_remove_st = Vec::new();
                for r in self.pending_service_tasks.iter() {
                    if r.value().instance_id == catch.instance_id
                        && r.value().node_id == *node_to_cancel
                        && r.value().token_id == catch.token_id
                    {
                        to_remove_st.push(*r.key());
                    }
                }
                for id in to_remove_st {
                    self.pending_service_tasks.remove(&id);
                    self.remove_persisted_service_task(id).await;
                }
            }

            token.variables.extend(variables.clone());

            let old_state = if let Some(lk) = self.instances.get(&catch.instance_id).await {
                Some(lk.read().await.clone())
            } else {
                None
            };

            let _def_key_audit = {
                let inst_arc = self
                    .instances
                    .get(&catch.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;
                if !matches!(inst.state, InstanceState::ParallelExecution { .. }) {
                    inst.state = InstanceState::Running;
                }
                inst.push_audit_log(format!(
                    "✉️ Msg '{}' correlated, resuming '{catch_id}'",
                    message_name
                ));
                inst.definition_key
            };

            self.record_history_event(
                catch.instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                &format!("Message '{}' correlated", message_name),
                crate::history::ActorType::Engine,
                None,
                old_state.as_ref(),
            )
            .await;

            let def = self
                .definitions
                .get(&def_key)
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(
                &def,
                &catch.node_id,
                &token.variables,
            )?;
            token.current_node = next.clone();

            {
                let inst_arc = self
                    .instances
                    .get(&catch.instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }

            self.remove_persisted_message_catch(catch_id).await;
            self.run_instance_batch(catch.instance_id, token).await?;
        }

        let mut defs_to_start = Vec::new();
        let all_defs = self.definitions.all();
        for (def_key, def) in &all_defs {
            if let Some((
                _,
                BpmnElement::MessageStartEvent {
                    message_name: ref_msg,
                },
            )) = def.start_event()
                && ref_msg == &message_name
            {
                defs_to_start.push(*def_key);
            }
        }

        for def_key in defs_to_start {
            let new_id = self
                .start_instance_with_variables(def_key, variables.clone())
                .await?;
            if let Some(ref bk) = business_key {
                if let Some(inst_arc) = self.instances.get(&new_id).await {
                    let mut inst = inst_arc.write().await;
                    inst.business_key = bk.clone();
                }
                self.persist_instance(new_id).await;
            }
            affected_instances.push(new_id);
        }

        Ok(affected_instances)
    }
}
