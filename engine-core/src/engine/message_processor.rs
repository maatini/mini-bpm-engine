use std::collections::HashMap;
use serde_json::Value;
use uuid::Uuid;
use super::WorkflowEngine;
use crate::error::{EngineError, EngineResult};
use crate::InstanceState;
use crate::model::BpmnElement;

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
            if catch.message_name == message_name {
                if let Some(inst_arc) = self.instances.get(&catch.instance_id).await {
                    let inst = inst_arc.read().await;
                    if let Some(ref bk) = business_key {
                        if &inst.business_key != bk {
                            continue;
                        }
                    }
                    to_resume.push(catch.id);
                    affected_instances.push(catch.instance_id);
                }
            }
        }
        
        for catch_id in to_resume {
            let catch = self.pending_message_catches.remove(&catch_id).map(|(_, v)| v)
                .ok_or_else(|| EngineError::InvalidDefinition(format!("Message catch {catch_id} disappeared")))?;
                
            // Event-Based Gateway support: If this message catch triggered, clear any sibling wait states
            self.clear_wait_states_for_token(catch.instance_id, &catch.token_id).await;
            
            if catch.token_id.is_nil() {
                // This is a Scope Event Listener (Event Sub-Process) trigger
                let child_bpmn_id = catch.node_id.clone();
                let child_def_key = {
                    let (k, _) = self.definitions.find_latest_by_bpmn_id(&child_bpmn_id).await
                        .ok_or_else(|| EngineError::InvalidDefinition(format!("Event Subprocess '{child_bpmn_id}' not found")))?;
                    k
                };
                
                // Get the instance variables to pass down, merged with correlation variables
                let mut instance_vars = {
                    let inst_arc = self.instances.get(&catch.instance_id).await
                        .ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                    let inst = inst_arc.read().await;
                    inst.variables.clone()
                };
                instance_vars.extend(variables.clone());
                
                // Spawn the call activity loosely
                let _child_id = self.spawn_call_activity(child_def_key, catch.instance_id, child_bpmn_id.clone(), instance_vars).await?;
                
                self.remove_persisted_message_catch(catch_id).await;
                continue;
            }

            // Retrieve token from central store
            let mut token = {
                let inst_arc = self.instances.get(&catch.instance_id).await.ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.tokens.remove(&catch.token_id)
                    .ok_or_else(|| EngineError::InvalidDefinition(format!("Token {} not found in instance", catch.token_id)))?
            };
            token.variables.extend(variables.clone());
            
            let old_state = if let Some(lk) = self.instances.get(&catch.instance_id).await { Some(lk.read().await.clone()) } else { None };
            let def_key = {
                let inst_arc = self.instances.get(&catch.instance_id).await.ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.state = InstanceState::Running;
                inst.audit_log.push(format!("✉️ Msg '{}' correlated, resuming '{catch_id}'", message_name));
                inst.definition_key
            };
            
            self.record_history_event(
                catch.instance_id,
                crate::history::HistoryEventType::TokenAdvanced,
                &format!("Message '{}' correlated", message_name),
                crate::history::ActorType::Engine,
                None,
                old_state.as_ref()
            ).await;
            
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
            let next = crate::engine::executor::resolve_next_target(&def, &catch.node_id, &token.variables)?;
            token.current_node = next.clone();
            
            {
                let inst_arc = self.instances.get(&catch.instance_id).await.ok_or(EngineError::NoSuchInstance(catch.instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }
            
            self.remove_persisted_message_catch(catch_id).await;
            self.run_instance_batch(catch.instance_id, token).await?;
        }
        
        let mut defs_to_start = Vec::new();
        let all_defs = self.definitions.all().await;
        for (def_key, def) in &all_defs {
            if let Some((_, BpmnElement::MessageStartEvent { message_name: ref_msg })) = def.start_event() {
                if ref_msg == &message_name {
                    defs_to_start.push(*def_key);
                }
            }
        }
        
        for def_key in defs_to_start {
            let new_id = self.start_instance_with_variables(def_key, variables.clone()).await?;
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
