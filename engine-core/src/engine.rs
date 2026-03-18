use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::TimeDelta;

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
// External task item (Camunda-style)
// ---------------------------------------------------------------------------

/// An external task that can be fetched and completed by remote workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalTaskItem {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub definition_id: String,
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
// Condition evaluation (gateway support)
// ---------------------------------------------------------------------------

/// Evaluates a simple condition expression against token variables.
///
/// Supported forms:
/// - `"variable == value"` / `"variable != value"`
/// - `"variable > value"` / `"variable < value"` / `"variable >= value"` / `"variable <= value"`
/// - `"variable"` (truthy check: non-null, non-false, non-zero, non-empty-string)
///
/// Returns `false` if the variable is missing or the expression is malformed.
fn evaluate_condition(expr: &str, variables: &HashMap<String, Value>) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return false;
    }

    // Try comparison operators (longest first to avoid prefix conflicts)
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(idx) = expr.find(op) {
            let var_name = expr[..idx].trim();
            let rhs_str = expr[idx + op.len()..].trim();

            let lhs = match variables.get(var_name) {
                Some(v) => v,
                None => return false,
            };

            // Parse RHS as a JSON value for comparison
            let rhs = parse_rhs(rhs_str);

            return match op {
                "==" => values_eq(lhs, &rhs),
                "!=" => !values_eq(lhs, &rhs),
                ">" => values_cmp(lhs, &rhs) == Some(std::cmp::Ordering::Greater),
                "<" => values_cmp(lhs, &rhs) == Some(std::cmp::Ordering::Less),
                ">=" => values_cmp(lhs, &rhs).is_some_and(|o| o != std::cmp::Ordering::Less),
                "<=" => values_cmp(lhs, &rhs).is_some_and(|o| o != std::cmp::Ordering::Greater),
                _ => false,
            };
        }
    }

    // Fallback: truthy check on a single variable name
    match variables.get(expr) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) | None => false,
        // Arrays and objects are truthy
        Some(_) => true,
    }
}

/// Parses a right-hand-side string into a `serde_json::Value`.
fn parse_rhs(s: &str) -> Value {
    // Strip surrounding quotes (single or double) for string comparison
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    // Boolean literals
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    // Null
    if s == "null" {
        return Value::Null;
    }
    // Try number
    if let Ok(n) = s.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(n) {
            return Value::Number(n);
        }
    }
    // Fallback: treat as plain string
    Value::String(s.to_string())
}

/// Equality comparison for JSON values.
fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().zip(b.as_f64()).is_some_and(|(x, y)| (x - y).abs() < f64::EPSILON)
        }
        _ => a == b,
    }
}

/// Ordering comparison for JSON values (numbers only).
fn values_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let fa = a.as_f64()?;
            let fb = b.as_f64()?;
            fa.partial_cmp(&fb)
        }
        _ => None,
    }
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
    pub pending_external_tasks: Vec<ExternalTaskItem>,
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
            pending_external_tasks: Vec::new(),
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

    /// Starts a new process instance with pre-populated variables.
    ///
    /// Like `start_instance`, but the token carries initial variables from the
    /// caller. The instance's `variables` field is also seeded.
    pub async fn start_instance_with_variables(
        &mut self,
        definition_id: &str,
        variables: HashMap<String, Value>,
    ) -> EngineResult<Uuid> {
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
            audit_log: vec![format!(
                "▶ Process started at node '{start_id}' with {} variable(s)",
                variables.len()
            )],
            variables: variables.clone(),
        };

        log::info!(
            "Started instance {instance_id} of '{definition_id}' at node '{start_id}' with {} vars",
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
                    return Ok(());
                }
                NextAction::WaitForExternalTask(ext_task) => {
                    let task_id = ext_task.id;
                    if let Some(inst) = self.instances.get_mut(&instance_id) {
                        inst.state = InstanceState::WaitingOnExternalTask { task_id };
                    }
                    self.pending_external_tasks.push(ext_task);
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
                    .next_nodes(&current_id)
                    .iter()
                    .find(|f| {
                        f.condition
                            .as_ref()
                            .map(|c| evaluate_condition(c, &token.variables))
                            .unwrap_or(true)
                    })
                    .map(|f| f.target.clone())
                    .ok_or_else(|| {
                        EngineError::InvalidDefinition(format!(
                            "No matching outgoing flow from '{current_id}'"
                        ))
                    })?;
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
                    .next_nodes(&current_id)
                    .iter()
                    .find(|f| {
                        f.condition
                            .as_ref()
                            .map(|c| evaluate_condition(c, &token.variables))
                            .unwrap_or(true)
                    })
                    .map(|f| f.target.clone())
                    .ok_or_else(|| {
                        EngineError::InvalidDefinition(format!(
                            "No matching outgoing flow from '{current_id}'"
                        ))
                    })?;
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

            BpmnElement::ExternalTask { topic } => {
                let def_id = instance.definition_id.clone();
                let ext_task = ExternalTaskItem {
                    id: Uuid::new_v4(),
                    instance_id,
                    definition_id: def_id,
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

                let inst = self.instances.get_mut(&instance_id).unwrap();
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

                let inst = self.instances.get_mut(&instance_id).unwrap();
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

                let inst = self.instances.get_mut(&instance_id).unwrap();
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
                    .map(|target| {
                        let mut t = Token::with_variables(&target, token.variables.clone());
                        t.current_node = target;
                        t
                    })
                    .collect();

                if forked.len() == 1 {
                    // Only one match → no need for multi-token handling
                    Ok(NextAction::Continue(forked.into_iter().next().unwrap()))
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
            .next_nodes(&pending.node_id)
            .iter()
            .find(|f| {
                f.condition
                    .as_ref()
                    .map(|c| evaluate_condition(c, &token.variables))
                    .unwrap_or(true)
            })
            .map(|f| f.target.clone())
            .ok_or_else(|| {
                EngineError::InvalidDefinition(format!(
                    "No matching outgoing flow from '{}'",
                    pending.node_id
                ))
            })?;

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

    /// Returns all pending external tasks (for debugging / admin).
    pub fn get_external_tasks(&self) -> &[ExternalTaskItem] {
        &self.pending_external_tasks
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

    // ----- external task operations -----------------------------------------

    /// Fetches and locks external tasks matching the requested topics.
    ///
    /// Returns up to `max_tasks` unlocked tasks whose topic appears in
    /// `topics`. Each returned task is locked for `lock_duration` seconds
    /// and assigned to `worker_id`.
    pub fn fetch_and_lock(
        &mut self,
        worker_id: &str,
        max_tasks: usize,
        topics: &[String],
        lock_duration: i64,
    ) -> Vec<ExternalTaskItem> {
        let now = Utc::now();
        let mut result = Vec::new();

        for task in &mut self.pending_external_tasks {
            if result.len() >= max_tasks {
                break;
            }

            // Skip tasks whose topic is not requested
            if !topics.contains(&task.topic) {
                continue;
            }

            // Skip tasks that are already locked and not expired
            if let Some(expiration) = task.lock_expiration {
                if expiration > now {
                    continue;
                }
                // Lock expired — release it
                log::info!("External task {}: lock expired, releasing", task.id);
            }

            // Lock the task
            task.worker_id = Some(worker_id.to_string());
            task.lock_expiration =
                Some(now + TimeDelta::seconds(lock_duration));

            log::info!(
                "External task {} locked by worker '{}' for {}s",
                task.id, worker_id, lock_duration
            );

            result.push(task.clone());
        }

        result
    }

    /// Completes an external task, advancing the process instance.
    ///
    /// The task must be locked by `worker_id`. Optional variables are merged.
    pub async fn complete_external_task(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        variables: HashMap<String, Value>,
    ) -> EngineResult<()> {
        let idx = self
            .pending_external_tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(EngineError::ExternalTaskNotFound(task_id))?;

        let task = &self.pending_external_tasks[idx];

        // Verify lock ownership
        match &task.worker_id {
            Some(locked_by) if locked_by != worker_id => {
                return Err(EngineError::ExternalTaskLocked {
                    task_id,
                    worker_id: locked_by.clone(),
                });
            }
            None => {
                return Err(EngineError::ExternalTaskNotLocked(task_id));
            }
            _ => {}
        }

        let task = self.pending_external_tasks.remove(idx);
        let instance_id = task.instance_id;

        // Merge variables into the token
        let mut token = task.token;
        for (k, v) in variables {
            token.variables.insert(k, v);
        }

        log::info!(
            "Instance {}: completed external task '{}' (task_id: {task_id})",
            instance_id, task.node_id
        );

        let inst = self
            .instances
            .get_mut(&instance_id)
            .ok_or(EngineError::NoSuchInstance(instance_id))?;
        inst.audit_log.push(format!(
            "✅ External task '{}' completed by worker '{}'",
            task.node_id, worker_id
        ));
        inst.state = InstanceState::Running;
        inst.variables = token.variables.clone();

        // Advance token to the next node
        let def = self
            .definitions
            .get(&inst.definition_id)
            .ok_or_else(|| EngineError::NoSuchDefinition(inst.definition_id.clone()))?;

        let next = def
            .next_nodes(&task.node_id)
            .iter()
            .find(|f| {
                f.condition
                    .as_ref()
                    .map(|c| evaluate_condition(c, &token.variables))
                    .unwrap_or(true)
            })
            .map(|f| f.target.clone())
            .ok_or_else(|| {
                EngineError::InvalidDefinition(format!(
                    "No matching outgoing flow from '{}'",
                    task.node_id
                ))
            })?;

        token.current_node = next;
        if let Some(p) = &self.persistence {
            if let Err(e) = p.save_token(&token).await {
                log::error!("Failed to save token after external task: {}", e);
            }
        }

        self.run_instance(instance_id, token).await
    }

    /// Reports a failure for an external task.
    ///
    /// Decrements retries. When retries reach 0, the task becomes an incident.
    pub fn fail_external_task(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        retries: Option<i32>,
        error_message: Option<String>,
        error_details: Option<String>,
    ) -> EngineResult<()> {
        let task = self
            .pending_external_tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or(EngineError::ExternalTaskNotFound(task_id))?;

        // Verify lock ownership
        match &task.worker_id {
            Some(locked_by) if locked_by != worker_id => {
                return Err(EngineError::ExternalTaskLocked {
                    task_id,
                    worker_id: locked_by.clone(),
                });
            }
            None => {
                return Err(EngineError::ExternalTaskNotLocked(task_id));
            }
            _ => {}
        }

        // Update retries
        let new_retries = retries.unwrap_or(task.retries - 1);
        task.retries = new_retries;
        task.error_message = error_message.clone();
        task.error_details = error_details.clone();

        // Release the lock so it can be retried (or becomes incident)
        task.worker_id = None;
        task.lock_expiration = None;

        if new_retries <= 0 {
            // Incident: log and record on the instance
            let instance_id = task.instance_id;
            if let Some(inst) = self.instances.get_mut(&instance_id) {
                let msg = error_message.unwrap_or_else(|| "Unknown error".into());
                inst.audit_log.push(format!(
                    "🚨 INCIDENT: External task '{}' failed with 0 retries — {}",
                    task.node_id, msg
                ));
            }
            log::warn!(
                "External task {task_id}: incident created (retries exhausted)"
            );
        } else {
            log::info!(
                "External task {task_id}: failed, {} retries remaining",
                new_retries
            );
        }

        Ok(())
    }

    /// Extends the lock on an external task.
    pub fn extend_lock(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        additional_duration: i64,
    ) -> EngineResult<()> {
        let task = self
            .pending_external_tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or(EngineError::ExternalTaskNotFound(task_id))?;

        match &task.worker_id {
            Some(locked_by) if locked_by != worker_id => {
                return Err(EngineError::ExternalTaskLocked {
                    task_id,
                    worker_id: locked_by.clone(),
                });
            }
            None => {
                return Err(EngineError::ExternalTaskNotLocked(task_id));
            }
            _ => {}
        }

        task.lock_expiration =
            Some(Utc::now() + TimeDelta::seconds(additional_duration));

        log::info!(
            "External task {task_id}: lock extended by {additional_duration}s"
        );

        Ok(())
    }

    /// Handles a BPMN error for an external task.
    ///
    /// Simple implementation: logs the error and creates an incident-style
    /// audit entry. The task is removed from the pending queue.
    pub fn handle_bpmn_error(
        &mut self,
        task_id: Uuid,
        worker_id: &str,
        error_code: &str,
    ) -> EngineResult<()> {
        let idx = self
            .pending_external_tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(EngineError::ExternalTaskNotFound(task_id))?;

        let task = &self.pending_external_tasks[idx];

        match &task.worker_id {
            Some(locked_by) if locked_by != worker_id => {
                return Err(EngineError::ExternalTaskLocked {
                    task_id,
                    worker_id: locked_by.clone(),
                });
            }
            None => {
                return Err(EngineError::ExternalTaskNotLocked(task_id));
            }
            _ => {}
        }

        let task = self.pending_external_tasks.remove(idx);
        let instance_id = task.instance_id;

        if let Some(inst) = self.instances.get_mut(&instance_id) {
            inst.audit_log.push(format!(
                "🚨 BPMN error '{}' thrown by worker '{}' at external task '{}'",
                error_code, worker_id, task.node_id
            ));
        }

        log::warn!(
            "External task {task_id}: BPMN error '{error_code}' from worker '{worker_id}'"
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
    async fn conditional_routing_on_service_task() {
        let mut engine = WorkflowEngine::new();
        engine.register_service_handler(
            "noop",
            Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
        );

        let def = ProcessDefinitionBuilder::new("cond_svc")
            .node("start", BpmnElement::StartEvent)
            .node("svc", BpmnElement::ServiceTask("noop".into()))
            .node("end_a", BpmnElement::EndEvent)
            .node("end_b", BpmnElement::EndEvent)
            .flow("start", "svc")
            .conditional_flow("svc", "end_a", "x == 1")
            .conditional_flow("svc", "end_b", "x == 2")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        let mut vars = HashMap::new();
        vars.insert("x".into(), Value::Number(2.into()));
        let inst_id = engine
            .start_instance_with_variables("cond_svc", vars)
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
        let log = engine.get_audit_log(inst_id).unwrap();
        let end_entry = log.iter().find(|l| l.contains("Process completed")).unwrap();
        assert!(end_entry.contains("end_b"), "Expected end_b path: {end_entry}");
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

    // -----------------------------------------------------------------------
    // Condition evaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn condition_eq_number() {
        let mut vars = HashMap::new();
        vars.insert("amount".into(), Value::Number(100.into()));
        assert!(evaluate_condition("amount == 100", &vars));
        assert!(!evaluate_condition("amount == 200", &vars));
    }

    #[test]
    fn condition_neq_string() {
        let mut vars = HashMap::new();
        vars.insert("status".into(), Value::String("approved".into()));
        assert!(evaluate_condition("status == 'approved'", &vars));
        assert!(evaluate_condition("status != 'rejected'", &vars));
        assert!(!evaluate_condition("status == 'rejected'", &vars));
    }

    #[test]
    fn condition_gt_lt() {
        let mut vars = HashMap::new();
        vars.insert("score".into(), Value::Number(75.into()));
        assert!(evaluate_condition("score > 50", &vars));
        assert!(evaluate_condition("score >= 75", &vars));
        assert!(evaluate_condition("score < 100", &vars));
        assert!(evaluate_condition("score <= 75", &vars));
        assert!(!evaluate_condition("score > 75", &vars));
    }

    #[test]
    fn condition_truthy_check() {
        let mut vars = HashMap::new();
        vars.insert("flag".into(), Value::Bool(true));
        vars.insert("zero".into(), Value::Number(0.into()));
        vars.insert("empty".into(), Value::String(String::new()));

        assert!(evaluate_condition("flag", &vars));
        assert!(!evaluate_condition("zero", &vars));
        assert!(!evaluate_condition("empty", &vars));
        assert!(!evaluate_condition("missing_var", &vars));
    }

    #[test]
    fn condition_missing_variable() {
        let vars = HashMap::new();
        assert!(!evaluate_condition("x == 5", &vars));
    }

    // -----------------------------------------------------------------------
    // ExclusiveGateway (XOR) tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn exclusive_gateway_takes_matching_path() {
        let mut engine = WorkflowEngine::new();
        engine.register_service_handler(
            "noop",
            Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
        );

        // Start → XOR Gateway → (amount > 100 → high) / (default → low) → End
        let def = ProcessDefinitionBuilder::new("xor_test")
            .node("start", BpmnElement::StartEvent)
            .node(
                "gw",
                BpmnElement::ExclusiveGateway {
                    default: Some("low".into()),
                },
            )
            .node("high", BpmnElement::ServiceTask("noop".into()))
            .node("low", BpmnElement::ServiceTask("noop".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "high", "amount > 100")
            .flow("gw", "low") // unconditional (default candidate)
            .flow("high", "end")
            .flow("low", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        // amount = 500 → should take the "high" path
        let mut vars = HashMap::new();
        vars.insert("amount".into(), Value::Number(500.into()));
        let inst_id = engine
            .start_instance_with_variables("xor_test", vars)
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
        let log = engine.get_audit_log(inst_id).unwrap();
        let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
        assert!(gw_entry.contains("high"), "Expected high path: {gw_entry}");
    }

    #[tokio::test]
    async fn exclusive_gateway_uses_default_when_no_match() {
        let mut engine = WorkflowEngine::new();
        engine.register_service_handler(
            "noop",
            Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
        );

        let def = ProcessDefinitionBuilder::new("xor_default")
            .node("start", BpmnElement::StartEvent)
            .node(
                "gw",
                BpmnElement::ExclusiveGateway {
                    default: Some("low".into()),
                },
            )
            .node("high", BpmnElement::ServiceTask("noop".into()))
            .node("low", BpmnElement::ServiceTask("noop".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "high", "amount > 100")
            .flow("gw", "low")
            .flow("high", "end")
            .flow("low", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        // amount = 50 → no condition matches → should use default "low"
        let mut vars = HashMap::new();
        vars.insert("amount".into(), Value::Number(50.into()));
        let inst_id = engine
            .start_instance_with_variables("xor_default", vars)
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
        let log = engine.get_audit_log(inst_id).unwrap();
        let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
        assert!(gw_entry.contains("low"), "Expected low (default) path: {gw_entry}");
    }

    #[tokio::test]
    async fn exclusive_gateway_error_when_no_match_no_default() {
        let mut engine = WorkflowEngine::new();

        let def = ProcessDefinitionBuilder::new("xor_fail")
            .node("start", BpmnElement::StartEvent)
            .node(
                "gw",
                BpmnElement::ExclusiveGateway { default: None },
            )
            .node("a", BpmnElement::EndEvent)
            .node("b", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x == 1")
            .conditional_flow("gw", "b", "x == 2")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        // No variables at all → no condition matches → error
        let result = engine.start_instance("xor_fail").await;
        assert!(
            matches!(result, Err(EngineError::NoMatchingCondition(_))),
            "Expected NoMatchingCondition, got: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // InclusiveGateway (OR) tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn inclusive_gateway_forks_multiple_paths() {
        let mut engine = WorkflowEngine::new();
        engine.register_service_handler(
            "track_a",
            Arc::new(|vars: &mut HashMap<String, Value>| {
                vars.insert("path_a".into(), Value::Bool(true));
                Ok(())
            }),
        );
        engine.register_service_handler(
            "track_b",
            Arc::new(|vars: &mut HashMap<String, Value>| {
                vars.insert("path_b".into(), Value::Bool(true));
                Ok(())
            }),
        );

        // Start → Inclusive GW → (a > 0 → svc_a → end) / (b > 0 → svc_b → end)
        let def = ProcessDefinitionBuilder::new("or_test")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("svc_a", BpmnElement::ServiceTask("track_a".into()))
            .node("svc_b", BpmnElement::ServiceTask("track_b".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "svc_a", "a > 0")
            .conditional_flow("gw", "svc_b", "b > 0")
            .flow("svc_a", "end")
            .flow("svc_b", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        // Both conditions true → both paths should fire
        let mut vars = HashMap::new();
        vars.insert("a".into(), Value::Number(10.into()));
        vars.insert("b".into(), Value::Number(20.into()));
        let inst_id = engine
            .start_instance_with_variables("or_test", vars)
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
        let log = engine.get_audit_log(inst_id).unwrap();
        let gw_entry = log.iter().find(|l| l.contains("Inclusive gateway")).unwrap();
        assert!(
            gw_entry.contains("2 path(s)"),
            "Expected 2 forked paths: {gw_entry}"
        );
    }

    #[tokio::test]
    async fn inclusive_gateway_single_match_no_fork() {
        let mut engine = WorkflowEngine::new();
        engine.register_service_handler(
            "noop",
            Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
        );

        let def = ProcessDefinitionBuilder::new("or_single")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("a", BpmnElement::ServiceTask("noop".into()))
            .node("b", BpmnElement::ServiceTask("noop".into()))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x == 1")
            .conditional_flow("gw", "b", "x == 2")
            .flow("a", "end")
            .flow("b", "end")
            .build()
            .unwrap();

        engine.deploy_definition(def);

        // Only x == 1 → single match → Continue (not ContinueMultiple)
        let mut vars = HashMap::new();
        vars.insert("x".into(), Value::Number(1.into()));
        let inst_id = engine
            .start_instance_with_variables("or_single", vars)
            .await
            .unwrap();

        assert_eq!(
            *engine.get_instance_state(inst_id).unwrap(),
            InstanceState::Completed
        );
    }
}
