---
name: bpmninja-server
description: Skill for the engine-server, persistence-nats, and agent-orchestrator crates covering REST API, NATS persistence, and external task workers. Implements EvoSkills co-evolutionary verification (arXiv 2604.01687).
version: 2.0.0
tags: [rust, axum, nats, jetstream, rest-api, external-tasks, persistence, evoskills]
requires: [cargo]
---

# BPMNinja Server Skill

## When to Activate
Activate whenever you work on any of these crates:
- `engine-server` – Axum REST API endpoints
- `persistence-nats` – NATS JetStream persistence implementation
- `agent-orchestrator` – External task worker orchestration

## Scope & File Map

### engine-server
```
engine-server/src/
├── lib.rs                 # Public API
├── main.rs                # Server bootstrap, Axum router setup
└── server/
    ├── mod.rs             # Route registration, AppState
    ├── deploy.rs          # POST /deploy – process deployment
    ├── files.rs           # File-related endpoints
    ├── history.rs         # GET /history – audit trail
    ├── instances.rs       # GET/POST /instances – lifecycle
    ├── messages.rs        # POST /messages – message correlation
    ├── monitoring.rs      # GET /health, /metrics
    ├── state.rs           # AppState, shared engine reference
    ├── tasks.rs           # External task API (fetch, complete, handle-error)
    └── timers.rs          # Timer-related endpoints
```

### persistence-nats
```
persistence-nats/src/
├── lib.rs                 # Public API, re-exports
├── client.rs              # NATS connection management
├── models.rs              # Serialization models for NATS
├── trait_impl.rs          # WorkflowPersistence trait implementation
└── tests.rs               # Integration tests (require NATS server)
```

### agent-orchestrator
```
agent-orchestrator/src/
└── main.rs                # Worker binary (polling, heartbeat, retry)
```

## Domain Rules & Patterns

### engine-server
1. **Axum Framework**: All HTTP endpoints use Axum handlers with shared `AppState`.
2. **Error Responses**: Return structured JSON errors with appropriate HTTP status codes.
3. **External Task API**: Must be compatible with Camunda External Task patterns (fetch-and-lock, complete, handle-failure).
4. **No Engine Logic**: The server is a thin adapter. Business logic belongs in engine-core.

### persistence-nats
1. **Trait-Only Interface**: Implements `WorkflowPersistence` from `engine-core::port`. Never import concrete engine types directly.
2. **JetStream**: Use NATS JetStream for durable storage (KV stores, object store).
3. **Crash Recovery**: All state mutations must be recoverable after process restart.
4. **Serialization**: Use serde JSON for NATS message payloads. Backward-compatible schema evolution.

### agent-orchestrator
1. **Polling Pattern**: Workers poll for available tasks with configurable intervals.
2. **Exponential Backoff**: Failed tasks use exponential retry with jitter.
3. **Heartbeat**: Long-running tasks must send periodic heartbeats to prevent timeout.

## Co-Evolutionary Verification (EvoSkills, arXiv 2604.01687)

Every change MUST go through this loop before commit:

### Step 1 – Generate
Use the Graphify MCP Tools first to analyze the relevant Graph Communities (e.g., 3, 12, 20 for server, 6, 9 for persistence). Only after understanding the graph boundaries, read the specific source files. Produce diff-ready Rust changes.

### Step 2 – Surrogate Verification (Self-Critique)
Evaluate changes (score 0–10 each, **all must be ≥ 7**):

| # | Criterion | Question |
|---|---|---|
| 1 | API Safety | Are all endpoints properly validated? No path traversal, injection, or panic on bad input? |
| 2 | Trait Compliance | Does persistence-nats implement WorkflowPersistence correctly without importing concrete engine types? |
| 3 | Error Handling | Are NATS connection failures and timeouts handled gracefully? No unwrap on network I/O? |
| 4 | Camunda Compat | Do external task endpoints match Camunda's fetch-and-lock, complete, handle-failure patterns? |
| 5 | Recovery | Can the system recover from a NATS restart without data loss? Are state mutations journaled? |
| 6 | Test Coverage | Are new endpoints covered by integration tests? Are NATS tests properly conditional on server availability? |

If ANY criterion scores < 7 → return to Step 1 with actionable diagnostic.

### Step 3 – External Oracle
Run `scripts/oracle.sh`. Returns only **PASS** or **FAIL + exit code**.

### Step 4 – Evolution Decision
- **Surrogate FAIL** → Fix and retry (max 15 retries)
- **Surrogate PASS, Oracle FAIL** → Escalate surrogate criteria, retry
- **Oracle PASS** → Commit. Update Evolution Log.

## Common Pitfalls
- Adding business logic to the server instead of engine-core
- Importing concrete engine types in persistence-nats (use traits)
- NATS integration tests that fail without a running NATS server (guard with `#[cfg]` or skip)
- Unbounded polling without backoff in agent-orchestrator
- Missing CORS headers or security middleware

## Evolution Log
| Date | Change | Surrogate Rounds | Oracle Result | Notes |
|---|---|---|---|---|
