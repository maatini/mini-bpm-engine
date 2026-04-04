---
name: engine-server
description: Skill for the engine-server crate — building the Axum REST API adapter for the workflow engine.
version: 3.0
triggers: ["server", "api", "rest", "axum", "http", "engine-server"]
author: Antigravity
tags: [rust, axum, rest-api]
---

# ENGINE SERVER SKILL
## Crate: `engine-server`

Axum-based HTTP REST API adapter. All business logic lives in `engine-core`; this crate is purely the HTTP layer.

## REST API Endpoints

### Definitions
- `POST /api/deploy` — Deploy a BPMN definition (XML body)
- `GET /api/definitions` — List all deployed definitions (with version, node_count, is_latest)
- `GET /api/definitions/:id/xml` — Get original BPMN XML for a definition
- `DELETE /api/definitions/:id` — Delete a definition (`?cascade=true` to also delete instances)

### Instances
- `POST /api/start` — Start a new process instance (by definition key)
- `POST /api/start/latest` — Start an instance using the latest version of a bpmn_id
- `GET /api/instances` — List all process instances
- `GET /api/instances/:id` — Get details of a single instance
- `PUT /api/instances/:id/variables` — Update instance variables at runtime
- `DELETE /api/instances/:id` — Delete a process instance

### File Variables
- `POST /api/instances/:id/files/:var_name` — Upload a file variable (multipart)
- `GET /api/instances/:id/files/:var_name` — Download a file variable
- `DELETE /api/instances/:id/files/:var_name` — Delete a file variable

### User Tasks
- `GET /api/tasks` — List all pending user tasks
- `POST /api/complete/:id` — Complete a user task

### Service Tasks (Camunda-style external tasks)
- `GET /api/service-tasks` — List all pending service tasks
- `POST /api/service-task/fetchAndLock` — Fetch and lock tasks for a worker (supports long-polling)
- `POST /api/service-task/:id/complete` — Complete a service task with result variables
- `POST /api/service-task/:id/failure` — Report task failure (with retries)
- `POST /api/service-task/:id/extendLock` — Extend lock duration
- `POST /api/service-task/:id/bpmnError` — Report a BPMN error

### Messages & Timers
- `POST /api/message` — Correlate a message (message_name, optional business_key, variables)
- `POST /api/timers/process` — Trigger expired timers manually

### History
- `GET /api/instances/:id/history` — Query instance history (filterable by event_types, actor_types, node_id, date range)
- `GET /api/instances/:id/history/:event_id` — Get a single history entry

### Monitoring & Health
- `GET /api/info` — Backend info (type, NATS URL, connected)
- `GET /api/monitoring` — Engine stats (definitions, instances, tasks, timers, messages, storage)
- `GET /api/health` — Simple health check (always 200)
- `GET /api/ready` — Readiness check (verifies NATS connectivity)

## Error Handling
- `AppError` enum maps `EngineError` variants to HTTP status codes:
  - `400` — Invalid input (bad XML, invalid variables)
  - `404` — `NoSuchInstance`, `NoSuchDefinition`, `ServiceTaskNotFound`, `NoSuchNode`
  - `409` — `TaskNotPending`, `ServiceTaskLocked`, `ServiceTaskNotLocked`, `AlreadyCompleted`, `DefinitionHasInstances`
  - `500` — Internal / persistence errors

## Rules
- Keep business logic in `engine-core`. The server is an adapter only.
- Map `EngineError` to appropriate HTTP status codes via `AppError`.
- Use `serde_json` for request/response serialization.
- All handlers are async and use `State<Arc<AppState>>` with `RwLock<WorkflowEngine>`.
- Max body size: 5MB (configured via `DefaultBodyLimit`).
- CORS: Allow all origins, methods, and headers.
- File uploads use `Multipart` extractor.
