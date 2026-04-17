---
trigger: file_match
file_patterns: ["engine-core/**"]
---

# BPMN_WORKFLOW_ENGINE.md - Project Specification

## Supported BPMN Elements (23 variants)
- **StartEvent** — plain start, process begins immediately
- **TimerStartEvent(TimerDefinition)** — timer-triggered, fires after configured duration/date/cycle
- **MessageStartEvent { message_name }** — start event triggered by a named message
- **EndEvent** — terminal node, marks instance as completed
- **TerminateEndEvent** — immediately kills all active tokens in the instance
- **ErrorEndEvent { error_code }** — throws a BPMN error on completion
- **ServiceTask { topic, multi_instance }** — Camunda-style external task; creates a `PendingServiceTask` that remote workers fetch-and-lock, then complete or fail
- **UserTask { assignee, multi_instance }** — creates a `PendingUserTask` assigned to a user/role, waits for external `complete_user_task()` call
- **ScriptTask { script, multi_instance }** — executes a Rhai script inline and automatically advances the token
- **SendTask { message_name, multi_instance }** — publishes a named message and automatically advances
- **ExclusiveGateway { default }** — XOR split; first matching condition wins, optional default flow
- **InclusiveGateway** — OR split; all matching conditions fire (token forking via `ContinueMultiple`)
- **ParallelGateway** — AND split/join; all outgoing paths taken unconditionally, join waits for ALL incoming tokens
- **ComplexGateway** — Custom joining logic
- **EventBasedGateway** — execution pauses until exactly one of the target catch events is triggered (only MessageCatchEvent/TimerCatchEvent targets)
- **TimerCatchEvent(TimerDefinition)** — intermediate timer catch; pauses token until duration elapses
- **MessageCatchEvent { message_name }** — intermediate catch event waiting for a message
- **BoundaryTimerEvent { attached_to, timer, cancel_activity }** — boundary timer attached to an activity
- **BoundaryMessageEvent { attached_to, message_name, cancel_activity }** — boundary message catching
- **BoundaryErrorEvent { attached_to, error_code }** — boundary error event attached to an activity
- **CallActivity { called_element }** — invokes another process definition as a sub-process
- **EmbeddedSubProcess { start_node_id }** — embedded sub-process with flattened definition
- **SubProcessEndEvent { sub_process_id }** — internal end event for scope completion

### Additional Concepts
- **Conditional Sequence Flows** — edges carry optional condition expressions (e.g. `amount > 100`). Evaluated by `condition.rs`.
- **Execution Listeners** — Rhai scripts attached to nodes (start/end events). Mutate token variables. Evaluated by `script_runner.rs`.
- **Scope Event Listeners** — Timer/Message/Error event sub-process triggers (`ScopeEventListener` enum).

## Architecture (must follow)

### 1. Model Layer (`model.rs`)
- `BpmnElement` enum — all 21 variants above
- `SequenceFlow { target, condition }` — directed edge with optional condition
- `Token { id: Uuid, current_node, variables: HashMap<String, Value>, is_merged }`
- `ProcessDefinition { key, id, version, nodes, flows, listeners, event_listeners, sub_processes }` — validated at construction time
- `ProcessDefinitionBuilder` — fluent builder with `.node()`, `.flow()`, `.conditional_flow()`, `.listener()`, `.scope_event()`, `.sub_process()`
- `ExecutionListener { event: ListenerEvent, script: String }`
- `ScopeEventListener` — Timer/Message/Error variants for event sub-processes
- `TimerDefinition` — Duration, AbsoluteDate, CronCycle, RepeatingInterval
- `FileReference` — typed wrapper for file variable attachments

### 2. Engine Core (`engine/` submodule)
- `engine/mod.rs` — `WorkflowEngine` public API, `deploy_definition()`, `start_instance()`, message correlation
- `engine/types.rs` — `ProcessInstance`, `InstanceState`, `NextAction`, `PendingUserTask`, `PendingTimer`, `PendingMessageCatch`, `ActiveToken`
- `engine/executor.rs` — Coordinates parallel dispatch (`advance_token()`)
- `engine/handlers/*.rs` — Distinct handlers for Events, Tasks, Gateways, and SubProcesses. Extracted from `executor.rs` for modularity.
- `engine/gateway.rs` — XOR/OR/AND gateway routing and join synchronization via `TokenRegistry`
- `engine/registry.rs` — `DefinitionRegistry` implemented via lock-free `DashMap` concurrency.
- `engine/instance_store.rs` — Instance query and storage helpers
- `engine/boundary.rs` — Boundary event processing (timers, errors)
- `engine/service_task.rs` — External task operations (fetch-and-lock, complete, fail, extend lock, BPMN error)
- `engine/tests.rs` — Comprehensive integration tests
- `engine/stress_tests.rs` — Concurrency and load stress tests
- `engine/definition_ops.rs`, `engine/instance_ops.rs`, `engine/message_processor.rs`, `engine/persistence_ops.rs`, `engine/process_start.rs`, `engine/retry_queue.rs`, `engine/timer_processor.rs`, `engine/user_task.rs` — Workflow state mutations

**`InstanceState` enum (10 variants):**
- `Running` — token(s) actively advancing
- `WaitingOnUserTask { task_id }` — paused for human input
- `WaitingOnServiceTask { task_id }` — paused for external worker
- `WaitingOnTimer { timer_id }` — paused for timer expiration
- `WaitingOnMessage { message_id }` — paused for incoming message
- `ParallelExecution { active_token_count }` — multiple tokens active (parallel gateway)
- `WaitingOnCallActivity { sub_instance_id, token }` — sub-process invocation
- `Completed`
- `CompletedWithError { error_code }`
- `WaitingOnEventBasedGateway`

**`NextAction` enum (14 variants):**
- `Continue(Token)` — advance token to next node
- `ContinueMultiple(Vec<Token>)` — fork into multiple tokens (inclusive gateway, multi-instance)
- `WaitForUser(PendingUserTask)` — pause for human input
- `WaitForServiceTask(PendingServiceTask)` — pause for external worker
- `WaitForJoin { gateway_id, token }` — wait at gateway join point
- `WaitForTimer(PendingTimer)` — pause for timer expiration
- `WaitForMessage(PendingMessageCatch)` — pause for incoming message
- `WaitForEventGroup` — pause at event-based gateway
- `WaitForCallActivity` — pause for sub-process completion
- `MultiInstanceFork` — fork for multi-instance execution
- `MultiInstanceNext` — advance to next multi-instance iteration
- `Complete` — instance finished successfully
- `ErrorEnd { error_code }` — instance finished with BPMN error
- `Terminate` — kill all active tokens immediately

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
