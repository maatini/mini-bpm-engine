---
trigger: file_match
file_patterns: ["desktop-tauri/src-tauri/**"]
---

# UI/Desktop Agent (Tauri Rust Backend)
- **Domain:** `desktop-tauri/src-tauri/` (Rust backend for Tauri)
- **Role:** Bridge between the React frontend and `engine-core`. Exposes Tauri Commands.

## Key File: `src-tauri/src/main.rs`
Contains all `#[tauri::command]` handlers and the `AppState` struct.

## AppState
```rust
struct AppState {
    engine: Arc<WorkflowEngine>,
}
```
- Engine uses internal DashMap-based concurrency — no external Mutex needed
- Engine is initialized on startup with optional NATS auto-connect
- Graceful fallback to in-memory if NATS is unavailable
- Full state restore from NATS KV/Object Store on successful connection

## Tauri Commands (all async, all take `State<AppState>`)
| Command | Maps to Engine Method |
|---|---|
| `deploy_definition` | `deploy_definition()` + `save_bpmn_xml()` |
| `start_instance` | `start_instance()` |
| `list_instances` | `list_instances()` |
| `get_instance_details` | `get_instance()` |
| `update_instance_variables` | `update_instance_variables()` |
| `delete_instance` | `delete_instance()` |
| `get_pending_tasks` | `get_pending_user_tasks()` |
| `complete_task` | `complete_user_task()` |
| `list_definitions` | `list_definitions()` |
| `get_definition_xml` | persistence `load_bpmn_xml()` |
| `delete_definition` | `delete_definition()` |
| `get_backend_info` | persistence `get_storage_info()` |
| `switch_backend` | Reconnect to NATS or switch to in-memory |
| `get_monitoring_data` | `get_stats()` + optional `get_nats_info()` |
| `get_instance_history` | persistence `query_history()` |
| `get_pending_service_tasks` | `get_pending_service_tasks()` |
| `complete_service_task` | `complete_service_task()` |

## Rules
- All commands use `&WorkflowEngine` directly (internal DashMap-based concurrency, no mutex locking)
- Error handling: Return `Result<T, String>` — Tauri serializes the error string to the frontend
- Keep logic minimal — delegate to `engine-core` methods
- When adding a new command, also add a wrapper in `src/lib/tauri.ts`
