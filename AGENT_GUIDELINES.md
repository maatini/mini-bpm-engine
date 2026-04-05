# AGENT_GUIDELINES.md

## Welcome to the bpmninja Multi-Agent Project

This project uses a strict **Agent-First Modular Architecture**. Instead of one AI agent trying to understand and build the whole project, we have isolated the project into multiple single-responsibility crates.

### Overview of Agents
Refer to the `.agent/manifest.json` for routing rules:
- **Rust Engine Agent**: Works only in `engine-core/`. Implements the state machine.
- **BPMN Parser Agent**: Works only in `bpmn-parser/`. Implements quick-xml parsing.
- **NATS Persistence Agent**: Works only in `persistence-nats/`.
- **Server Agent**: Works only in `engine-server/`. Implements the Axum REST API.
- **Tauri UI Agent**: Works only in `desktop-tauri/`.
- **Orchestrator Agent**: Works in `agent-orchestrator/` to tie everything together.

### Workflows (use these!)
- `/verify` - Full Rust test, build, and lint check
- `/verify-ui` - TypeScript build check for desktop-tauri
- `/build` - Build workspace
- `/test` - Run all tests
- `/lint` - Run clippy correctly
- `/dev-tauri` - Start the frontend dev server

### Model Guidelines
- The architecture and conventions are designed to work seamlessly with both Claude Opus 4.6 and Gemini 3.1 Pro via Google Antigravity.
- Agents MUST: Read `.agent/rules/` before work, use the `/verify` workflow after Rust changes, `/verify-ui` after frontend changes.

### General Rules
- Read `.agent/rules/PROJECT_CONTEXT.md` and `.agent/rules/AGENT_CONVENTIONS.md` before starting work.
- DO NOT edit code outside of your designated crate.
- Respect Rust Traits/Interfaces built by other agents.
