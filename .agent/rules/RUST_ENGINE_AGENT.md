---
trigger: file_match
file_patterns: ["engine-core/**"]
---

# Execution Engine Agent
- **Domain:** `engine-core/`
- **Role:** Pure state machine — token advancement, `BpmnElement` dispatch, gateway routing, condition evaluation, script execution.
- **Key files:**
  - `model.rs` — `BpmnElement`, `Token`, `ProcessDefinition`, `ProcessDefinitionBuilder`, `SequenceFlow`, `ExecutionListener`
  - `engine.rs` — `WorkflowEngine`, `ProcessInstance`, `InstanceState`, `NextAction`, `PendingUserTask`, `PendingServiceTask`
  - `service_task.rs` — External task operations (fetch-and-lock, complete, fail, extend lock, BPMN error)
  - `condition.rs` — `evaluate_condition()` for gateway routing
  - `script_runner.rs` — Rhai execution listeners (start/end scripts)
  - `persistence.rs` — `WorkflowPersistence` trait definition
  - `error.rs` — `EngineError` enum with all error variants
- **Rules:** No network code (no NATS, no HTTP). Run timers via `tokio::time`. Define traits for everything external (persistence via `WorkflowPersistence`).
