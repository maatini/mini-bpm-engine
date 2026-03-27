---
trigger: always_on
---

# CLAUDE.md - Agent Instructions for this Project

You are an expert Rust software engineer specialized in building reliable workflow engines.

**Core Rules (MUST follow always):**
- Think step-by-step before writing code.
- Always prioritize compile-time safety and idiomatic Rust.
- Never use unwrap(), expect(), or panic! in library code (only in main or tests when acceptable).
- Use proper error handling with thiserror + anyhow.
- Run `cargo clippy --all-targets --all-features -- -D warnings` after every major change.
- Write comprehensive unit + integration tests.
- Keep the architecture clean and modular.

**Project Goal**
Build a minimal, embeddable BPMN 2.0 Workflow Engine in Rust with:
- StartEvent, TimerStartEvent
- EndEvent
- ServiceTask (Camunda-style external task: fetch-and-lock pattern)
- UserTask
- ExclusiveGateway (XOR — condition-based routing with optional default flow)
- InclusiveGateway (OR — token forking for all matching conditions)
- Conditional Sequence Flows (condition expressions on edges)
- Execution Listeners (Rhai scripts on start/end of nodes)

Use token-based execution. In-memory for tests, NATS persistence for production.

**Preferred Stack**
- Tokio for async and timers
- anyhow + thiserror for error handling
- `log` crate for logging (not tracing)
- `quick-xml` + serde for BPMN XML parsing (`bpmn-parser` crate)
- `rhai` for embedded scripting (execution listeners)
- `chrono` for timestamps
- `axum` for REST API (`engine-server` crate)
- `async-nats` with JetStream for persistence (`persistence-nats` crate)

**Workspace Crates**
| Crate | Purpose |
|---|---|
| `engine-core` | Pure state machine, token execution, gateway routing, condition evaluator, script runner |
| `bpmn-parser` | Parses BPMN 2.0 XML → `ProcessDefinition` |
| `persistence-nats` | Implements `WorkflowPersistence` trait via NATS KV/Object/JetStream |
| `engine-server` | Axum HTTP REST API (deploy, start, complete, external tasks) |
| `desktop-tauri` | Tauri desktop app with React/shadcn UI |
| `agent-orchestrator` | External worker orchestration (stub) |

Follow the rules in RUST_AGENT_RULES.md and BPMN_WORKFLOW_ENGINE.md strictly.
