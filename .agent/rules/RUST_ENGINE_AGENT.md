---
trigger: file_match
file_patterns: ["engine-core/**"]
---

# Execution Engine Agent
- **Domain:** `engine-core/`
- **Role:** Pure state machine — token advancement, `BpmnElement` dispatch, gateway routing, condition evaluation, script execution, timer/message/error boundary events.
- **Key files:**
  - `model.rs` — `BpmnElement` (19 variants), `Token`, `ProcessDefinition`, `ProcessDefinitionBuilder`, `SequenceFlow`, `ExecutionListener`, `ScopeEventListener`
  - `engine/mod.rs` — `WorkflowEngine` public API, deploy, start, message correlation
  - `engine/types.rs` — `ProcessInstance`, `InstanceState` (9 variants), `NextAction` (8 variants), `ActiveToken`, `PendingTimer`, `PendingMessageCatch`
  - `engine/executor.rs` — `execute_step()`, `advance_token()` — main execution loop
  - `engine/gateway.rs` — XOR/OR/AND/EventBased gateway routing and join synchronization
  - `engine/registry.rs` — `TokenRegistry` for parallel/inclusive gateway join tracking
  - `engine/boundary.rs` — Boundary event processing (timers, errors)
  - `engine/service_task.rs` — External task operations (fetch-and-lock, complete, fail, extend lock, BPMN error)
  - `engine/instance_store.rs` — DashMap-based instance query and storage helpers
  - `condition.rs` — `evaluate_condition()` for gateway routing
  - `script_runner.rs` — Rhai execution listeners (start/end scripts)
  - `persistence.rs` — `WorkflowPersistence` trait definition
  - `persistence_in_memory.rs` — In-memory persistence for tests
  - `timer_definition.rs` — `TimerDefinition` enum (Duration, AbsoluteDate, CronCycle, RepeatingInterval)
  - `error.rs` — `EngineError` enum with all error variants
  - `history.rs` — `HistoryEntry`, `HistoryEventType`, `HistoryDiff`, `calculate_diff()`
- **Rules:** No network code (no NATS, no HTTP). Run timers via `tokio::time`. Define traits for everything external (persistence via `WorkflowPersistence`). Refer to `BPMN_WORKFLOW_ENGINE.md` for the full element specification.
