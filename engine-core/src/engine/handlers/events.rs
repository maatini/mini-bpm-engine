use crate::engine::WorkflowEngine;
use crate::engine::executor::resolve_next_target;
use crate::engine::types::{NextAction, PendingMessageCatch, PendingTimer};
use crate::error::{EngineError, EngineResult};
use crate::model::{ProcessDefinition, Token};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

impl WorkflowEngine {
    pub(crate) async fn handle_start_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        tracing::debug!("Passing through start event '{current_id}'");
        let next = resolve_next_target(def_clone, current_id, &token.variables)?;
        self.run_end_scripts(instance_id, token, def_clone, current_id)
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

    pub(crate) async fn handle_end_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.push_audit_log(format!("⏹ Process completed at end event '{current_id}'"));
        tracing::info!("Instance {instance_id}: reached end event '{current_id}'");
        Ok(NextAction::Complete)
    }

    pub(crate) async fn handle_terminate_end_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.push_audit_log(format!(
            "⛔ Terminate end event '{current_id}' — killing all active tokens"
        ));
        tracing::info!("Instance {instance_id}: terminate end event '{current_id}'");
        Ok(NextAction::Terminate)
    }

    pub(crate) async fn handle_error_end_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        error_code: &String,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.push_audit_log(format!(
            "💥 Process completed at error end '{current_id}' with error '{:?}'",
            error_code
        ));
        Ok(NextAction::ErrorEnd {
            error_code: error_code.clone(),
        })
    }

    pub(crate) async fn handle_timer_catch_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        current_id: &str,
        timer_def: &crate::timer_definition::TimerDefinition,
    ) -> EngineResult<NextAction> {
        let now = Utc::now();
        let expires_at = timer_def.next_expiry(now).unwrap_or(now);
        let pending = PendingTimer {
            id: Uuid::new_v4(),
            instance_id,
            node_id: current_id.to_string(),
            expires_at,
            token_id: token.id,
            timer_def: Some(timer_def.clone()),
            remaining_repetitions: None,
        };
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.tokens.insert(token.id, token.clone());
        inst.push_audit_log(format!("⏱ Timer catch event '{current_id}' — waiting"));
        Ok(NextAction::WaitForTimer(pending))
    }

    pub(crate) async fn handle_message_catch_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        current_id: &str,
        message_name: &str,
    ) -> EngineResult<NextAction> {
        let pending = PendingMessageCatch {
            id: Uuid::new_v4(),
            instance_id,
            node_id: current_id.to_string(),
            message_name: message_name.to_string(),
            token_id: token.id,
        };
        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.tokens.insert(token.id, token.clone());
        inst.push_audit_log(format!(
            "✉️ Message catch event '{current_id}' waiting for '{message_name}'"
        ));
        Ok(NextAction::WaitForMessage(pending))
    }

    pub(crate) async fn handle_boundary_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
    ) -> EngineResult<NextAction> {
        let next = resolve_next_target(def_clone, current_id, &token.variables)?;
        self.run_end_scripts(instance_id, token, def_clone, current_id)
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
