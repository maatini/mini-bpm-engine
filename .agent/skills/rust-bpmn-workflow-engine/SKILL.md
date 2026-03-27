---
name: rust-bpmn-workflow-engine
description: Expert skill for building a minimal, production-ready BPMN 2.0 Workflow Engine in Rust with StartEvent, TimerStartEvent, EndEvent, ServiceTask and UserTask.
version: 2.0
triggers: ["bpmn", "workflow engine", "token execution", "timer task", "user task", "service task", "gateway"]
author: Maatini
tags: [rust, tokio, bpmn, state-machine, concurrency]
---

# RUST BPMN WORKFLOW ENGINE SKILL

## Core Expertise (always apply)
You are an expert in building safe, idiomatic Rust workflow engines. Use token-based execution, async Tokio, strict compile-time safety, and follow all persistence rules when NATS is requested.

## Required Patterns (use exactly these)

### 1. Model Layer (`engine-core/src/model.rs`)
- Enum `BpmnElement` with 7 variants:
  - `StartEvent` — plain start, process begins immediately
  - `TimerStartEvent(Duration)` — timer-triggered
  - `EndEvent` — terminal node
  - `ServiceTask { topic: String }` — Camunda-style external task (fetch-and-lock)
  - `UserTask(String)` — user task with assignee
  - `ExclusiveGateway { default: Option<String> }` — XOR split, first match wins
  - `InclusiveGateway` — OR split, all matches fork
- Struct `SequenceFlow { target: String, condition: Option<String> }`
- Struct `Token { id: Uuid, current_node: String, variables: HashMap<String, Value> }`
- Struct `ProcessDefinition { key: Uuid, id: String, nodes, flows, listeners }` — validated at construction
- `ProcessDefinitionBuilder` — fluent builder with `.node()`, `.flow()`, `.conditional_flow()`, `.listener()`
- `ExecutionListener { event: ListenerEvent, script: String }` — Rhai scripts

### 2. Engine Core (`engine-core/src/engine.rs`)
- `WorkflowEngine` struct with `definitions`, `instances`, `pending_user_tasks`, `pending_service_tasks`, `persistence`, `script_engine`
- `execute_step()` — dispatches on `BpmnElement`, returns `NextAction`
- `NextAction` enum: `Continue(Token)`, `ContinueMultiple(Vec<Token>)`, `WaitForUser(PendingUserTask)`, `WaitForServiceTask(PendingServiceTask)`, `Complete`
- `ProcessInstance { id, definition_key, business_key, state, current_node, audit_log, variables }`
- `InstanceState` enum: `Running`, `WaitingOnUserTask { task_id }`, `WaitingOnServiceTask { task_id }`, `Completed`

### 3. External Tasks (`engine-core/src/service_task.rs`)
Camunda-style fetch-and-lock pattern:
- `PendingServiceTask` with `worker_id`, `lock_expiration`, `retries`, `error_message`, `error_details`
- `fetch_and_lock_service_tasks(worker_id, max_tasks, topics, lock_duration)` — returns locked tasks
- `complete_service_task(task_id, worker_id, variables)` — completes and advances instance
- `fail_service_task(task_id, worker_id, retries, error_message, error_details)` — decrements retries
- `extend_lock(task_id, worker_id, additional_duration)` — extends lock timeout
- `handle_bpmn_error(task_id, worker_id, error_code)` — reports BPMN error

### 4. Condition Evaluator (`engine-core/src/condition.rs`)
- `evaluate_condition(expr, variables) -> bool`
- Supports: `==`, `!=`, `>`, `>=`, `<`, `<=`, truthy checks
- Parses RHS as JSON value for type-safe comparison

### 5. Script Runner (`engine-core/src/script_runner.rs`)
- Uses embedded `rhai::Engine`
- `run_node_scripts()` — executes start/end scripts, mutates token variables
- `run_end_scripts()` — convenience wrapper that also updates instance variables

### 6. Persistence (`engine-core/src/persistence.rs`)
- `WorkflowPersistence` trait with async methods:
  - `save_token`, `load_tokens`, `save_instance`, `list_instances`, `delete_instance`
  - `save_definition`, `list_definitions`, `delete_definition`
  - `save_user_task`, `delete_user_task`, `list_user_tasks`
  - `save_service_task`, `delete_service_task`, `list_service_tasks`
- Attached via `engine.with_persistence(Arc<dyn WorkflowPersistence>)`

### 7. Error Handling (`engine-core/src/error.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    InvalidDefinition(String),
    NoSuchNode(String),
    NoSuchDefinition(Uuid),
    NoSuchInstance(Uuid),
    TaskNotPending { task_id: Uuid, actual_state: String },
    AlreadyCompleted,
    TimerMismatch { expected: u64, provided: u64 },
    NoMatchingCondition(String),
    ServiceTaskNotFound(Uuid),
    ServiceTaskLocked { task_id: Uuid, worker_id: String },
    ServiceTaskNotLocked(Uuid),
    DefinitionHasInstances(usize),
    PersistenceError(String),
    ScriptError(String),
}
```

## Best Practices (mandatory)
- Rust 2024 edition. In-memory for tests, NATS for production.
- Error Handling: `thiserror` for `EngineError`. Use `?` operator everywhere.
- Logging: `log::info!`, `log::debug!`, `log::warn!`, `log::error!` (NOT tracing).
- Anti-Patterns: NEVER use `.unwrap()` or `.expect()` in production code.
- Unit tests with `#[tokio::test]` for every element. Minimum 85% coverage on core logic.
- Clippy: `cargo clippy --all-targets -- -D warnings` after every change.

## Example: Execute Step Pattern
```rust
pub async fn execute_step(
    &mut self,
    instance_id: Uuid,
    token: &mut Token,
) -> EngineResult<NextAction> {
    let def = /* get definition */;
    let element = def.get_node(&token.current_node)
        .ok_or_else(|| EngineError::NoSuchNode(token.current_node.clone()))?
        .clone();

    // Run start scripts
    script_runner::run_node_scripts(&self.script_engine, instance_id, token, &def, &current_id, ListenerEvent::Start, &mut audits)?;

    match &element {
        BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_) => {
            let next = resolve_next_target(&def, &current_id, &token.variables)?;
            token.current_node = next;
            Ok(NextAction::Continue(token.clone()))
        }
        BpmnElement::EndEvent => Ok(NextAction::Complete),
        BpmnElement::ServiceTask { topic } => {
            // Create PendingServiceTask, return WaitForServiceTask
        }
        BpmnElement::UserTask(assignee) => {
            // Create PendingUserTask, return WaitForUser
        }
        BpmnElement::ExclusiveGateway { default } => {
            // Evaluate conditions, first match wins, fallback to default
        }
        BpmnElement::InclusiveGateway => {
            // Evaluate all conditions, fork tokens for matches
        }
    }
}
```
