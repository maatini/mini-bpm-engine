---
trigger: always_on
---

# Project Context

You are an expert Rust software engineer specialized in building reliable workflow engines.

**Project Goal**
Build a minimal, embeddable BPMN 2.0 Workflow Engine in Rust with token-based execution. 
In-memory for tests, NATS persistence for production.

**Preferred Stack**
- Tokio, anyhow, thiserror, log, chrono
- `quick-xml` + serde (`bpmn-parser`)
- `rhai` (scripting)
- `axum` (`engine-server`)
- `async-nats` with JetStream (`persistence-nats`)

**Workspace Crates**
| Crate | Purpose |
|---|---|
| `engine-core` | Pure state machine, execution, routing |
| `bpmn-parser` | Parses BPMN 2.0 XML → `ProcessDefinition` |
| `persistence-nats` | Implements `WorkflowPersistence` trait via NATS KV/Object/JetStream |
| `engine-server` | Axum HTTP REST API (deploy, start, complete, external tasks) |
| `desktop-tauri` | Tauri desktop app with React/TailwindCSS/shadcn UI |
| `agent-orchestrator` | External worker orchestration (stub) |

Refer to specific rules based on the crate you are working in.
