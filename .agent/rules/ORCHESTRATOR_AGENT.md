---
trigger: file_match
file_patterns: ["agent-orchestrator/**"]
---

# Orchestrator Agent
- **Domain:** `agent-orchestrator/`
- **Role:** The glue. Starts Tokio runtimes, connects NATS to the Engine, and binds the Parser. Is currently a stub and does not contain the backend's main (which lives in engine-server).
- **Rules:** Clean architecture. Handle graceful shutdowns and environment configurations.
