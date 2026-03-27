---
trigger: always_on
---

# BPMN_WORKFLOW_ENGINE.md - Project Specification

## Supported BPMN Elements
- **StartEvent** — plain start, process begins immediately
- **TimerStartEvent(Duration)** — timer-triggered, fires after configured duration
- **EndEvent** — terminal node, marks instance as completed
- **ServiceTask { topic }** — Camunda-style external task; creates a `PendingServiceTask` that remote workers fetch-and-lock, then complete or fail
- **UserTask(assignee)** — creates a `PendingUserTask` assigned to a user/role, waits for external `complete_user_task()` call
- **ExclusiveGateway { default }** — XOR split; first matching condition wins, optional default flow
- **InclusiveGateway** — OR split; all matching conditions fire (token forking via `ContinueMultiple`)

### Additional Concepts
- **Conditional Sequence Flows** — edges carry optional condition expressions (e.g. `amount > 100`). Evaluated by `condition.rs`.
- **Execution Listeners** — Rhai scripts attached to nodes (start/end events). Mutate token variables. Evaluated by `script_runner.rs`.

## Architecture (must follow)

### 1. Model Layer (`model.rs`)
- `BpmnElement` enum — all 7 variants above
- `SequenceFlow { target, condition }` — directed edge with optional condition
- `Token { id: Uuid, current_node, variables: HashMap<String, Value> }`
- `ProcessDefinition { key, id, nodes, flows, listeners }` — validated at construction time
- `ProcessDefinitionBuilder` — fluent builder with `.node()`, `.flow()`, `.conditional_flow()`, `.listener()`
- `ExecutionListener { event: ListenerEvent, script: String }`

### 2. Engine Core (`engine.rs`)
- `WorkflowEngine` struct with `definitions`, `instances`, `pending_user_tasks`, `pending_service_tasks`
- `execute_step()` — dispatches on `BpmnElement`, returns `NextAction`
- `NextAction` enum: `Continue(Token)`, `ContinueMultiple(Vec<Token>)`, `WaitForUser`, `WaitForServiceTask`, `Complete`
- `ProcessInstance { id, definition_key, business_key, state, current_node, audit_log, variables }`
- `InstanceState` enum: `Running`, `WaitingOnUserTask`, `WaitingOnServiceTask`, `Completed`

### 3. External Tasks (`service_task.rs`)
- `PendingServiceTask` — Camunda-style with `worker_id`, `lock_expiration`, `retries`, `error_message`
- `fetch_and_lock_service_tasks(worker_id, max_tasks, topics, lock_duration)`
- `complete_service_task(task_id, worker_id, variables)`
- `fail_service_task(task_id, worker_id, retries, error_message, error_details)`
- `extend_lock(task_id, worker_id, additional_duration)`
- `handle_bpmn_error(task_id, worker_id, error_code)`

### 4. Condition Evaluator (`condition.rs`)
- `evaluate_condition(expr, variables)` — supports `==`, `!=`, `>`, `>=`, `<`, `<=`, truthy checks

### 5. Script Runner (`script_runner.rs`)
- Uses embedded `rhai::Engine` for execution listeners
- `run_node_scripts()` — executes start/end scripts, mutates token variables

### 6. Persistence (`persistence.rs`)
- `WorkflowPersistence` trait — async interface for save/load/delete of instances, definitions, user tasks, service tasks
- Optional via `engine.with_persistence(Arc<dyn WorkflowPersistence>)`

### 7. Error Handling (`error.rs`)
- `EngineError` enum with variants: `InvalidDefinition`, `NoSuchNode`, `NoSuchDefinition`, `NoSuchInstance`, `TaskNotPending`, `TimerMismatch`, `NoMatchingCondition`, `ServiceTaskNotFound`, `ServiceTaskLocked`, `ServiceTaskNotLocked`, `DefinitionHasInstances`, `PersistenceError`, `ScriptError`

Prioritize correctness of token flow, gateway routing, and external task lifecycle.
