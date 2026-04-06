---
trigger: file_match
file_patterns: ["persistence-nats/**"]
---

# NATS Message Broker Rules (BPMNinja Project)

You are an expert in NATS + Rust persistence for workflow engines.
Whenever the user requests NATS integration, persistence, or distributed state storage, follow these rules **exactly**.

## 1. Technology Stack (fixed)
- Crate: `async-nats = { version = "0.38", features = ["jetstream"] }`
- Connection: `nats://localhost:4222` (nats-server runs via devbox/docker-compose)
- Always `tokio::spawn` for background tasks (Watch, Timer, etc.)
- Error handling: extend `EngineError` with `PersistenceError` variant

## 2. NATS Feature Split (NEVER change this!)
| Feature          | Bucket/Stream Name       | Usage in BPMNinja                                   | Why this choice? |
|------------------|--------------------------|-----------------------------------------------------|------------------|
| **Object Store** | `bpmn_xml`              | Original BPMN 2.0 XML (immutable)                  | For large artifacts, chunking, versioning |
| **Object Store** | `instance_files`        | File variable attachments (binary)                  | Binary blobs, separate from KV |
| **KV Store**     | `definitions`           | ProcessDefinition (JSON)                            | Fast reads/writes, watch support |
| **KV Store**     | `instances`             | ProcessInstance + Token + Variables + Audit-Log     | Running process state |
| **KV Store**     | `user_tasks`            | PendingUserTask                                     | Pending tasks for external completion |
| **KV Store**     | `service_tasks`         | PendingServiceTask (external tasks)                 | Camunda-style external task state |
| **KV Store**     | `timers`                | PendingTimer                                        | Timer catch events and boundary timers |
| **KV Store**     | `messages`              | PendingMessageCatch                                 | Message catch events waiting for correlation |
| **JetStream**    | Stream `HISTORY`        | Subjects: `history.instance.*`                      | Per-instance audit trail, queryable |

## 3. Important Principles
- **On every write** → immediately save to KV + publish event to JetStream (atomic).
- **On engine start** → full state restore from KV + Object Store.
- **BPMN 2.0 XML** always in Object Store + metadata in KV `definitions`.
- **In-memory cache** only for tests (feature flag `in-memory`).
- **No breaking changes** to the existing `WorkflowEngine` public API.
- **Minimal & idiomatic**: `Arc<NatsPersistence>`, async everywhere, `serde_json` for serialization.
- **Tests** must continue to pass 100% (in-memory fallback).

## 4. Forbidden
- No Redis, PostgreSQL, or other databases.
- No manual stream management without `jetstream::new(client)`.
- No blocking the Tokio runtime (always `.await`).
- No large binaries in KV Store (→ Object Store!).

These rules override all other rules when NATS is mentioned.
