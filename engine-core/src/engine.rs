use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::condition::evaluate_condition;
use crate::error::{EngineError, EngineResult};
use crate::model::{BpmnElement, ListenerEvent, ProcessDefinition, Token};
use crate::persistence::WorkflowPersistence;
use crate::script_runner;

// Sub-modules with additional `impl WorkflowEngine` blocks
#[path = "external_task.rs"]
mod external_task;

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
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// External task item (Camunda-style)
// ---------------------------------------------------------------------------

/// An external task that can be fetched and completed by remote workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalTaskItem {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub definition_key: Uuid,
    pub node_id: String,
    pub topic: String,
    pub token: Token,
    pub created_at: DateTime<Utc>,
    /// The worker that currently holds the lock (None = unlocked).
    pub worker_id: Option<String>,
    /// When the lock expires (None = not locked).
    pub lock_expiration: Option<DateTime<Utc>>,
    /// Remaining retries before an incident is created.
    pub retries: i32,
    /// Error message from the last failure.
    pub error_message: Option<String>,
    /// Detailed error information from the last failure.
    pub error_details: Option<String>,
}

// ---------------------------------------------------------------------------
// Next action (execution result)
// ---------------------------------------------------------------------------

/// The result of executing a single step in the process.
#[derive(Debug, Serialize, Deserialize)]
pub enum NextAction {
    /// The token should continue to the next node.
    Continue(Token),
    /// Multiple tokens should continue (inclusive gateway fork).
    ContinueMultiple(Vec<Token>),
    /// The engine must pause — a user task is pending.
    WaitForUser(PendingUserTask),
    /// The engine must pause — an external task is pending.
    WaitForExternalTask(ExternalTaskItem),
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
    WaitingOnExternalTask { task_id: Uuid },
    Completed,
}

// ---------------------------------------------------------------------------
// Process instance
// ---------------------------------------------------------------------------

/// A live process instance tracked by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInstance {
    pub id: Uuid,
    pub definition_key: Uuid,
    pub business_key: String,
    pub state: InstanceState,
    pub current_node: String,
    pub audit_log: Vec<String>,
    /// Current process variables (synced from the executing token).
    pub variables: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Helper: resolve next target via condition evaluation
// ---------------------------------------------------------------------------

/// Resolves the next target node by evaluating conditions on outgoing flows.
///
/// Returns the target of the first matching flow (or the first unconditional
/// flow). Used by StartEvent, ServiceTask, and task-completion handlers.
fn resolve_next_target(
    def: &ProcessDefinition,
    from: &str,
    variables: &HashMap<String, Value>,
) -> EngineResult<String> {
    def.next_nodes(from)
        .iter()
        .find(|f| {
            f.condition
                .as_ref()
                .map(|c| evaluate_condition(c, variables))
                .unwrap_or(true)
        })
        .map(|f| f.target.clone())
        .ok_or_else(|| {
            EngineError::InvalidDefinition(format!(
                "No matching outgoing flow from '{from}'"
            ))
        })
}

// ---------------------------------------------------------------------------
// Workflow engine
// ---------------------------------------------------------------------------

/// The central workflow engine managing definitions, instances, and handlers.
pub struct WorkflowEngine {
    pub definitions: HashMap<Uuid, Arc<ProcessDefinition>>,
    pub instances: HashMap<Uuid, ProcessInstance>,
    pub service_handlers: HashMap<String, ServiceHandlerFn>,
    pub pending_user_tasks: Vec<PendingUserTask>,
    pub pending_external_tasks: Vec<ExternalTaskItem>,
    pub persistence: Option<Arc<dyn WorkflowPersistence>>,
    pub script_engine: rhai::Engine,
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
            pending_external_tasks: Vec::new(),
            persistence: None,
            script_engine: rhai::Engine::new(),
        }
    }

    /// Attaches a persistence layer to the engine.
    pub fn with_persistence(mut self, persistence: Arc<dyn WorkflowPersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Persists the current state of a process instance (if a persistence
    /// layer is configured). Logs and swallows errors.
    async fn persist_instance(&self, instance_id: Uuid) {
        if let (Some(p), Some(inst)) = (&self.persistence, self.instances.get(&instance_id)) {
            if let Err(e) = p.save_instance(inst).await {
                log::error!("Failed to persist instance {}: {}", instance_id, e);
            }
        }
    }

    /// Persists a process definition to the KV store.
    async fn persist_definition(&self, key: Uuid) {
        if let (Some(p), Some(def)) = (&self.persistence, self.definitions.get(&key)) {
            if let Err(e) = p.save_definition(def).await {
                log::error!("Failed to persist definition {}: {}", key, e);
            }
        }
    }

    /// Persists a pending user task to the KV store.
    async fn persist_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Some(task) = self.pending_user_tasks.iter().find(|t| t.task_id == task_id) {
                if let Err(e) = p.save_user_task(task).await {
                    log::error!("Failed to persist user task {}: {}", task_id, e);
                }
            }
        }
    }

    /// Deletes a completed pending user task from the KV store.
    async fn remove_persisted_user_task(&self, task_id: Uuid) {
        if let Some(p) = &self.persistence {
            if let Err(e) = p.delete_user_task(task_id).await {
                log::error!("Failed to delete persisted user task {}: {}", task_id, e);
            }
        }
    }

    // ----- deployment ------------------------------------------------------

    /// Deploys a process definition so instances can be started from it.
    ///
    /// Upsert semantics: if a definition with the same BPMN process ID already
    /// exists, its key is preserved and data is overwritten.
    /// Returns the definition key (UUID).
    pub async fn deploy_definition(&mut self, definition: ProcessDefinition) -> Uuid {
        // Upsert: reuse key if same BPMN process ID already deployed
        let existing_key = self.definitions.values()
            .find(|d| d.id == definition.id)
            .map(|d| d.key);

        let key = existing_key.unwrap_or(definition.key);
        let mut def = definition;
        def.key = key;
        log::info!("Deployed definition '{}' (key: {})", def.id, key);
        self.definitions.insert(key, Arc::new(def));
        self.persist_definition(key).await;
        key
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
        mut variables: HashMap<String, Value>,
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
            state: InstanceState::Running,
            current_node: start_id.to_string(),
            audit_log: vec![format!(
                "▶ Process started at node '{start_id}' with {} variable(s)",
                variables.len()
            )],
            variables: variables.clone(),
        };

        log::info!(
            "Started instance {instance_id} of def key {definition_key} at node '{start_id}' with {} vars",
            variables.len()
        );

        self.instances.insert(instance_id, instance);
        let token = Token::with_variables(start_id, variables);
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save initial token: {}", e);
            }
        }
        self.run_instance(instance_id, token).await?;
        self.persist_instance(instance_id).await;

        Ok(instance_id)
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
            state: InstanceState::Running,
            current_node: start_id.clone(),
            audit_log: vec![format!(
                "⏰ Timer fired ({}s) — started at node '{start_id}'",
                provided_duration.as_secs()
            )],
            variables: HashMap::new(),
        };

        log::info!(
            "Timer-started instance {instance_id} of def key {definition_key} ({}s)",
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
        self.persist_instance(instance_id).await;

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
        definition_key: Uuid,
        duration: Duration,
    ) -> EngineResult<()> {
        if !self.definitions.contains_key(&definition_key) {
            return Err(EngineError::NoSuchDefinition(definition_key));
        }

        log::info!(
            "Scheduled timer for def key '{definition_key}' — will fire in {}s",
            duration.as_secs()
        );

        tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            log::info!("⏰ Timer fired for def key '{definition_key}' after {}s", duration.as_secs());
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
                NextAction::ContinueMultiple(forked_tokens) => {
                    // Run each forked branch sequentially.
                    // All branches share the same instance and must
                    // complete independently (each reaching an EndEvent).
                    for fork_token in forked_tokens {
                        if let Some(p) = &self.persistence {
                            if let Err(e) = p.save_token(&fork_token).await {
                                log::error!("Failed to save forked token: {}", e);
                            }
                        }
                        // Temporarily set instance back to Running for each branch
                        if let Some(inst) = self.instances.get_mut(&instance_id) {
                            inst.state = InstanceState::Running;
                        }
                        // Recursively run each forked branch in a Box::pin to
                        // satisfy the borrow checker for recursive async
                        Box::pin(self.run_instance(instance_id, fork_token)).await?;
                    }
                    return Ok(());
                }
                NextAction::WaitForUser(pending) => {
                    let task_id = pending.task_id;
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::WaitingOnUserTask { task_id };
                    }
                    self.pending_user_tasks.push(pending);
                    self.persist_instance(instance_id).await;
                    self.persist_user_task(task_id).await;
                    return Ok(());
                }
                NextAction::WaitForExternalTask(ext_task) => {
                    let task_id = ext_task.id;
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::WaitingOnExternalTask { task_id };
                    }
                    self.persist_instance(instance_id).await;
                    self.pending_external_tasks.push(ext_task);
                    return Ok(());
                }
                NextAction::Complete => {
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::Completed;
                    }
                    self.persist_instance(instance_id).await;
                    return Ok(());
                }
            }
        }
    }

    /// Helper: runs End scripts, commits variables to instance state.
    fn run_end_scripts(
        &mut self,
        instance_id: Uuid,
        token: &mut Token,
        def: &ProcessDefinition,
        node_id: &str,
    ) -> EngineResult<()> {
        let inst = self.instances.get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        script_runner::run_end_scripts(
            &self.script_engine,
            instance_id,
            token,
            def,
            node_id,
            &mut inst.audit_log,
            &mut inst.variables,
        )
    }

    /// Executes a single step for the given token position.
    async fn execute_step(
        &mut self,
        instance_id: Uuid,
        token: &mut Token,
    ) -> EngineResult<NextAction> {
        let def_key = {
            let instance = self
                .instances
                .get(&instance_id)
                .ok_or(EngineError::NoSuchInstance(instance_id))?;
            instance.definition_key
        };

        let def = self
            .definitions
            .get(&def_key)
            .ok_or(EngineError::NoSuchDefinition(def_key))?;

        let current_id = token.current_node.clone();
        let element = def
            .get_node(&current_id)
            .ok_or_else(|| EngineError::NoSuchNode(current_id.clone()))?
            .clone();

        // Cheap Arc clone to release the immutable borrow on self.definitions
        let def_clone = Arc::clone(def);

        // Run start scripts
        let mut start_audits = Vec::new();
        script_runner::run_node_scripts(
            &self.script_engine,
            instance_id,
            token,
            &def_clone,
            &current_id,
            ListenerEvent::Start,
            &mut start_audits,
        )?;
        if let Some(inst) = self.instances.get_mut(&instance_id) {
            inst.audit_log.append(&mut start_audits);
            inst.variables = token.variables.clone();
        }

        match &element {
            BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_) => {
                log::debug!("Passing through start event '{current_id}'");
                let next = resolve_next_target(&def_clone, &current_id, &token.variables)?;
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)?;
                token.current_node = next.clone();
                // Keep instance in sync so the UI highlights the correct node
                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = next;
                Ok(NextAction::Continue(token.clone()))
            }

            BpmnElement::EndEvent => {
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)?;
                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = current_id.clone();
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

                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.audit_log.push(format!(
                    "⚙ Executed service task '{current_id}' (handler: {handler_name})"
                ));
                log::info!(
                    "Instance {instance_id}: executed service task '{current_id}' → '{handler_name}'"
                );

                let next = resolve_next_target(&def_clone, &current_id, &token.variables)?;
                self.run_end_scripts(instance_id, token, &def_clone, &current_id)?;
                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = next.clone();
                inst.variables = token.variables.clone();
                token.current_node = next;
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

                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = current_id.clone();
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

            BpmnElement::ExternalTask { topic } => {
                let ext_task = ExternalTaskItem {
                    id: Uuid::new_v4(),
                    instance_id,
                    definition_key: def_key,
                    node_id: current_id.clone(),
                    topic: topic.clone(),
                    token: token.clone(),
                    created_at: Utc::now(),
                    worker_id: None,
                    lock_expiration: None,
                    retries: 3,
                    error_message: None,
                    error_details: None,
                };

                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = current_id.clone();
                inst.audit_log.push(format!(
                    "🔗 External task '{current_id}' created for topic '{topic}' — waiting (task_id: {})",
                    ext_task.id
                ));
                log::info!(
                    "Instance {instance_id}: external task '{current_id}' pending for topic '{topic}' (task_id: {})",
                    ext_task.id
                );

                Ok(NextAction::WaitForExternalTask(ext_task))
            }

            // ----- Exclusive Gateway (XOR) -----
            BpmnElement::ExclusiveGateway { default } => {
                let outgoing = def_clone.next_nodes(&current_id);
                let mut chosen_target: Option<String> = None;

                // Evaluate conditions in order; first match wins
                for sf in outgoing {
                    if let Some(ref cond) = sf.condition {
                        if evaluate_condition(cond, &token.variables) {
                            chosen_target = Some(sf.target.clone());
                            break;
                        }
                    }
                }

                // Fallback to default flow if no condition matched
                if chosen_target.is_none() {
                    if let Some(default_target) = default {
                        chosen_target = Some(default_target.clone());
                    }
                }

                let target = chosen_target.ok_or_else(|| {
                    EngineError::NoMatchingCondition(current_id.clone())
                })?;

                self.run_end_scripts(instance_id, token, &def_clone, &current_id)?;

                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.audit_log.push(format!(
                    "◆ Exclusive gateway '{current_id}' → took path to '{target}'"
                ));
                log::info!(
                    "Instance {instance_id}: exclusive gateway '{current_id}' → '{target}'"
                );

                token.current_node = target.clone();
                inst.current_node = target;
                Ok(NextAction::Continue(token.clone()))
            }

            // ----- Inclusive Gateway (OR) -----
            BpmnElement::InclusiveGateway => {
                let outgoing = def_clone.next_nodes(&current_id);
                let mut matched_targets: Vec<String> = Vec::new();

                // Evaluate all conditions; every match is taken
                for sf in outgoing {
                    if let Some(ref cond) = sf.condition {
                        if evaluate_condition(cond, &token.variables) {
                            matched_targets.push(sf.target.clone());
                        }
                    } else {
                        // Unconditional flows are always taken
                        matched_targets.push(sf.target.clone());
                    }
                }

                if matched_targets.is_empty() {
                    return Err(EngineError::NoMatchingCondition(current_id.clone()));
                }

                self.run_end_scripts(instance_id, token, &def_clone, &current_id)?;

                let inst = self.instances.get_mut(&instance_id)
                    .ok_or(EngineError::NoSuchInstance(instance_id))?;
                inst.current_node = current_id.clone();
                inst.audit_log.push(format!(
                    "◇ Inclusive gateway '{current_id}' → forked to {} path(s): [{}]",
                    matched_targets.len(),
                    matched_targets.join(", ")
                ));
                log::info!(
                    "Instance {instance_id}: inclusive gateway '{current_id}' → {} path(s)",
                    matched_targets.len()
                );

                // Fork tokens — each gets a copy of the current variables
                let forked: Vec<Token> = matched_targets
                    .into_iter()
                    .map(|target| Token::with_variables(&target, token.variables.clone()))
                    .collect();

                if forked.len() == 1 {
                    // Only one match → no need for multi-token handling.
                    // SAFETY: we just verified `forked.len() == 1`, so this
                    // will always succeed. Using `expect` instead of `unwrap`
                    // for clearer panic context in case of a logic bug.
                    let single = forked.into_iter().next()
                        .expect("BUG: forked vec verified as len()==1 but is empty");
                    Ok(NextAction::Continue(single))
                } else {
                    Ok(NextAction::ContinueMultiple(forked))
                }
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

        self.remove_persisted_user_task(task_id).await;

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
        let def_key = inst.definition_key;

        // Advance token to the next node
        let def = self
            .definitions
            .get(&def_key)
            .ok_or(EngineError::NoSuchDefinition(def_key))?;
        let def = Arc::clone(def);

        self.run_end_scripts(instance_id, &mut token, &def, &pending.node_id)?;

        let next = resolve_next_target(&def, &pending.node_id, &token.variables)?;

        token.current_node = next.clone();
        // Update instance current_node so UI highlights correctly
        let inst = self.instances.get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        inst.current_node = next;
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

    /// Returns all pending external tasks (for debugging / admin).
    pub fn get_external_tasks(&self) -> &[ExternalTaskItem] {
        &self.pending_external_tasks
    }

    /// Returns a list of all process instances (cloned).
    pub fn list_instances(&self) -> Vec<ProcessInstance> {
        self.instances.values().cloned().collect()
    }

    /// Returns full details for a single process instance.
    pub fn get_instance_details(&self, id: Uuid) -> EngineResult<ProcessInstance> {
        self.instances
            .get(&id)
            .cloned()
            .ok_or(EngineError::NoSuchInstance(id))
    }

    /// Deletes a process instance and cleans up associated pending tasks.
    pub async fn delete_instance(&mut self, instance_id: Uuid) -> EngineResult<()> {
        if self.instances.remove(&instance_id).is_none() {
            return Err(EngineError::NoSuchInstance(instance_id));
        }

        if let Some(ref persistence) = self.persistence {
            // Delete associated user tasks from persistence
            for task in self.pending_user_tasks.iter().filter(|t| t.instance_id == instance_id) {
                let _ = persistence.delete_user_task(task.task_id).await;
            }
            // Delete instance from persistence
            persistence.delete_instance(&instance_id.to_string()).await?;
        }

        // Clean up pending user tasks in memory
        self.pending_user_tasks.retain(|t| t.instance_id != instance_id);
        
        // Clean up pending external tasks in memory
        self.pending_external_tasks.retain(|t| t.instance_id != instance_id);

        Ok(())
    }

    /// Deletes a process definition. 
    /// If cascade is true, deletes all associated process instances first.
    pub async fn delete_definition(&mut self, definition_key: Uuid, cascade: bool) -> EngineResult<()> {
        if !self.definitions.contains_key(&definition_key) {
            return Err(EngineError::NoSuchDefinition(definition_key));
        }

        // Check for instances
        let associated_instances: Vec<Uuid> = self.instances.values()
            .filter(|i| i.definition_key == definition_key)
            .map(|i| i.id)
            .collect();

        if !associated_instances.is_empty() {
            if !cascade {
                return Err(EngineError::DefinitionHasInstances(associated_instances.len()));
            }
            // Cascade delete instances
            for instance_id in associated_instances {
                self.delete_instance(instance_id).await?;
            }
        }

        self.definitions.remove(&definition_key);

        if let Some(ref persistence) = self.persistence {
            persistence.delete_definition(&definition_key.to_string()).await?;
        }

        Ok(())
    }

    /// Updates variables on a running process instance.
    ///
    /// - Keys with non-null values are created or overwritten.
    /// - Keys with `Value::Null` are removed from the instance variables.
    pub fn update_instance_variables(
        &mut self,
        instance_id: Uuid,
        variables: HashMap<String, Value>,
    ) -> EngineResult<()> {
        let instance = self
            .instances
            .get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;

        let mut added: usize = 0;
        let mut modified: usize = 0;
        let mut deleted: usize = 0;

        for (key, value) in variables {
            if value.is_null() {
                // Delete
                if instance.variables.remove(&key).is_some() {
                    deleted += 1;
                }
            } else {
                match instance.variables.entry(key) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        // Update existing
                        e.insert(value);
                        modified += 1;
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        // Create new
                        e.insert(value);
                        added += 1;
                    }
                }
            }
        }

        instance.audit_log.push(format!(
            "Variables updated: +{added} ~{modified} -{deleted}"
        ));

        log::info!(
            "Instance {}: variables updated (+{added} ~{modified} -{deleted})",
            instance_id
        );

        Ok(())
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
#[path = "tests.rs"]
mod tests;
