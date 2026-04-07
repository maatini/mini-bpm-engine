use crate::engine::WorkflowEngine;
use crate::engine::boundary::setup_boundary_events;
use crate::engine::executor::resolve_next_target;
use crate::engine::types::NextAction;
use crate::error::{EngineError, EngineResult};
use crate::model::{ProcessDefinition, Token};
use std::sync::Arc;
use uuid::Uuid;

impl WorkflowEngine {
    pub(crate) async fn handle_call_activity(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        called_element: &str,
    ) -> EngineResult<NextAction> {
        let (pending_timers, pending_msgs) =
            setup_boundary_events(def_clone, current_id, instance_id, token);
        for t in pending_timers {
            self.pending_timers.insert(t.id, t);
        }
        for m in pending_msgs {
            self.pending_message_catches.insert(m.id, m);
        }

        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = current_id.to_string();
        inst.push_audit_log(format!(
            "🔗 Call Activity '{current_id}' invoking '{called_element}'"
        ));
        tracing::info!("Instance {instance_id}: '{current_id}' invoking '{called_element}'");
        Ok(NextAction::WaitForCallActivity {
            called_element: called_element.to_string(),
            token: token.clone(),
        })
    }

    pub(crate) async fn handle_embedded_sub_process(
        &self,
        token: &mut Token,
        start_node_id: &str,
    ) -> EngineResult<NextAction> {
        token.current_node = start_node_id.to_string();
        Ok(NextAction::Continue(token.clone()))
    }

    pub(crate) async fn handle_sub_process_end_event(
        &self,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        sub_process_id: &str,
    ) -> EngineResult<NextAction> {
        let next = resolve_next_target(def_clone, sub_process_id, &token.variables)?;
        token.current_node = next.clone();
        Ok(NextAction::Continue(token.clone()))
    }
}
