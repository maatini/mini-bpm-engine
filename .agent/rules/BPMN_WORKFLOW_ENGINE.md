---
trigger: file_match
file_patterns: ["engine-core/**"]
---

# BPMN_WORKFLOW_ENGINE.md - Project Specification

## Supported BPMN Elements
- **StartEvent** — plain start, process begins immediately
- **TimerStartEvent(Duration)** — timer-triggered, fires after configured duration
- **EndEvent** — terminal node, marks instance as completed
- **ErrorEndEvent { error_code }** — throws a BPMN error on completion
- **ServiceTask { topic }** — Camunda-style external task; creates a `PendingServiceTask` that remote workers fetch-and-lock, then complete or fail
- **UserTask(assignee)** — creates a `PendingUserTask` assigned to a user/role, waits for external `complete_user_task()` call
- **ExclusiveGateway { default }** — XOR split; first matching condition wins, optional default flow
- **InclusiveGateway** — OR split; all matching conditions fire (token forking via `ContinueMultiple`)
- **ParallelGateway** — AND split/join; all outgoing paths taken unconditionally, join waits for ALL incoming tokens
- **TimerCatchEvent(Duration)** — intermediate timer catch; pauses token until duration elapses
- **BoundaryTimerEvent { attached_to, duration, cancel_activity }** — boundary timer attached to an activity
- **MessageStartEvent { message_name }** — start event triggered by a named message
- **MessageCatchEvent { message_name }** — intermediate catch event waiting for a message
- **BoundaryErrorEvent { attached_to, error_code }** — boundary error event attached to an activity
- **CallActivity { called_element }** — invokes another process definition as a sub-process

### Additional Concepts
- **Conditional Sequence Flows** — edges carry optional condition expressions (e.g. `amount > 100`). Evaluated by `condition.rs`.
- **Execution Listeners** — Rhai scripts attached to nodes (start/end events). Mutate token variables. Evaluated by `script_runner.rs`.

## Architecture (must follow)

### 1. Model Layer (`model.rs`)
- `BpmnElement` enum — all 15 variants above
- `SequenceFlow { target, condition }` — directed edge with optional condition
- `Token { id: Uuid, current_node, variables: HashMap<String, Value> }`
- `ProcessDefinition { key, id, nodes, flows, listeners }` — validated at construction time
- `ProcessDefinitionBuilder` — fluent builder with `.node()`, `.flow()`, `.conditional_flow()`, `.listener()`
- `ExecutionListener { event: ListenerEvent, script: String }`

### 2. Engine Core (`engine/` submodule)
- `engine/mod.rs` — `WorkflowEngine` public API, `deploy_definition()`, `start_instance()`, message correlation
- `engine/types.rs` — `ProcessInstance`, `InstanceState`, `NextAction`, `PendingUserTask`, `PendingTimer`, `PendingMessageCatch`, `ActiveToken`
- `engine/executor.rs` — `execute_step()`, `advance_token()` — dispatches on `BpmnElement`, returns `NextAction`
- `engine/gateway.rs` — XOR/OR/AND gateway routing and join synchronization via `TokenRegistry`
- `engine/registry.rs` — `TokenRegistry` for parallel/inclusive gateway join synchronization
- `engine/instance_store.rs` — Instance query and storage helpers
- `engine/boundary.rs` — Boundary event processing (timers, errors)
- `engine/service_task.rs` — External task operations (fetch-and-lock, complete, fail, extend lock, BPMN error)
- `engine/tests.rs` — Comprehensive integration tests
- `engine/stress_tests.rs` — Concurrency and load stress tests

**`InstanceState` enum:**
- `Running` — token(s) actively advancing
- `WaitingOnUserTask { task_id }` — paused for human input
- `WaitingOnServiceTask { task_id }` — paused for external worker
- `WaitingOnTimer { timer_id }` — paused for timer expiration
- `WaitingOnMessage { message_id }` — paused for incoming message
- `ParallelExecution { active_token_count }` — multiple tokens active (parallel gateway)
- `WaitingOnCallActivity { sub_instance_id, token }` — sub-process invocation
- `Completed`

**`NextAction` enum:**
- `Continue(Token)`, `ContinueMultiple(Vec<Token>)`, `WaitForUser(PendingUserTask)`, `WaitForServiceTask(PendingServiceTask)`, `WaitForJoin { gateway_id, token }`, `WaitForTimer(PendingTimer)`, `WaitForMessage(PendingMessageCatch)`, `Complete`

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
- `WorkflowPersistence` trait — async interface for save/load/delete of instances, definitions, user tasks, service tasks, XML, storage info, and history:
  - `append_history_entry()`, `query_history()`, `HistoryQuery`
  - `save_bpmn_xml()`, `load_bpmn_xml()`, `list_bpmn_xml_ids()`
  - `get_storage_info()`, `StorageInfo`
- Optional via `engine.with_persistence(Arc<dyn WorkflowPersistence>)`

### 7. Error Handling (`error.rs`)
- `EngineError` enum with variants: `InvalidDefinition`, `NoSuchNode`, `NoSuchDefinition`, `NoSuchInstance`, `TaskNotPending`, `InstanceCompleted`, `TimerMismatch`, `NoMatchingCondition`, `ServiceTaskNotFound`, `ServiceTaskLocked`, `ServiceTaskNotLocked`, `DefinitionHasInstances`, `PersistenceError`, `ScriptError`

### 8. History Module (`history.rs`)
- `HistoryEntry` — represents a single event in instance lifecycle
- `HistoryEventType` — enum (`InstanceStarted`, `TaskCompleted`, `VariableUpdated`, etc.)
- `HistoryDiff`, `VariableDiff` — captures state delta
- `ActorType` — enum (`Engine`, `User`, `ServiceWorker`, `Timer`, `Listener`)
- `calculate_diff()` — utility to generate diff between instance states

Prioritize correctness of token flow, gateway routing, and external task lifecycle.
