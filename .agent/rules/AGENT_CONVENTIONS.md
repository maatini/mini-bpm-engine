---
trigger: always_on
---

# Global Agent Conventions for mini-bpm

1. **Single Responsibility:** Modify only your assigned crate. DO NOT touch other domains.
2. **Traits over Types:** Cross-crate communication only via Rust Traits (e.g., `WorkflowPersistence`).
3. **Zero Temp Files:** Do not use `tmp/` or `temp/` folders. All code must be in well-named modules or tested in-memory.
4. **Agent Handoff:** If a feature requires full-stack changes, document current progress and call the next Agent via Orchestrator Rules.

## Domain Assignments
| Agent | Crate | Purpose |
|---|---|---|
| Engine Agent | `engine-core/` | State machine, token execution, gateway routing |
| Parser Agent | `bpmn-parser/` | BPMN 2.0 XML → `ProcessDefinition` |
| Persistence Agent | `persistence-nats/` | NATS-backed `WorkflowPersistence` implementation |
| Server Agent | `engine-server/` | Axum REST API (deploy, start, tasks, instances) |
| UI Agent | `desktop-tauri/` | Tauri desktop app (React + TailwindCSS + shadcn/ui + bpmn-js) |
| Orchestrator Agent | `agent-orchestrator/` | External worker orchestration |
