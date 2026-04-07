use crate::engine::WorkflowEngine;
use crate::engine::types::{NextAction, PendingMessageCatch, PendingTimer};
use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, ProcessDefinition, Token};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::gateway::{
    execute_complex_gateway, execute_exclusive_gateway, execute_inclusive_gateway,
    execute_parallel_gateway,
};

impl WorkflowEngine {
    pub(crate) async fn handle_parallel_gateway(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let action = execute_parallel_gateway(def_clone, current_id, token)?;
        if let NextAction::ContinueMultiple(ref f) = action {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.current_node = current_id.to_string();
            inst.push_audit_log(format!(
                "■ Parallel gateway '{current_id}' → forked to {} path(s)",
                f.len()
            ));
        }
        Ok(action)
    }

    pub(crate) async fn handle_exclusive_gateway(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        default: &Option<String>,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let action = execute_exclusive_gateway(def_clone, current_id, token, default)?;
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.push_audit_log(format!(
            "◆ Exclusive gateway '{current_id}' → took path to '{}'",
            token.current_node
        ));
        inst.current_node = token.current_node.clone();
        Ok(action)
    }

    pub(crate) async fn handle_inclusive_gateway(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let action = execute_inclusive_gateway(def_clone, current_id, token)?;
        if let NextAction::ContinueMultiple(ref f) = action {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.current_node = current_id.to_string();
            inst.push_audit_log(format!(
                "◇ Inclusive gateway '{current_id}' → forked to {} path(s)",
                f.len()
            ));
        }
        Ok(action)
    }

    pub(crate) async fn handle_complex_gateway(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        default: &Option<String>,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let action = execute_complex_gateway(def_clone, current_id, token, default)?;
        if let NextAction::ContinueMultiple(ref f) = action {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.current_node = current_id.to_string();
            inst.push_audit_log(format!(
                "⟡ Complex gateway '{current_id}' → forked to {} path(s)",
                f.len()
            ));
        } else if let NextAction::Continue(ref next_token) = action {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.push_audit_log(format!(
                "⟡ Complex gateway '{current_id}' → took path to '{}'",
                next_token.current_node
            ));
            inst.current_node = next_token.current_node.clone();
        }
        Ok(action)
    }

    pub(crate) async fn handle_event_based_gateway(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let mut actions = Vec::new();
        for sf in def_clone.next_nodes(current_id) {
            let target_node = sf.target.clone();
            if let Some(target_element) = def_clone.get_node(&target_node) {
                match target_element {
                    BpmnElement::TimerCatchEvent(timer_def) => {
                        let now = Utc::now();
                        let expires_at = timer_def.next_expiry(now).unwrap_or(now);
                        let pending = PendingTimer {
                            id: Uuid::new_v4(),
                            instance_id,
                            node_id: target_node.clone(),
                            expires_at,
                            token_id: token.id,
                            timer_def: Some(timer_def.clone()),
                            remaining_repetitions: None,
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
        inst.current_node = current_id.to_string();
        inst.tokens.insert(token.id, token.clone());
        inst.push_audit_log(format!(
            "⭮ Event-based gateway '{current_id}' waiting for {} alternative events",
            actions.len()
        ));
        Ok(NextAction::WaitForEventGroup(actions))
    }
}
