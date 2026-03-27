---
name: nats-persistence
description: Expert skill for implementing NATS JetStream-based persistence for the mini-bpm workflow engine using KV stores, Object Store, and event streaming.
version: 2.0
triggers: ["nats", "persistence", "jetstream", "kv store", "object store"]
author: Maatini
tags: [rust, nats, jetstream, persistence, async]
---

# NATS PERSISTENCE SKILL

## Overview
Implements the `WorkflowPersistence` trait from `engine-core` using NATS JetStream KV stores, Object Store, and event streaming.

## Crate: `persistence-nats`
- **Dependency:** `async-nats = { version = "0.38", features = ["jetstream"] }`
- **Connection:** `nats://localhost:4222`
- **Main struct:** `NatsPersistence` implementing `WorkflowPersistence` trait

## NATS Feature Split (fixed, NEVER change)

| Feature | Bucket/Stream | Key Format | Content |
|---|---|---|---|
| **Object Store** | `bpmn_xml` | Definition key (UUID) | Original BPMN 2.0 XML (immutable) |
| **KV Store** | `definitions` | Definition key (UUID) | `ProcessDefinition` (JSON) |
| **KV Store** | `instances` | Instance ID (UUID) | `ProcessInstance` (JSON) |
| **KV Store** | `user_tasks` | Task ID (UUID) | `PendingUserTask` (JSON) |
| **KV Store** | `service_tasks` | Task ID (UUID) | `PendingServiceTask` (JSON) |
| **JetStream** | `WORKFLOW_EVENTS` | Subjects: `workflow.*` | Audit events |

## Key Implementation Pattern

```rust
pub struct NatsPersistence {
    definitions: async_nats::jetstream::kv::Store,
    instances: async_nats::jetstream::kv::Store,
    user_tasks: async_nats::jetstream::kv::Store,
    service_tasks: async_nats::jetstream::kv::Store,
    bpmn_xml: async_nats::jetstream::object_store::ObjectStore,
    jetstream: async_nats::jetstream::Context,
}

impl NatsPersistence {
    pub async fn connect(url: &str, stream_name: &str) -> EngineResult<Self> { /* ... */ }
    pub async fn save_bpmn_xml(&self, key: &str, xml: &str) -> EngineResult<()> { /* ... */ }
    pub async fn load_bpmn_xml(&self, key: &str) -> EngineResult<String> { /* ... */ }
    pub async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>> { /* ... */ }
    pub async fn get_nats_info(&self) -> EngineResult<NatsInfo> { /* ... */ }
}
```

## Important Principles
- On every write → save to KV + publish event to JetStream
- On engine start → full state restore from KV + Object Store
- BPMN XML always in Object Store (large artifacts), metadata in KV
- Serialization: `serde_json` for all KV values
- Error handling: wrap NATS errors in `EngineError::PersistenceError`
- No blocking the Tokio runtime — always `.await`
- Tests must pass with in-memory fallback (no NATS requirement)