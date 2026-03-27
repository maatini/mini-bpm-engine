---
name: engine-core
description: Skill for the engine-core crate — token-based BPMN execution, gateway routing, condition evaluation, and Rhai script execution.
version: 1.0
triggers: ["engine", "token", "gateway", "condition", "script", "execute_step"]
author: Maatini
tags: [rust, bpmn, state-machine, execution]
---

# ENGINE CORE SKILL

## Crate: `engine-core`
Pure state machine — no network code, no NATS, no HTTP.

## Module Structure
| File | Purpose |
|---|---|
| `model.rs` | `BpmnElement`, `Token`, `ProcessDefinition`, `ProcessDefinitionBuilder`, `SequenceFlow`, `ExecutionListener` |
| `engine.rs` | `WorkflowEngine`, `ProcessInstance`, `InstanceState`, `NextAction`, `PendingUserTask`, `PendingServiceTask` |
| `service_task.rs` | External task ops (fetch-and-lock, complete, fail, extend lock, BPMN error) |
| `condition.rs` | `evaluate_condition()` — condition evaluator for gateway routing |
| `script_runner.rs` | Rhai execution listeners (start/end scripts that mutate token variables) |
| `persistence.rs` | `WorkflowPersistence` trait definition (async interface) |
| `error.rs` | `EngineError` enum, `EngineResult<T>` alias |
| `tests.rs` | Comprehensive integration tests |
| `lib.rs` | Public re-exports |

## Execution Flow
1. `deploy_definition()` — validates and stores `ProcessDefinition`
2. `start_instance()` / `start_instance_with_variables()` — creates `ProcessInstance`, starts token loop
3. `run_instance()` — loops `execute_step()` until wait-state or end:
   - `NextAction::Continue(token)` → advance token
   - `NextAction::ContinueMultiple(tokens)` → fork (InclusiveGateway)
   - `NextAction::WaitForUser(task)` → pause, store pending task
   - `NextAction::WaitForServiceTask(task)` → pause, store external task
   - `NextAction::Complete` → mark instance completed
4. `complete_user_task()` / `complete_service_task()` — resume execution

## Key Design Decisions
- `Arc<ProcessDefinition>` for shared definitions (cheap clone)
- `WorkflowEngine` holds all state as `HashMap`s (not Arc<Mutex>)
- Persistence is optional: `Option<Arc<dyn WorkflowPersistence>>`
- Scripts run via `rhai::Engine` embedded in `WorkflowEngine`
- Condition evaluator is a pure function, no state
