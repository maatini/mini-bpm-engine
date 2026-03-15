use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, ProcessDefinition, Token};
use crate::persistence::WorkflowPersistence;

// ---------------------------------------------------------------------------
// Service handler
// ---------------------------------------------------------------------------

/// Type alias for a service handler function.
///
/// Receives a mutable reference to the token's variables and returns a Result.
/// For async work, wrap the handler in a `tokio::spawn` block.
pub type ServiceHandlerFn =
    Arc<dyn Fn(&mut HashMap<String, Value>) -> EngineResult<()> + Send + Sync>;

// ---------------------------------------------------------------------------
// Pending user task
// ---------------------------------------------------------------------------

/// A user task that is waiting for external completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUserTask {
    pub task_id: Uuid,
    pub instance_id: Uuid,
    pub node_id: String,
    pub assignee: String,
    pub token: Token,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Next action (execution result)
// ---------------------------------------------------------------------------

/// The result of executing a single step in the process.
#[derive(Debug, Serialize, Deserialize)]
pub enum NextAction {
    /// The token should continue to the next node.
    Continue(Token),
    /// The engine must pause — a user task is pending.
    WaitForUser(PendingUserTask),
    /// The process reached an end event.
    Complete,
}

// ---------------------------------------------------------------------------
// Instance state
// ---------------------------------------------------------------------------

/// The state of a process instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceState {
    Running,
    WaitingOnUserTask { task_id: Uuid },
    Completed,
}

// ---------------------------------------------------------------------------
// Process instance
// ---------------------------------------------------------------------------

/// A live process instance tracked by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInstance {
    #[allow(dead_code)]
    pub id: Uuid,
    pub definition_id: String,
    pub state: InstanceState,
    pub current_node: String,
    pub audit_log: Vec<String>,
    /// Current process variables (synced from the executing token).
    pub variables: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Workflow engine
// ---------------------------------------------------------------------------

/// The central workflow engine managing definitions, instances, and handlers.
pub struct WorkflowEngine {
    pub definitions: HashMap<String, ProcessDefinition>,
    pub instances: HashMap<Uuid, ProcessInstance>,
    pub service_handlers: HashMap<String, ServiceHandlerFn>,
    pub pending_user_tasks: Vec<PendingUserTask>,
    pub persistence: Option<Arc<dyn WorkflowPersistence>>,
}

impl WorkflowEngine {
    /// Creates a new, empty engine.
    pub fn new() -> Self {
        log::info!("WorkflowEngine initialized");
        Self {
            definitions: HashMap::new(),
            instances: HashMap::new(),
            service_handlers: HashMap::new(),
            pending_user_tasks: Vec::new(),
            persistence: None,
        }
    }

    /// Attaches a persistence layer to the engine.
    pub fn with_persistence(mut self, persistence: Arc<dyn WorkflowPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    // ----- deployment ------------------------------------------------------

    /// Deploys a process definition so instances can be started from it.
    pub fn deploy_definition(&mut self, definition: ProcessDefinition) {
        log::info!("Deployed definition '{}'", definition.id);
        self.definitions.insert(definition.id.clone(), definition);
    }

    // ----- handler registration --------------------------------------------

    /// Registers a service handler function for a given service-task name.
    pub fn register_service_handler(
        &mut self,
        name: impl Into<String>,
        handler: ServiceHandlerFn,
    ) {
        let name = name.into();
        log::info!("Registered service handler '{name}'");
        self.service_handlers.insert(name, handler);
    }

    // ----- starting instances ----------------------------------------------

    /// Starts a new process instance from a deployed definition.
    ///
    /// The definition must have a plain `StartEvent`.
    pub async fn start_instance(&mut self, definition_id: &str) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(definition_id)
            .ok_or_else(|| EngineError::NoSuchDefinition(definition_id.to_string()))?;

        let (start_id, start_element) = def
            .start_event()
            .ok_or_else(|| EngineError::InvalidDefinition("No start event".into()))?;

        if matches!(start_element, BpmnElement::TimerStartEvent(_)) {
            return Err(EngineError::InvalidDefinition(
                "Use trigger_timer_start() for timer start events".into(),
            ));
        }

        let instance_id = Uuid::new_v4();
        let instance = ProcessInstance {
            id: instance_id,
            definition_id: definition_id.to_string(),
            state: InstanceState::Running,
            current_node: start_id.to_string(),
            audit_log: vec![format!("▶ Process started at node '{start_id}'")],
            variables: HashMap::new(),
        };

        log::info!(
            "Started instance {instance_id} of '{definition_id}' at node '{start_id}'"
        );

        self.instances.insert(instance_id, instance);
        let token = Token::new(start_id);
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save initial token: {}", e);
            }
        }
        self.run_instance(instance_id, token).await?;

        Ok(instance_id)
    }

    /// Simulates an external timer trigger that starts a timer-start-event process.
    ///
    /// Validates the duration against the definition, then spawns the instance.
    pub async fn trigger_timer_start(
        &mut self,
        definition_id: &str,
        provided_duration: Duration,
    ) -> EngineResult<Uuid> {
        let def = self
            .definitions
            .get(definition_id)
            .ok_or_else(|| EngineError::NoSuchDefinition(definition_id.to_string()))?;

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
        let instance = ProcessInstance {
            id: instance_id,
            definition_id: definition_id.to_string(),
            state: InstanceState::Running,
            current_node: start_id.clone(),
            audit_log: vec![format!(
                "⏰ Timer fired ({}s) — started at node '{start_id}'",
                provided_duration.as_secs()
            )],
            variables: HashMap::new(),
        };

        log::info!(
            "Timer-started instance {instance_id} of '{definition_id}' ({}s)",
            provided_duration.as_secs()
        );

        self.instances.insert(instance_id, instance);
        let token = Token::new(&start_id);
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save initial token (timer): {}", e);
            }
        }
        self.run_instance(instance_id, token).await?;

        Ok(instance_id)
    }

    /// Schedules a timer that, after sleeping for the given duration,
    /// will trigger a timer-start instance. Returns immediately.
    ///
    /// Note: this uses `tokio::time::sleep` in a spawned task. The engine
    /// reference is not carried into the task — instead the caller should
    /// poll or use channels in a production setup. For the demo, we return
    /// the duration and let the main code handle it.
    pub fn schedule_timer_start(
        &self,
        definition_id: &str,
        duration: Duration,
    ) -> EngineResult<()> {
        if !self.definitions.contains_key(definition_id) {
            return Err(EngineError::NoSuchDefinition(definition_id.to_string()));
        }

        let def_id = definition_id.to_string();
        log::info!(
            "Scheduled timer for '{def_id}' — will fire in {}s",
            duration.as_secs()
        );

        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            log::info!("⏰ Timer fired for '{def_id}' after {}s", duration.as_secs());
            // In a real engine this would send a message via mpsc channel
            // to the engine to start the instance. For demo purposes we log.
        });

        Ok(())
    }

    // ----- execution -------------------------------------------------------

    /// Runs an instance by repeatedly executing steps until a wait-state or end.
    async fn run_instance(
        &mut self,
        instance_id: Uuid,
        mut token: Token,
    ) -> EngineResult<()> {
        loop {
            let action = self.execute_step(instance_id, &mut token).await?;

            match action {
                NextAction::Continue(next_token) => {
                    token = next_token;
                    if let Some(p) = &self.persistence {
                        if let Err(e) = p.save_token(&token).await {
                            log::error!("Failed to save token: {}", e);
                        }
                    }
                }
                NextAction::WaitForUser(pending) => {
                    let task_id = pending.task_id;
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::WaitingOnUserTask { task_id };
                    }
                    self.pending_user_tasks.push(pending);
                    return Ok(());
                }
                NextAction::Complete => {
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::Completed;
                    }
                    return Ok(());
                }
            }
        }
    }

    /// Executes a single step for the given token position.
    async fn execute_step(
        &mut self,
        instance_id: Uuid,
        token: &mut Token,
    ) -> EngineResult<NextAction> {
        let instance = self
            .instances
            .get(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;

        let def = self
            .definitions
            .get(&instance.definition_id)
            .ok_or_else(|| EngineError::NoSuchDefinition(instance.definition_id.clone()))?;

        let current_id = token.current_node.clone();
        let element = def
            .get_node(&current_id)
            .ok_or_else(|| EngineError::NoSuchNode(current_id.clone()))?
            .clone();

        // We need to re-borrow mutably for audit log updates
        let def_clone = def.clone();

        match &element {
            BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_) => {
                log::debug!("Passing through start event '{current_id}'");
                let next = def_clone
                    .next_node(&current_id)
                    .ok_or_else(|| {
                        EngineError::InvalidDefinition(format!(
                            "No outgoing flow from '{current_id}'"
                        ))
                    })?
                    .to_string();
                token.current_node = next;
                Ok(NextAction::Continue(token.clone()))
            }

            BpmnElement::EndEvent => {
                let inst = self.instances.get_mut(&instance_id).unwrap();
                inst.audit_log
                    .push(format!("⏹ Process completed at end event '{current_id}'"));
                log::info!("Instance {instance_id}: reached end event '{current_id}'");
                Ok(NextAction::Complete)
            }

            BpmnElement::ServiceTask(handler_name) => {
                let handler = self
                    .service_handlers
                    .get(handler_name)
                    .ok_or_else(|| EngineError::HandlerNotFound(handler_name.clone()))?
                    .clone();

                // Execute the handler
                handler(&mut token.variables)?;

                let inst = self.instances.get_mut(&instance_id).unwrap();
                inst.audit_log.push(format!(
                    "⚙ Executed service task '{current_id}' (handler: {handler_name})"
                ));
                log::info!(
                    "Instance {instance_id}: executed service task '{current_id}' → '{handler_name}'"
                );

                let next = def_clone
                    .next_node(&current_id)
                    .ok_or_else(|| {
                        EngineError::InvalidDefinition(format!(
                            "No outgoing flow from '{current_id}'"
                        ))
                    })?
                    .to_string();
                token.current_node = next.clone();
                inst.current_node = next;
                inst.variables = token.variables.clone();
                Ok(NextAction::Continue(token.clone()))
            }

            BpmnElement::UserTask(assignee) => {
                let pending = PendingUserTask {
                    task_id: Uuid::new_v4(),
                    instance_id,
                    node_id: current_id.clone(),
                    assignee: assignee.clone(),
                    token: token.clone(),
                    created_at: Utc::now(),
                };

                let inst = self.instances.get_mut(&instance_id).unwrap();
                inst.audit_log.push(format!(
                    "👤 User task '{current_id}' assigned to '{assignee}' — waiting (task_id: {})",
                    pending.task_id
                ));
                log::info!(
                    "Instance {instance_id}: user task '{current_id}' pending for '{assignee}' (task_id: {})",
                    pending.task_id
                );

                Ok(NextAction::WaitForUser(pending))
            }
        }
    }

    // ----- user task completion ---------------------------------------------

    /// Completes a pending user task by its task_id, optionally merging variables.
    ///
    /// Resumes the process instance after the user task.
    pub async fn complete_user_task(
        &mut self,
        task_id: Uuid,
        additional_vars: HashMap<String, Value>,
    ) -> EngineResult<()> {
        // Find and remove the pending task
        let idx = self
            .pending_user_tasks
            .iter()
            .position(|p| p.task_id == task_id)
            .ok_or_else(|| EngineError::TaskNotPending {
                task_id,
                actual_state: "not found in pending tasks".into(),
            })?;

        let pending = self.pending_user_tasks.remove(idx);
        let instance_id = pending.instance_id;

        // Merge additional variables into the token
        let mut token = pending.token;
        for (k, v) in additional_vars {
            token.variables.insert(k, v);
        }

        log::info!(
            "Instance {instance_id}: completed user task '{}' (task_id: {task_id})",
            pending.node_id
        );

        let inst = self
            .instances
            .get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        inst.audit_log
            .push(format!("✅ User task '{}' completed", pending.node_id));
        inst.state = InstanceState::Running;
        inst.variables = token.variables.clone();

        // Advance token to the next node
        let def = self
            .definitions
            .get(&inst.definition_id)
            .ok_or_else(|| EngineError::NoSuchDefinition(inst.definition_id.clone()))?;

        let next = def
            .next_node(&pending.node_id)
            .ok_or_else(|| {
                EngineError::InvalidDefinition(format!(
                    "No outgoing flow from '{}'",
                    pending.node_id
                ))
            })?
            .to_string();

        token.current_node = next;
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save token after user task: {}", e);
            }
        }

        // Continue running
        self.run_instance(instance_id, token).await
    }

    // ----- query helpers ---------------------------------------------------

    /// Returns the state of a process instance.
    pub fn get_instance_state(&self, instance_id: Uuid) -> EngineResult<&InstanceState> {
        self.instances
            .get(&instance_id)
            .map(|i| &i.state)
            .ok_or(EngineError::NoSuchInstance(instance_id))
    }

    /// Returns the audit log of a process instance.
    pub fn get_audit_log(&self, instance_id: Uuid) -> EngineResult<&[String]> {
        self.instances
            .get(&instance_id)
            .map(|i| i.audit_log.as_slice())
            .ok_or(EngineError::NoSuchInstance(instance_id))
    }

    /// Returns all currently pending user tasks.
    pub fn get_pending_user_tasks(&self) -> &[PendingUserTask] {
        &self.pending_user_tasks
    }

    /// Returns a list of all process instances (cloned).
    pub fn list_instances(&self) -> Vec<ProcessInstance> {
        self.instances.values().cloned().collect()
    }

    /// Returns full details for a single process instance.
    pub fn get_instance_details(&self, instance_id: Uuid) -> EngineResult<ProcessInstance> {
        self.instances
            .get(&instance_id)
            .cloned()
            .ok_or(EngineError::NoSuchInstance(instance_id))
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProcessDefinitionBuilder;

    fn setup_linear_engine() -> (WorkflowEngine, String) {
        let mut engine = WorkflowEngine::new();

        // Register a simple service handler
        engine.register_service_handler(
            "validate",
            Arc::new(|vars: &mut HashMap<String, Value>| {
                vars.insert("validated".into(), Value::Bool(true));
                Ok(())
            }),
        );

        let def = ProcessDefinitionBuilder::new("linear")
            .node("start", BpmnElement::StartEvent)
            .node("svc", BpmnElement::ServiceTask("validate".into()))
            .node("ut", BpmnElement::UserTask("alice".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "ut")
            .flow("ut", "end")
            .build()
            .unwrap();

        let def_id = def.id.clone();
        engine.deploy_definition(def);
        (engine, def_id)
    }

    #[tokio::test]
    async fn start_instance_pauses_at_user_task() {
        let (mut engine, def_id) = setup_linear_engine();
        let inst_id = engine.start_instance(&def_id).await.unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::WaitingOnUserTask {
                task_id: engine.pending_user_tasks[0].task_id
            }
        );
        assert_eq!(engine.pending_user_tasks.len(), 1);
    }

    #[tokio::test]
    async fn complete_user_task_reaches_end() {
        let (mut engine, def_id) = setup_linear_engine();
        let inst_id = engine.start_instance(&def_id).await.unwrap();

        let task_id = engine.pending_user_tasks[0].task_id;
        engine
            .complete_user_task(task_id, HashMap::new())
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
        assert!(engine.pending_user_tasks.is_empty());
    }

    #[tokio::test]
    async fn completing_wrong_task_gives_error() {
        let (mut engine, def_id) = setup_linear_engine();
        engine.start_instance(&def_id).await.unwrap();

        let wrong_id = Uuid::new_v4();
        let result = engine
            .complete_user_task(wrong_id, HashMap::new())
            .await;
        assert!(matches!(result, Err(EngineError::TaskNotPending { .. })));
    }

    #[tokio::test]
    async fn service_handler_modifies_variables() {
        let (mut engine, def_id) = setup_linear_engine();
        engine.start_instance(&def_id).await.unwrap();

        // The token should have 'validated: true' from the service handler
        let pending = &engine.pending_user_tasks[0];
        assert_eq!(
            pending.token.variables.get("validated"),
            Some(&Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn timer_start_succeeds() {
        let mut engine = WorkflowEngine::new();
        let dur = Duration::from_secs(60);

        let def = ProcessDefinitionBuilder::new("timer_proc")
            .node("ts", BpmnElement::TimerStartEvent(dur))
            .node("end", BpmnElement::EndEvent)
            .flow("ts", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);
        let inst_id = engine.trigger_timer_start("timer_proc", dur).await.unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
    }

    #[tokio::test]
    async fn timer_mismatch_gives_error() {
        let mut engine = WorkflowEngine::new();

        let def = ProcessDefinitionBuilder::new("timer_proc")
            .node("ts", BpmnElement::TimerStartEvent(Duration::from_secs(60)))
            .node("end", BpmnElement::EndEvent)
            .flow("ts", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);
        let result = engine
            .trigger_timer_start("timer_proc", Duration::from_secs(30))
            .await;
        assert!(matches!(result, Err(EngineError::TimerMismatch { .. })));
    }

    #[tokio::test]
    async fn plain_start_rejects_timer_def() {
        let mut engine = WorkflowEngine::new();

        let def = ProcessDefinitionBuilder::new("timer_proc")
            .node("ts", BpmnElement::TimerStartEvent(Duration::from_secs(5)))
            .node("end", BpmnElement::EndEvent)
            .flow("ts", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);
        let result = engine.start_instance("timer_proc").await;
        assert!(matches!(
            result,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("timer")
        ));
    }

    #[tokio::test]
    async fn unknown_definition_gives_error() {
        let mut engine = WorkflowEngine::new();
        let result = engine.start_instance("nonexistent").await;
        assert!(matches!(
            result,
            Err(EngineError::NoSuchDefinition(_))
        ));
    }

    #[tokio::test]
    async fn missing_handler_gives_error() {
        let mut engine = WorkflowEngine::new();

        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("svc", BpmnElement::ServiceTask("unknown_handler".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);
        let result = engine.start_instance("p1").await;
        assert!(matches!(result, Err(EngineError::HandlerNotFound(_))));
    }

    #[tokio::test]
    async fn audit_log_captures_all_steps() {
        let (mut engine, def_id) = setup_linear_engine();
        let inst_id = engine.start_instance(&def_id).await.unwrap();

        let task_id = engine.pending_user_tasks[0].task_id;
        engine
            .complete_user_task(task_id, HashMap::new())
            .await
            .unwrap();

        let log = engine.get_audit_log(inst_id).unwrap();
        assert!(log.len() >= 4);
        assert!(log[0].contains("started"));
        assert!(log.last().unwrap().contains("completed"));
    }
}
