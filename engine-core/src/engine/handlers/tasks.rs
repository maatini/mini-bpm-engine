use crate::engine::WorkflowEngine;
use crate::engine::boundary::setup_boundary_events;
use crate::engine::executor::resolve_next_target;
use crate::engine::types::{NextAction, PendingServiceTask, PendingUserTask};
use crate::error::{EngineError, EngineResult};
use crate::model::{ProcessDefinition, Token};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

impl WorkflowEngine {
    pub(crate) async fn handle_user_task(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        assignee: &String,
    ) -> EngineResult<NextAction> {
        let (pending_timers, pending_msgs) =
            setup_boundary_events(def_clone, current_id, instance_id, token);
        for t in pending_timers {
            self.pending_timers.insert(t.id, t);
        }
        for m in pending_msgs {
            self.pending_message_catches.insert(m.id, m);
        }

        let pending = PendingUserTask {
            task_id: Uuid::new_v4(),
            instance_id,
            node_id: current_id.to_string(),
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
        inst.current_node = current_id.to_string();
        inst.tokens.insert(token.id, token.clone());
        inst.push_audit_log(format!(
            "👤 User task '{current_id}' assigned to '{:?}' — waiting (task_id: {})",
            assignee, pending.task_id
        ));
        tracing::info!(
            "Instance {instance_id}: user task '{current_id}' pending for '{:?}'",
            assignee
        );

        Ok(NextAction::WaitForUser(pending))
    }

    pub(crate) async fn handle_script_task(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        script: &str,
    ) -> EngineResult<NextAction> {
        let script_engine = crate::engine::create_script_engine();
        let mut scope = rhai::Scope::new();
        for (k, v) in &token.variables {
            scope.push_dynamic(k, rhai::serde::to_dynamic(v).unwrap_or(rhai::Dynamic::UNIT));
        }

        script_engine
            .eval_with_scope::<()>(&mut scope, script)
            .map_err(|e| EngineError::ScriptError(e.to_string()))?;

        for (k, _, v) in scope.iter_raw() {
            if let Ok(json_val) = rhai::serde::from_dynamic(v) {
                token.variables.insert(k.to_string(), json_val);
            }
        }

        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;
        let next = resolve_next_target(def_clone, current_id, &token.variables)?;
        token.current_node = next.clone();

        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = next;
        inst.variables = token.variables.clone();
        inst.push_audit_log(format!("📜 Script task '{current_id}' executed"));
        Ok(NextAction::Continue(token.clone()))
    }

    pub(crate) async fn handle_send_task(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        message_name: &str,
    ) -> EngineResult<NextAction> {
        tracing::info!(
            "Instance {instance_id}: send task '{current_id}' publishing message '{message_name}'"
        );
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;

        let next = resolve_next_target(def_clone, current_id, &token.variables)?;
        token.current_node = next.clone();

        let inst_arc = self
            .instances
            .get(&instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        let mut inst = inst_arc.write().await;
        inst.current_node = next;
        inst.push_audit_log(format!(
            "📤 Send task '{current_id}' published message '{message_name}'"
        ));
        Ok(NextAction::Continue(token.clone()))
    }

    pub(crate) async fn handle_service_task(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        topic: &str,
    ) -> EngineResult<NextAction> {
        let (pending_timers, pending_msgs) =
            setup_boundary_events(def_clone, current_id, instance_id, token);
        for t in pending_timers {
            self.pending_timers.insert(t.id, t);
        }
        for m in pending_msgs {
            self.pending_message_catches.insert(m.id, m);
        }

        let svc_task = PendingServiceTask {
            id: Uuid::new_v4(),
            instance_id,
            definition_key: def_clone.key,
            node_id: current_id.to_string(),
            topic: topic.to_string(),
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
        inst.current_node = current_id.to_string();
        inst.tokens.insert(token.id, token.clone());
        inst.push_audit_log(format!(
            "🔗 Service task '{current_id}' created for topic '{topic}' (task_id: {})",
            svc_task.id
        ));
        tracing::info!(
            "Instance {instance_id}: service task '{current_id}' pending for topic '{topic}'"
        );
        Ok(NextAction::WaitForServiceTask(svc_task))
    }
}
