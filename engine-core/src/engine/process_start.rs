use std::collections::HashMap;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, Token};

use super::{InstanceState, ProcessInstance, WorkflowEngine};

impl WorkflowEngine {
    /// Starts a new process instance from a deployed definition.
    ///
    /// The definition must have a plain `StartEvent`.
    /// Delegates to `start_instance_with_variables` with an empty variable map.
    pub async fn start_instance(&mut self, definition_key: Uuid) -> EngineResult<Uuid> {
        self.start_instance_with_variables(definition_key, HashMap::new()).await
    }

    /// Starts a new process instance with pre-populated variables.
    ///
    /// Like `start_instance`, but the token carries initial variables from the
    /// caller. The instance's `variables` field is also seeded.
    pub async fn start_instance_with_variables(
        &mut self,
        definition_key: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        self.start_instance_with_variables_and_parent(definition_key, variables, None).await
    }

    /// Starts a new instance from the latest version of a BPMN process ID.
    ///
    /// Looks up the definition with the highest version matching `bpmn_id`
    /// and starts an instance from it. Returns the (instance_id, definition_key) tuple.
    pub async fn start_instance_latest(
        &mut self,
        bpmn_id: &str,
        variables: HashMap<String, Value>,
    ) -> EngineResult<(Uuid, Uuid)> {
        let (def_key, _) = self.definitions.find_latest_by_bpmn_id(bpmn_id)
            .await
            .ok_or_else(|| EngineError::InvalidDefinition(
                format!("No definition found with BPMN ID '{}'", bpmn_id)
            ))?;
        let inst_id = self.start_instance_with_variables(def_key, variables).await?;
        Ok((inst_id, def_key))
    }

    /// Internal method to start an instance and link it to a parent call activity
    pub(crate) async fn start_instance_with_variables_and_parent(
        &mut self,
        definition_key: Uuid,
        mut variables: HashMap<String, Value>,
        parent_instance_id: Option<Uuid>,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .await
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
        };

        log::info!(
            "Started instance {instance_id} of def key {definition_key} at node '{start_id}' with {} vars",
            variables.len()
        );

        self.instances.insert(instance_id, instance).await;
        
        // Record history for start
        self.record_history_event(
            instance_id,
            crate::history::HistoryEventType::InstanceStarted,
            &format!("Started instance of process '{}'", def.id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;

        let token = Token::with_variables(start_id, variables);

        Box::pin(self.run_instance_batch(instance_id, token)).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }

    /// Spawns a call activity sub-process
    pub(crate) async fn spawn_call_activity(
        &mut self,
        child_def_key: Uuid,
        parent_instance_id: Uuid,
        called_node: String,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
        let child_id = self.start_instance_with_variables_and_parent(child_def_key, variables, Some(parent_instance_id)).await?;
        
        self.record_history_event(
            parent_instance_id,
            crate::history::HistoryEventType::CallActivityStarted,
            &format!("Started Call Activity '{}' (child instance {})", called_node, child_id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;
        
        Ok(child_id)
    }

    /// Checks if a completed instance has a parent, and if so, resumes the parent.
    pub(crate) async fn resume_parent_if_needed(&mut self, completed_instance_id: Uuid, error_code: Option<String>) -> EngineResult<()> {
        let inst_arc = self.instances.get(&completed_instance_id).await.ok_or(EngineError::NoSuchInstance(completed_instance_id))?;
        let inst = inst_arc.read().await;
            
        let parent_id = match inst.parent_instance_id {
            Some(pid) => pid,
            None => return Ok(()),
        };
        
        let child_vars = inst.variables.clone();
        
        // Find the parent
        log::info!("Child instance {completed_instance_id} completed, resuming parent {parent_id}");
        
        let (called_node_id, token_to_resume, def_key) = {
            let parent_arc = self.instances.get(&parent_id).await.ok_or(EngineError::NoSuchInstance(parent_id))?;
            let mut parent = parent_arc.write().await;
                
            let (called_node_id, mut token_to_resume) = if let InstanceState::WaitingOnCallActivity { token, .. } = &parent.state {
                let t = token.clone();
                parent.state = InstanceState::Running;
                (parent.current_node.clone(), Some(t))
            } else {
                return Ok(());
            };
            
            parent.audit_log.push(format!("🔗 Call Activity '{called_node_id}' completed successfully"));
            
            let def_key = parent.definition_key;
            
            if let Some(active) = parent.active_tokens.iter_mut().find(|at| at.token.current_node == called_node_id && !at.completed) {
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
            &format!("Call Activity '{}' completed (child instance {})", called_node_id, completed_instance_id),
            crate::history::ActorType::Engine,
            None,
            None
        ).await;

        if let Some(mut token) = token_to_resume {
            let def = self.definitions.get(&def_key).await
                .ok_or(EngineError::NoSuchDefinition(def_key))?;
                
            self.run_end_scripts(parent_id, &mut token, &def, &called_node_id).await?;
            
            // Check for BPMN Error handling first
            let mut handle_as_incident = false;
            let mut target_boundary = None;
            
            if let Some(code) = &error_code {
                if let Some(boundary_id) = crate::engine::executor::find_boundary_error_event(&def, &called_node_id, code) {
                    target_boundary = Some(boundary_id);
                } else {
                    handle_as_incident = true;
                }
            }

            if handle_as_incident {
                let parent_arc = self.instances.get(&parent_id).await.ok_or(EngineError::NoSuchInstance(parent_id))?;
                let mut inst = parent_arc.write().await;
                inst.state = InstanceState::WaitingOnCallActivity { sub_instance_id: completed_instance_id, token: token.clone() };
                inst.audit_log.push(format!("💥 INCIDENT: Child {completed_instance_id} failed with unhandled BPMN error '{}'", error_code.as_deref().unwrap_or_default()));
                self.persist_instance(parent_id).await;
                return Ok(());
            }

            let next_node = if let Some(bound_id) = target_boundary {
                bound_id
            } else {
                crate::engine::executor::resolve_next_target(&def, &called_node_id, &token.variables)?
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
        &mut self,
        definition_key: Uuid,
        provided_duration: Duration,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(&definition_key)
            .await
            .ok_or(EngineError::NoSuchDefinition(definition_key))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        match start_element {
            BpmnElement::TimerStartEvent(expected_dur) => {
                if *expected_dur != provided_duration {
                    return Err(EngineError::TimerMismatch {
                        expected: expected_dur.as_secs(),
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
        };

        log::info!(
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
            None
        ).await;

        let token = Token::new(&start_id);

        self.run_instance_batch(instance_id, token).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
    }
}
