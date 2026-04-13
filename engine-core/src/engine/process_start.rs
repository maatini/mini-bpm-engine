use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::domain::{BpmnElement, ScopeEventListener, Token};
use crate::domain::{EngineError, EngineResult};
use crate::runtime::{PendingMessageCatch, PendingTimer};
use chrono::Utc;

use super::WorkflowEngine;
use crate::runtime::{InstanceState, ProcessInstance};

impl WorkflowEngine {
    /// Starts a new process instance from a deployed definition.
    ///
    /// The definition must have a plain `StartEvent`.
    /// Delegates to `start_instance_with_variables` with an empty variable map.
    pub async fn start_instance(&self, definition_key: Uuid) -> EngineResult<Uuid> {
        self.start_instance_with_variables(definition_key, HashMap::new())
            .await
    }

    /// Starts a new process instance with pre-populated variables.
    ///
    /// Like `start_instance`, but the token carries initial variables from the
    /// caller. The instance's `variables` field is also seeded.
    pub async fn start_instance_with_variables(
        &self,
        definition_key: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        self.start_instance_with_variables_and_parent(definition_key, variables, None)
            .await
    }

    /// Starts a new instance from the latest version of a BPMN process ID.
    ///
    /// Looks up the definition with the highest version matching `bpmn_id`
    /// and starts an instance from it. Returns the (instance_id, definition_key) tuple.
    pub async fn start_instance_latest(
        &self,
        bpmn_id: &str,
        variables: HashMap<String, Value>,
    ) -> EngineResult<(Uuid, Uuid)> {
        let (def_key, _) = self
            .definitions
            .find_latest_by_bpmn_id(bpmn_id)
            .ok_or_else(|| {
                EngineError::InvalidDefinition(format!(
                    "No definition found with BPMN ID '{}'",
                    bpmn_id
                ))
            })?;
        let inst_id = self
            .start_instance_with_variables(def_key, variables)
            .await?;
        Ok((inst_id, def_key))
    }

    /// Internal method to start an instance and link it to a parent call activity
    pub(crate) async fn start_instance_with_variables_and_parent(
        &self,
        definition_key: Uuid,
        mut variables: HashMap<String, Value>,
        parent_instance_id: Option<Uuid>,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        if matches!(start_element, BpmnElement::TimerStartEvent(_)) {
            return Err(EngineError::InvalidDefinition(
                "Use trigger_timer_start() for timer start events".into(),
            ));
        }
        let instance_id = Uuid::new_v4();
        let business_key = variables
            .remove("business_key")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let instance = ProcessInstance {
            id: instance_id,
            definition_key,
            business_key,
            parent_instance_id,
            state: InstanceState::Running,
            current_node: start_id.to_string(),
            audit_log: vec![format!(
                "▶ Process started at node '{start_id}' with {} variable(s)",
                variables.len()
            )],
            variables: variables.clone(),
            tokens: HashMap::new(),
            active_tokens: Vec::new(),
            join_barriers: std::collections::HashMap::new(),
            multi_instance_state: std::collections::HashMap::new(),
            compensation_log: Vec::new(),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
        };

        tracing::info!(
            "Started instance {instance_id} of def key {definition_key} at node '{start_id}' with {} vars",
            variables.len()
        );

        metrics::counter!("bpmn_instance_started_total").increment(1);
        metrics::gauge!("bpmn_active_instances").increment(1.0);

        self.instances.insert(instance_id, instance).await;

        // Record history for start
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Started instance of process '{}'", def.id),
            crate::history::ActorType::Engine,
            None,
            None,
        )
        .await;

        let token = Token::with_variables(start_id, variables);

        // Register Scope Event Listeners (Event Sub-Processes)
        for listener in &def.event_listeners {
            match listener {
                ScopeEventListener::Timer {
                    timer,
                    is_interrupting: _,
                    target_definition,
                } => {
                    let now = Utc::now();
                    let expires_at = timer.next_expiry(now).unwrap_or(now);
                    let pending = PendingTimer {
                        id: Uuid::new_v4(),
                        instance_id,
                        node_id: target_definition.clone(), // Abusing node_id to store the target definition for Scope Events! Or we need a special token? Wait!
                        expires_at,
                        token_id: Uuid::nil(), // No specific token, applies to whole instance
                        timer_def: Some(timer.clone()),
                        remaining_repetitions: None,
                    };
                    self.pending_timers.insert(pending.id, pending);
                }
                ScopeEventListener::Message {
                    message_name,
                    is_interrupting: _,
                    target_definition,
                } => {
                    let pending = PendingMessageCatch {
                        id: Uuid::new_v4(),
                        instance_id,
                        node_id: target_definition.clone(),
                        message_name: message_name.clone(),
                        token_id: Uuid::nil(),
                    };
                    self.pending_message_catches.insert(pending.id, pending);
                }
                ScopeEventListener::Error { .. } => {
                    // Error is checked dynamically when an ErrorEndEvent is hit or engine panics.
                }
            }
        }

        Box::pin(self.run_instance_batch(instance_id, token)).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }

    /// Spawns a call activity sub-process
    pub(crate) async fn spawn_call_activity(
        &self,
        child_def_key: Uuid,
        parent_instance_id: Uuid,
        called_node: String,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        let child_id = self
            .start_instance_with_variables_and_parent(
                child_def_key,
                variables,
                Some(parent_instance_id),
            )
            .await?;

        self.record_history_event(
            parent_instance_id,
            crate::history::HistoryEventType::CallActivityStarted,
            &format!(
                "Started Call Activity '{}' (child instance {})",
                called_node, child_id
            ),
            crate::history::ActorType::Engine,
            None,
            None,
        )
        .await;

        Ok(child_id)
    }

    /// Checks if a completed instance has a parent, and if so, resumes the parent.
    pub(crate) async fn resume_parent_if_needed(
        &self,
        completed_instance_id: Uuid,
        error_code: Option<String>,
    ) -> EngineResult<()> {
        let inst_arc = self
            .instances
            .get(&completed_instance_id)
            .await
            .ok_or(EngineError::NoSuchInstance(completed_instance_id))?;
        let inst = inst_arc.read().await;

        let parent_id = match inst.parent_instance_id {
            Some(pid) => pid,
            None => return Ok(()),
        };

        let child_vars = inst.variables.clone();

        // Find the parent
        tracing::info!(
            "Child instance {completed_instance_id} completed, resuming parent {parent_id}"
        );

        let (called_node_id, token_to_resume, def_key) = {
            let parent_arc = self
                .instances
                .get(&parent_id)
                .await
                .ok_or(EngineError::NoSuchInstance(parent_id))?;
            let mut parent = parent_arc.write().await;

            let (called_node_id, mut token_to_resume) =
                if let InstanceState::WaitingOnCallActivity { token, .. } = &parent.state {
                    let t = token.clone();
                    parent.state = InstanceState::Running;
                    (parent.current_node.clone(), Some(t))
                } else {
                    return Ok(());
                };

            parent.push_audit_log(format!(
                "🔗 Call Activity '{called_node_id}' completed successfully"
            ));

            let def_key = parent.definition_key;

            if let Some(active) = parent
                .active_tokens
                .iter_mut()
                .find(|at| at.token.current_node == called_node_id && !at.completed)
            {
                active.token.variables.extend(child_vars.clone());
                token_to_resume = Some(active.token.clone());
            } else if let Some(ref mut linear_token) = token_to_resume {
                linear_token.variables.extend(child_vars.clone());
            }

            (called_node_id, token_to_resume, def_key)
        };

        self.record_history_event(
            parent_id,
            crate::history::HistoryEventType::CallActivityCompleted,
            &format!(
                "Call Activity '{}' completed (child instance {})",
                called_node_id, completed_instance_id
            ),
            crate::history::ActorType::Engine,
            None,
            None,
        )
        .await;

        if let Some(mut token) = token_to_resume {
            let def = self
                .definitions
                .get(&def_key)
                .ok_or(EngineError::NoSuchDefinition(def_key))?;

            self.run_end_scripts(parent_id, &mut token, &def, &called_node_id)
                .await?;

            // Check for BPMN Error handling first
            let mut handle_as_incident = false;
            let mut target_boundary = None;

            if let Some(code) = &error_code {
                if let Some(boundary_id) =
                    crate::engine::executor::find_boundary_error_event(&def, &called_node_id, code)
                {
                    target_boundary = Some(boundary_id);
                } else {
                    handle_as_incident = true;
                }
            }

            if handle_as_incident {
                let parent_arc = self
                    .instances
                    .get(&parent_id)
                    .await
                    .ok_or(EngineError::NoSuchInstance(parent_id))?;
                let mut inst = parent_arc.write().await;
                inst.state = InstanceState::WaitingOnCallActivity {
                    sub_instance_id: completed_instance_id,
                    token: token.clone(),
                };
                inst.push_audit_log(format!("💥 INCIDENT: Child {completed_instance_id} failed with unhandled BPMN error '{}'", error_code.as_deref().unwrap_or_default()));
                self.persist_instance(parent_id).await;
                return Ok(());
            }

            let next_node = if let Some(bound_id) = target_boundary {
                bound_id
            } else {
                crate::engine::executor::resolve_next_target(
                    &def,
                    &called_node_id,
                    &token.variables,
                )?
            };

            token.current_node = next_node.clone();

            if let Some(p_inst_arc) = self.instances.get(&parent_id).await {
                let mut p_inst = p_inst_arc.write().await;
                p_inst.current_node = next_node;
            }

            // Run the batch for the parent
            Box::pin(self.run_instance_batch(parent_id, token)).await?;
        }

        Ok(())
    }

    /// Simulates an external timer trigger that starts a timer-start-event process.
    ///
    /// Validates the duration against the definition, then spawns the instance.
    pub async fn trigger_timer_start(
        &self,
        definition_key: Uuid,
        provided_duration: Duration,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        match start_element {
            BpmnElement::TimerStartEvent(expected_timer) => {
                let is_match = match expected_timer {
                    crate::domain::TimerDefinition::Duration(d) => *d == provided_duration,
                    _ => false,
                };
                if !is_match {
                    return Err(EngineError::TimerMismatch {
                        expected: match expected_timer {
                            crate::domain::TimerDefinition::Duration(d) => d.as_secs(),
                            _ => 0,
                        },
                        provided: provided_duration.as_secs(),
                    });
                }
            }
            _ => {
                return Err(EngineError::InvalidDefinition(
                    "Start event is not a timer start event".into(),
                ));
            }
        }

        let start_id = start_id.to_string();
        let instance_id = Uuid::new_v4();
        let business_key = Uuid::new_v4().to_string();
        let instance = ProcessInstance {
            id: instance_id,
            definition_key,
            business_key,
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: start_id.clone(),
            audit_log: vec![format!(
                "⏰ Timer fired ({}s) — started at node '{start_id}'",
                provided_duration.as_secs()
            )],
            variables: HashMap::new(),
            tokens: HashMap::new(),
            active_tokens: Vec::new(),
            join_barriers: std::collections::HashMap::new(),
            multi_instance_state: std::collections::HashMap::new(),
            compensation_log: Vec::new(),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
        };

        tracing::info!(
            "Timer-started instance {instance_id} of def key {definition_key} ({}s)",
            provided_duration.as_secs()
        );

        self.instances.insert(instance_id, instance).await;

        // Record history for start
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Timer fired for instance of process '{}'", def.id),
            crate::history::ActorType::Timer,
            None,
            None,
        )
        .await;

        let token = Token::new(&start_id);

        self.run_instance_batch(instance_id, token).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }

    /// Starts a timer-start-event process, respecting the timer definition.
    ///
    /// - Duration: starts one instance immediately
    /// - RepeatingInterval (R3/PT30S): starts the first instance immediately,
    ///   then spawns a background task for the remaining repetitions
    /// - CronCycle: starts one instance immediately
    ///
    /// Returns the first instance ID.
    pub async fn start_timer_instance(
        self: &Arc<Self>,
        definition_key: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        let timer_def = match start_element {
            BpmnElement::TimerStartEvent(td) => td.clone(),
            _ => {
                return Err(EngineError::InvalidDefinition(
                    "Start event is not a timer start event".into(),
                ));
            }
        };

        let start_id: String = start_id.to_string();

        // Determine total repetitions for metadata
        let (total, interval_secs) = match &timer_def {
            crate::domain::TimerDefinition::RepeatingInterval {
                repetitions,
                interval,
            } => (repetitions.unwrap_or(1), Some(interval.as_secs())),
            _ => (1, None),
        };

        // Start the first instance immediately
        let first_id = self
            .spawn_timer_instance(
                definition_key,
                &start_id,
                &variables,
                1,
                total,
                interval_secs,
            )
            .await?;

        // For repeating intervals, schedule remaining repetitions in background
        if let crate::domain::TimerDefinition::RepeatingInterval {
            repetitions,
            interval,
        } = timer_def
        {
            let remaining = repetitions.map(|r| r.saturating_sub(1)).unwrap_or(u32::MAX);
            if remaining > 0 {
                let engine = Arc::clone(self);
                let vars = variables;
                let sid = start_id;
                tokio::spawn(async move {
                    for i in 0..remaining {
                        tokio::time::sleep(interval).await;
                        let iteration = i + 2; // first was #1
                        if let Err(e) = engine
                            .spawn_timer_instance(
                                definition_key,
                                &sid,
                                &vars,
                                iteration,
                                total,
                                interval_secs,
                            )
                            .await
                        {
                            tracing::error!(
                                "Timer repeat #{iteration} failed for def {definition_key}: {e:?}"
                            );
                            break;
                        }
                    }
                    tracing::info!(
                        "Timer cycle completed for def {definition_key} ({} total instances)",
                        remaining + 1
                    );
                });
            }
        }

        Ok(first_id)
    }

    /// Spawns a single instance for a timer start event.
    async fn spawn_timer_instance(
        &self,
        definition_key: Uuid,
        start_id: &str,
        variables: &HashMap<String, Value>,
        iteration: u32,
        total: u32,
        interval_secs: Option<u64>,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let instance_id = Uuid::new_v4();
        let mut vars = variables.clone();
        let business_key = vars
            .remove("business_key")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Timer cycle metadata for UI display
        vars.insert("_timer_iteration".into(), Value::from(iteration));
        vars.insert("_timer_total".into(), Value::from(total));
        vars.insert(
            "_timer_start_node".into(),
            Value::String(start_id.to_string()),
        );
        if let Some(secs) = interval_secs {
            vars.insert("_timer_interval_secs".into(), Value::from(secs));
        }

        let instance = ProcessInstance {
            id: instance_id,
            definition_key,
            business_key,
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: start_id.to_string(),
            audit_log: vec![format!(
                "⏰ Timer instance #{iteration}/{total} started at node '{start_id}' with {} variable(s)",
                vars.len()
            )],
            variables: vars,
            tokens: HashMap::new(),
            active_tokens: Vec::new(),
            join_barriers: std::collections::HashMap::new(),
            multi_instance_state: std::collections::HashMap::new(),
            compensation_log: Vec::new(),
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
        };

        tracing::info!("Timer instance #{iteration} {instance_id} of def key {definition_key}");

        self.instances.insert(instance_id, instance).await;

        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Timer instance #{iteration} of process '{}'", def.id),
            crate::history::ActorType::Timer,
            None,
            None,
        )
        .await;

        let token = Token::new(start_id);
        self.run_instance_batch(instance_id, token).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }
}
