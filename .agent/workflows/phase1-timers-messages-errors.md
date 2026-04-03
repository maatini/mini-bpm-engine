---
description: Phase 1 implementation plan for timers, messages, and errors
---

# Phase 1: Timers, Messages, and Errors

This workflow follows the strict execution plan strictly for Phase 1. Each step will be delegated to the responsible agent, ensuring single-responsibility and proper verification at each stage.

1. **Rust Engine Agent**: Extend the model (`BpmnElement`) with new timer, message and error variants + builder methods.
   - Files: `engine-core/src/model.rs`
   - Verification: `/verify`
2. **BPMN Parser Agent**: Extend the parser to support Timer Intermediate, Timer Boundary, Message Events and Error Boundary Events (including bpmn-js compatible XML).
   - Files: `bpmn-parser/src/**`
   - Verification: `/verify`
3. **Rust Engine Agent**: Implement full execution logic for all new events (Timer Queue, Message Correlation, Error Propagation).
   - Files: `engine-core/src/engine.rs`, etc.
   - Verification: `/verify`
4. **Persistence Agent** (NatsPersistence): Add support for new pending task types.
   - Files: `persistence-nats/src/**`
   - Verification: `/verify`, `test:nats`
5. **Server Agent**: Add new REST endpoints (`/api/message`, timer trigger, improved error handling).
   - Files: `engine-server/src/**`
   - Verification: `/verify`
6. **Rust Engine Agent**: Write extensive unit + integration tests (at least 25 new tests).
   - Files: `engine-core/src/**/tests.rs`
   - Verification: `/verify`, `/test`
7. **Tauri UI Agent**: Make the new elements visible and usable in the bpmn-js modeler + diagram highlighting.
   - Files: `desktop-tauri/src/**`
   - Verification: `/verify-ui`
8. **Orchestrator Agent**: Update `README.md` and `example.bpmn` with the new features.
   - Files: `README.md`, `example.bpmn`
9. **Full Verification**: Run `/verify`, `/test`, `/verify-ui`, `cargo test --features nats`.
   - Complete project check to ensure all features are robustly integrated.
