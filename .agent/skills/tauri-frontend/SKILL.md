---
name: tauri-frontend
description: Skill for the Tauri desktop application — React + shadcn/ui frontend with bpmn-js modeler and dual-mode backend (embedded/HTTP).
version: 1.0
triggers: ["tauri", "desktop", "ui", "frontend", "react", "shadcn"]
author: Maatini
tags: [tauri, react, typescript, shadcn, bpmn-js]
---

# TAURI FRONTEND SKILL

## Crate: `desktop-tauri`
Tauri v1 desktop application with React + shadcn/ui + bpmn-js.

## Dual Backend Modes
- **Embedded (default):** `WorkflowEngine` runs inside Tauri backend (`main.rs`)
- **HTTP mode:** `--features http-backend` connects to `engine-server` via REST API

## Architecture
```
desktop-tauri/
├── src-tauri/src/main.rs    # Tauri backend (all commands, state management)
├── src/                      # React frontend
│   ├── App.tsx               # Main app with routing
│   ├── components/           # shadcn/ui components
│   └── pages/                # Definitions, Instances, Tasks, Monitoring
├── tests/e2e/                # Playwright E2E tests
└── package.json
```

## Tauri Commands (exposed to frontend)
| Command | Purpose |
|---|---|
| `deploy_definition(xml, name)` | Deploy BPMN XML |
| `deploy_simple_process()` | Deploy hardcoded simple process |
| `start_instance(def_id, variables)` | Start new process instance |
| `list_instances()` | List all process instances |
| `get_instance_details(id)` | Get single instance details |
| `update_instance_variables(id, vars)` | Update instance variables |
| `delete_instance(id)` | Delete a process instance |
| `get_pending_tasks()` | List pending user tasks |
| `complete_task(task_id)` | Complete a user task |
| `list_definitions()` | List deployed definitions |
| `get_definition_xml(id)` | Get original BPMN XML |
| `delete_definition(id, cascade)` | Delete a definition |
| `get_backend_info()` | Get current backend type |
| `switch_backend(type, url)` | Switch between in-memory/NATS |
| `get_monitoring_data()` | Engine + NATS metrics |
| `read_bpmn_file(path)` | Read BPMN file from disk |

## State Management
- `AppState` with `Arc<Mutex<WorkflowEngine>>` for embedded mode
- BPMN XML cached in-memory + NATS Object Store
- NATS auto-connect on startup (graceful fallback to in-memory)
- Full state restore from NATS on startup

## Rules
- Do NOT implement business logic in TypeScript — keep it in Rust
- Use Tauri Commands for all engine interactions
- All `#[cfg(feature = "http-backend")]` variants must mirror embedded commands
