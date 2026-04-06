---
name: engine-core
description: Skill for the engine-core crate — token-based BPMN execution, gateway routing, condition evaluation, and Rhai script execution.
version: 3.0
triggers: ["engine", "token", "gateway", "condition", "script", "execute_step", "bpmn", "workflow engine"]
author: Antigravity
tags: [rust, bpmn, state-machine, execution]
---

# ENGINE CORE SKILL

## Crate: `engine-core`
Pure state machine — no network code, no NATS, no HTTP. Built with Tokio.

## Module Structure
| File | Purpose |
|---|---|
| `model.rs` | `BpmnElement` (21 variants), `Token`, `ProcessDefinition`, `SequenceFlow`, `ExecutionListener`, `ScopeEventListener` |
| `engine/mod.rs` | `WorkflowEngine` public API, `deploy_definition()`, `start_instance()`, message correlation |
| `engine/types.rs` | `ProcessInstance`, `InstanceState` (10 variants), `NextAction` (14 variants), `PendingUserTask`, `PendingTimer`, `PendingMessageCatch`, `ActiveToken` |
| `engine/executor.rs` | `execute_step()`, `advance_token()` — dispatches on `BpmnElement` |
| `engine/gateway.rs` | XOR/OR/AND/EventBased gateway routing and join synchronization |
| `engine/registry.rs` | `TokenRegistry` for parallel/inclusive gateway join tracking |
| `engine/instance_store.rs` | Instance query and storage helpers (DashMap-based) |
| `engine/boundary.rs` | Boundary event processing (timers, errors) |
| `engine/service_task.rs` | External task ops (fetch-and-lock, complete, fail, BPMN error) |
| `engine/tests.rs` | Comprehensive integration tests |
| `engine/stress_tests.rs
| `engine/definition_ops.rs` | Ops |
| `engine/instance_ops.rs` | Ops |
| `engine/message_processor.rs` | Ops |
| `engine/persistence_ops.rs` | Ops |
| `engine/process_start.rs` | Ops |
| `engine/retry_queue.rs` | Ops |
| `engine/timer_processor.rs` | Ops |
| `engine/user_task.rs` | Ops |` | Concurrency and load stress tests |
| `condition.rs` | `evaluate_condition()` — condition evaluator for gateway routing |
| `script_runner.rs` | Rhai execution listeners (start/end scripts) |
| `persistence.rs` | `WorkflowPersistence` trait definition |
| `persistence_in_memory.rs` | In-memory persistence for tests |
| `history.rs` | `HistoryEntry`, `HistoryEventType`, `calculate_diff()` |
| `timer_definition.rs` | `TimerDefinition` enum (Duration, AbsoluteDate, CronCycle, RepeatingInterval) |
| `error.rs` | `EngineError` enum (14 variants), `EngineResult<T>` alias |
| `lib.rs` | Public re-exports (including `EngineStats`) |

## Supported BPMN Elements (21 variants)
- **StartEvent** / **TimerStartEvent(TimerDefinition)** / **MessageStartEvent { message_name }**
- **EndEvent** / **TerminateEndEvent** / **ErrorEndEvent { error_code }**
- **ServiceTask { topic, multi_instance }** (Camunda-style fetch-and-lock)
- **UserTask(assignee, multi_instance)**
- **ScriptTask { script, multi_instance }** (inline Rhai execution, auto-advances)
- **SendTask { message_name, multi_instance }** (message throw, auto-advances)
- **ExclusiveGateway { default }** (XOR split)
- **InclusiveGateway** (OR split)
- **ParallelGateway** (AND split/join)
- **EventBasedGateway** (waits for first catch event)
- **TimerCatchEvent(TimerDefinition)** (intermediate timer)
- **BoundaryTimerEvent** / **BoundaryErrorEvent** (boundary events)
- **MessageCatchEvent { message_name }** (intermediate message catch)
- **CallActivity { called_element }** (sub-process invocation)
- **EmbeddedSubProcess** (embedded sub-process)

## Key Design Decisions
- `Arc<ProcessDefinition>` for shared definitions
- DashMap-based instance storage (no global RwLock)
- Token-based execution (`execute_step()` -> `NextAction`)
- Script listeners embedded via `rhai::Engine`
- `thiserror` for `EngineError` (no unwraps in lib code!)
