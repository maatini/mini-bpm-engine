use crate::engine::WorkflowEngine;
use crate::engine::executor::resolve_next_target;
use crate::runtime::{NextAction, PendingMessageCatch, PendingTimer};
use crate::domain::{EngineError, EngineResult};
use crate::domain::{ProcessDefinition, Token};
use crate::runtime::CompensationRecord;
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
        timer_def: &crate::domain::TimerDefinition,
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

    /// Handles an escalation end event — ends the current path with an escalation code.
    pub(crate) async fn handle_escalation_end_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        escalation_code: &str,
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
            "⚡ Escalation end event '{current_id}' with code '{escalation_code}'"
        ));
        Ok(NextAction::EscalationEnd {
            escalation_code: escalation_code.to_string(),
        })
    }

    /// Handles an intermediate escalation throw event.
    ///
    /// Searches for a matching BoundaryEscalationEvent in the definition.
    /// - Non-interrupting: spawns an extra token at the handler, main token continues.
    /// - Interrupting: redirects the main token to the boundary handler outflow.
    /// - Not caught: token continues normally (escalations are non-fatal).
    pub(crate) async fn handle_escalation_throw_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        escalation_code: &str,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::EscalationThrown,
            &format!("Escalation '{}' thrown at '{}'", escalation_code, current_id),
            crate::history::ActorType::Engine,
            None,
            None,
        )
        .await;

        // Search for matching boundary escalation event (any attached_to)
        if let Some((boundary_id, _attached_to, cancel_activity)) =
            crate::engine::executor::find_any_boundary_escalation_event(
                def_clone,
                escalation_code,
            )
        {
            let handler_target =
                resolve_next_target(def_clone, &boundary_id, &token.variables)?;

            if cancel_activity {
                // Interrupting: redirect token to boundary handler
                {
                    let inst_arc = self
                        .instances
                        .get(&instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(instance_id))?;
                    let mut inst = inst_arc.write().await;
                    inst.push_audit_log(format!(
                        "⚡ Escalation '{}' caught (interrupting) by '{}'",
                        escalation_code, boundary_id
                    ));
                    inst.current_node = handler_target.clone();
                }
                token.current_node = handler_target;
                Ok(NextAction::Continue(token.clone()))
            } else {
                // Non-interrupting: spawn extra token, main token continues
                let mut handler_token = Token::with_variables(&handler_target, token.variables.clone());
                handler_token.current_node = handler_target;

                let next = resolve_next_target(def_clone, current_id, &token.variables)?;
                token.current_node = next.clone();
                {
                    let inst_arc = self
                        .instances
                        .get(&instance_id)
                        .await
                        .ok_or(EngineError::NoSuchInstance(instance_id))?;
                    let mut inst = inst_arc.write().await;
                    inst.push_audit_log(format!(
                        "⚡ Escalation '{}' caught (non-interrupting) by '{}' — spawned handler token",
                        escalation_code, boundary_id
                    ));
                    inst.current_node = next;
                }
                Ok(NextAction::SpawnAndContinue {
                    main: token.clone(),
                    spawned: vec![handler_token],
                })
            }
        } else {
            // No handler found — escalation is ignored (non-fatal), token continues
            let next = resolve_next_target(def_clone, current_id, &token.variables)?;
            token.current_node = next.clone();
            {
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.push_audit_log(format!(
                    "⚡ Escalation '{}' thrown at '{}' — no handler found, continuing",
                    escalation_code, current_id
                ));
                inst.current_node = next;
            }
            Ok(NextAction::Continue(token.clone()))
        }
    }

    /// Handles a compensation throw event (intermediate or end).
    ///
    /// Executes registered compensation handlers in reverse order (LIFO).
    /// If `activity_ref` is set, only compensates that specific activity.
    pub(crate) async fn handle_compensation_throw_event(
        &self,
        instance_id: Uuid,
        token: &mut Token,
        def_clone: &Arc<ProcessDefinition>,
        current_id: &str,
        activity_ref: &Option<String>,
        is_end_event: bool,
    ) -> EngineResult<NextAction> {
        self.run_end_scripts(instance_id, token, def_clone, current_id)
            .await?;

        // Collect compensation handlers to execute
        let handlers: Vec<CompensationRecord> = {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let inst = inst_arc.read().await;

            if let Some(target_activity) = activity_ref {
                // Compensate specific activity
                inst.compensation_log
                    .iter()
                    .filter(|r| &r.activity_id == target_activity)
                    .cloned()
                    .collect()
            } else {
                // Compensate all in reverse order (LIFO)
                inst.compensation_log.iter().rev().cloned().collect()
            }
        };

        let handler_count = handlers.len();
        {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            inst.push_audit_log(format!(
                "♻ Compensation triggered at '{}' — {} handler(s) to execute",
                current_id, handler_count
            ));
            inst.current_node = current_id.to_string();
        }

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::CompensationTriggered,
            &format!(
                "Compensation triggered at '{}' ({} handlers)",
                current_id, handler_count
            ),
            crate::history::ActorType::Engine,
            None,
            None,
        )
        .await;

        // Execute each compensation handler synchronously (LIFO)
        for record in &handlers {
            let handler_node = &record.handler_node_id;
            if def_clone.get_node(handler_node).is_some() {
                let handler_token =
                    Token::with_variables(handler_node, token.variables.clone());
                tracing::info!(
                    "Instance {}: executing compensation handler '{}' for activity '{}'",
                    instance_id,
                    handler_node,
                    record.activity_id
                );
                // Run the handler through the batch — it will execute the compensation activity
                Box::pin(self.run_instance_batch(instance_id, handler_token)).await?;
                // Read back updated variables from the instance (handler may have modified them)
                if let Some(inst_arc) = self.instances.get(&instance_id).await {
                    let inst = inst_arc.read().await;
                    token.variables = inst.variables.clone();
                }
            } else {
                tracing::warn!(
                    "Compensation handler node '{}' not found in definition",
                    handler_node
                );
            }
        }

        // Clear executed compensation records
        {
            let inst_arc = self
                .instances
                .get(&instance_id)
                .await
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            let mut inst = inst_arc.write().await;
            if activity_ref.is_some() {
                inst.compensation_log
                    .retain(|r| Some(&r.activity_id) != activity_ref.as_ref());
            } else {
                inst.compensation_log.clear();
            }
            inst.push_audit_log(format!(
                "♻ Compensation completed — {} handler(s) executed",
                handler_count
            ));
        }

        if is_end_event {
            Ok(NextAction::Complete)
        } else {
            let next = resolve_next_target(def_clone, current_id, &token.variables)?;
            token.current_node = next.clone();
            {
                let inst_arc = self
                    .instances
                    .get(&instance_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                let mut inst = inst_arc.write().await;
                inst.current_node = next;
            }
            Ok(NextAction::Continue(token.clone()))
        }
    }

    /// Registers a compensation handler for a completed activity.
    /// Called when an activity with a BoundaryCompensationEvent completes successfully.
    pub(crate) async fn register_compensation_handler(
        &self,
        instance_id: Uuid,
        activity_id: &str,
        def: &ProcessDefinition,
    ) {
        if let Some(handler_node) =
            crate::engine::executor::find_compensation_handler(def, activity_id)
        {
            if let Some(inst_arc) = self.instances.get(&instance_id).await {
                let mut inst = inst_arc.write().await;
                inst.compensation_log.push(CompensationRecord {
                    activity_id: activity_id.to_string(),
                    handler_node_id: handler_node.clone(),
                });
                inst.push_audit_log(format!(
                    "♻ Registered compensation handler '{}' for activity '{}'",
                    handler_node, activity_id
                ));
            }
            tracing::debug!(
                "Instance {}: registered compensation handler '{}' for '{}'",
                instance_id,
                handler_node,
                activity_id
            );
        }
    }
}
