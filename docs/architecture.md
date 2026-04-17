# bpmninja — Architecture Documentation

> BPMN 2.0 workflow engine in Rust, token-based execution
> As of: 2026-04-10

---

## 1. Workspace Overview

The project is a Cargo workspace with 6 crates, a Tauri desktop app and an API spec:

| Crate | Lib LoC | Test LoC | Purpose |
|---|---|---|---|
| **engine-core** | ~7,191 | ~3,545 | Pure state machine, token execution, gateways, scripting |
| **bpmn-parser** | ~1,963 | (inline) | BPMN 2.0 XML → `ProcessDefinition` (quick-xml + serde) |
| **persistence-nats** | ~1,149 | (inline) | `WorkflowPersistence` via NATS JetStream KV/ObjectStore |
| **engine-server** | ~1,280 | ~1,934 | Axum REST API (HTTP adapter) + background timer scheduler |
| **desktop-tauri** | ~5,186 (TS) + ~623 (Rust) | — | Tauri + React + TailwindCSS + bpmn-js modeler (thin client) |
| **agent-orchestrator** | stub | — | External worker orchestration (planned) |

### Workspace Dependency Graph

```mermaid
graph TD
    subgraph "Rust Workspace"
        EC["engine-core<br/><i>Pure Logic</i>"]
        BP["bpmn-parser<br/><i>XML → Model</i>"]
        PN["persistence-nats<br/><i>NATS JetStream</i>"]
        ES["engine-server<br/><i>Axum REST</i>"]
    end
    
    subgraph "Frontend"
        DT["desktop-tauri<br/><i>React + bpmn-js</i>"]
    end
    
    BP --> EC
    PN --> EC
    ES --> EC
    ES --> BP
    ES --> PN
    DT -->|HTTP / Tauri Commands| ES
    
    style EC fill:#4a9eff,stroke:#333,color:#fff
    style BP fill:#ff9f43,stroke:#333,color:#fff
    style PN fill:#2ed573,stroke:#333,color:#fff
    style ES fill:#a55eea,stroke:#333,color:#fff
    style DT fill:#ff6b81,stroke:#333,color:#fff
```

---

## 2. engine-core — Core Architecture

### 2.1 Module Structure

```mermaid
graph TB
    subgraph "engine-core"
        LIB["lib.rs<br/><i>Public Exports</i>"]
        
        subgraph "model layer"
            MODEL["model.rs<br/>ProcessDefinition<br/>Token<br/>BpmnElement<br/>SequenceFlow"]
            ERROR["error.rs<br/>EngineError<br/>EngineResult"]
        end
        
        subgraph "engine module"
            MOD["mod.rs<br/>WorkflowEngine"]
            TYPES["types.rs<br/>ProcessInstance<br/>PendingTasks<br/>InstanceState"]
            EXEC["executor.rs<br/>run_instance_batch<br/>execute_step"]
            GW["gateway.rs<br/>XOR / AND / OR / Event-Based"]
            SVC["service_task.rs<br/>fetch-and-lock<br/>complete/fail"]
            BOUND["boundary.rs<br/>Timer/Error Events"]
            REG["registry.rs<br/>DefinitionRegistry"]
            STORE["instance_store.rs<br/>InstanceStore"]
            DEFOPS["definition_ops.rs<br/>Deploy/Delete/List"]
            INSTOPS["instance_ops.rs<br/>Start/Complete/Delete"]
            PROCSTART["process_start.rs<br/>Process Instantiation"]
            MSGPROC["message_processor.rs<br/>Message Correlation"]
            TIMERPROC["timer_processor.rs<br/>Timer Processing"]
            PERSISTOPS["persistence_ops.rs<br/>Save/Restore State"]
            RETRY["retry_queue.rs<br/>Fault-Tolerant Retry"]
            USERTASK["user_task.rs<br/>User Task Ops"]
        end
        
        subgraph "support"
            COND["condition.rs<br/>Expression Evaluator"]
            SCRIPT["script_runner.rs<br/>Rhai Scripts"]
            HIST["history.rs<br/>Audit Trail & Diffs"]
            PERSIST["persistence.rs<br/>WorkflowPersistence Trait"]
            INMEM["persistence_in_memory.rs<br/>InMemoryPersistence"]
        end
    end
    
    LIB --> MODEL
    LIB --> MOD
    MOD --> TYPES
    MOD --> EXEC
    MOD --> GW
    MOD --> SVC
    MOD --> BOUND
    MOD --> REG
    MOD --> STORE
    MOD --> DEFOPS
    MOD --> INSTOPS
    MOD --> PROCSTART
    MOD --> MSGPROC
    MOD --> TIMERPROC
    MOD --> PERSISTOPS
    MOD --> RETRY
    MOD --> USERTASK
    EXEC --> GW
    EXEC --> BOUND
    EXEC --> SCRIPT
    GW --> COND
    SVC --> EXEC
    MOD --> HIST
    MOD --> PERSIST

    style MOD fill:#4a9eff,stroke:#333,color:#fff
    style EXEC fill:#4a9eff,stroke:#333,color:#fff
    style MODEL fill:#ff9f43,stroke:#333,color:#fff
    style TYPES fill:#ff9f43,stroke:#333,color:#fff
```

### 2.2 WorkflowEngine — Component Breakdown (K2)

The engine is split into focused components:

```rust
pub struct WorkflowEngine {
    // K2: components instead of a god object
    pub(crate) definitions:             DefinitionRegistry,                  // Immutable definition store
    pub(crate) instances:               InstanceStore,                       // Per-instance locking (K1)
    
    // Wait-State Queues (DashMap for lock-free sharding/concurrency)
    pub(crate) pending_user_tasks:      Arc<DashMap<Uuid, PendingUserTask>>,
    pub(crate) pending_service_tasks:   Arc<DashMap<Uuid, PendingServiceTask>>,
    pub(crate) pending_timers:          Arc<DashMap<Uuid, PendingTimer>>,
    pub(crate) pending_message_catches: Arc<DashMap<Uuid, PendingMessageCatch>>,
    
    // Infrastructure
    pub(crate) persistence:             Option<Arc<dyn WorkflowPersistence>>,
    pub(crate) persistence_error_count: AtomicU64,
    pub(crate) retry_tx:                Option<retry_queue::RetryQueueTx>,
}
```

| Component | Struct | Locking Strategy |
|---|---|---|
| **DefinitionRegistry** | `Arc<RwLock<HashMap<Uuid, Arc<ProcessDefinition>>>>` | Shared, immutable after deploy |
| **InstanceStore** | `Arc<RwLock<HashMap<Uuid, Arc<RwLock<ProcessInstance>>>>>` | Per-instance fine-grained (K1) |
| **PendingTask queues** | `Arc<DashMap<Uuid, Pending*>>` | Lock-free sharding, concurrent O(1) ops |

---

## 3. Data Model

### 3.1 ProcessInstance (after K4 refactoring)

```mermaid
classDiagram
    class ProcessInstance {
        +Uuid id
        +Uuid definition_key
        +String business_key
        +Option~Uuid~ parent_instance_id
        +InstanceState state
        +String current_node
        +Vec~String~ audit_log
        +HashMap~String, Value~ variables
        +HashMap~Uuid, Token~ tokens ★
        +Vec~ActiveToken~ active_tokens
        +HashMap~String, JoinBarrier~ join_barriers
    }

    class Token {
        +Uuid id
        +String current_node
        +HashMap~String, Value~ variables
        +bool is_merged
    }

    class ActiveToken {
        +Token token
        +Option~String~ fork_id
        +usize branch_index
        +bool completed
    }

    class JoinBarrier {
        +String gateway_node_id
        +usize expected_count
        +Vec~Token~ arrived_tokens
    }

    class PendingUserTask {
        +Uuid task_id
        +Uuid instance_id
        +String node_id
        +String assignee
        +Uuid token_id ★
        +DateTime created_at
    }

    class PendingServiceTask {
        +Uuid id
        +Uuid instance_id
        +Uuid definition_key
        +String node_id
        +String topic
        +Uuid token_id ★
        +HashMap~String, Value~ variables_snapshot
        +Option~String~ worker_id
        +i32 retries
    }

    ProcessInstance "1" --o "*" Token : tokens (central store)
    ProcessInstance "1" --o "*" ActiveToken : active_tokens
    ProcessInstance "1" --o "*" JoinBarrier : join_barriers
    PendingUserTask ..> Token : references via token_id
    PendingServiceTask ..> Token : references via token_id

    note for ProcessInstance "★ = K4 refactoring: tokens stored centrally"
```

### 3.2 BPMN Element Types

```rust
pub enum BpmnElement {
    StartEvent,
    TimerStartEvent(TimerDefinition),              // Duration | AbsoluteDate | CronCycle | RepeatingInterval
    MessageStartEvent { message_name: String },
    EndEvent,
    TerminateEndEvent,
    ErrorEndEvent { error_code: String },
    UserTask(String),                               // assignee
    ServiceTask { topic: String, multi_instance: Option<MultiInstanceDef> },
    ScriptTask { script: String, multi_instance: Option<MultiInstanceDef> },
    SendTask { message_name: String, multi_instance: Option<MultiInstanceDef> },
    ExclusiveGateway { default: Option<String> },   // XOR
    InclusiveGateway,                               // OR
    ParallelGateway,                                // AND
    EventBasedGateway,                              // waits for first catch event
    TimerCatchEvent(TimerDefinition),
    BoundaryTimerEvent { attached_to, timer: TimerDefinition, cancel_activity },
    BoundaryMessageEvent { attached_to, message_name, cancel_activity },
    BoundaryErrorEvent { attached_to, error_code: Option<String> },
    MessageCatchEvent { message_name: String },
    CallActivity { called_element: String },
    EmbeddedSubProcess { start_node_id: String },   // embedded sub-process (flattened)
    SubProcessEndEvent { sub_process_id: String },  // synthetic end event
}
```

---

## 4. Execution Architecture

### 4.1 Token Lifecycle (K4)

Tokens exist in **exactly one place** at any moment in time:

```mermaid
flowchart TD
    START(("●")) -->|"Token new"| LV["Local Variable"]
    LV -->|"queue.push_back"| EL

    subgraph EL ["Execution Loop"]
        direction LR
        ES["execute_step"] --> NA["NextAction"]
        NA -->|"Continue"| ES
    end

    EL -->|"WaitForUser / Service / Timer"| CS["Central Store<br/>instance.tokens"]
    CS -->|"complete task<br/>tokens.remove"| LV
    EL -->|"WaitForJoin"| MG["Merged"]
    MG -->|"All tokens arrived"| EL
    EL -->|"EndEvent reached"| DONE(("●"))

    style CS fill:#fef3c7,stroke:#ca8a04,color:#0f172a
    style EL fill:#eff6ff,stroke:#2563eb
    style DONE fill:#16a34a,stroke:#16a34a,color:#fff
    style START fill:#1e293b,stroke:#1e293b,color:#fff
```

> **Central store**: `instance.tokens: HashMap<Uuid, Token>` — pending tasks only hold `token_id: Uuid`.

### 4.2 Execution Loop (run_instance_batch)

```mermaid
flowchart TD
    START["run_instance_batch(instance_id, initial_token)"] --> PUSH["queue.push_back(initial_token)"]
    PUSH --> POP{"queue.pop_front()"}
    POP -->|Some token| STEP["execute_step(instance_id, &mut token)"]
    POP -->|None| PERSIST["persist_instance()"]
    
    STEP --> MATCH{NextAction?}
    
    MATCH -->|Continue| QUEUE["queue.push_back(next_token)"]
    QUEUE --> POP
    
    MATCH -->|ContinueMultiple| FORK["register_join_barrier()<br/>register_active_tokens()<br/>queue ← all forked tokens"]
    FORK --> POP
    
    MATCH -->|WaitForJoin| JOIN{"All tokens arrived?"}
    JOIN -->|No| POP
    JOIN -->|Yes| MERGE["Merge variables<br/>queue ← merged_token"]
    MERGE --> POP
    
    MATCH -->|WaitForUser| STORE_UT["Store token → instance.tokens<br/>Push PendingUserTask"]
    STORE_UT --> POP
    
    MATCH -->|WaitForServiceTask| STORE_ST["Store token → instance.tokens<br/>Push PendingServiceTask"]
    STORE_ST --> POP
    
    MATCH -->|WaitForTimer| STORE_TI["Store token → instance.tokens<br/>Push PendingTimer"]
    STORE_TI --> POP
    
    MATCH -->|WaitForCallActivity| CHILD["spawn_call_activity()<br/>Wait for child instance"]
    CHILD --> POP
    
    MATCH -->|Complete| BRANCH["complete_branch_token()"]
    BRANCH --> ALL{"All tokens done?"}
    ALL -->|Yes| DONE["state = Completed<br/>resume_parent_if_needed()"]
    ALL -->|No| POP
    DONE --> PERSIST
    
    PERSIST --> END["return Ok(())"]
    
    style START fill:#4a9eff,color:#fff
    style END fill:#2ed573,color:#fff
    style FORK fill:#ff9f43,color:#fff
    style MERGE fill:#ff9f43,color:#fff
    style DONE fill:#2ed573,color:#fff
```

### 4.3 Gateway Routing

```mermaid
flowchart LR
    subgraph "ExclusiveGateway (XOR)"
        XOR_IN["Token incoming"] --> XOR_EVAL["Evaluate conditions<br/>(first match wins)"]
        XOR_EVAL -->|Match| XOR_OUT["1 Token → target"]
        XOR_EVAL -->|No match| XOR_DEF{"Default?"}
        XOR_DEF -->|Yes| XOR_OUT2["1 Token → default"]
        XOR_DEF -->|No| XOR_ERR["❌ NoMatchingCondition"]
    end

    subgraph "ParallelGateway (AND)"
        AND_IN["Token incoming"] --> AND_CHECK{"incoming ≥ 2<br/>and !is_merged?"}
        AND_CHECK -->|Yes| AND_WAIT["WaitForJoin<br/>(JoinBarrier)"]
        AND_CHECK -->|No| AND_FORK["Fork: N tokens<br/>(one per outgoing)"]
        AND_WAIT --> AND_MERGE["Merge variables<br/>is_merged = true"]
        AND_MERGE --> AND_FORK
    end

    subgraph "InclusiveGateway (OR)"
        OR_IN["Token incoming"] --> OR_EVAL["Evaluate all conditions"]
        OR_EVAL --> OR_FORK["N tokens<br/>(one per match)"]
    end

    style XOR_ERR fill:#ff4757,color:#fff
    style AND_WAIT fill:#ff9f43,color:#fff
```

---

## 5. Persistence Architecture

### 5.1 WorkflowPersistence Trait

```rust
#[async_trait]
pub trait WorkflowPersistence: Send + Sync {
    // Instance & Definition CRUD
    async fn save_instance(&self, instance: &ProcessInstance) -> EngineResult<()>;
    async fn list_instances(&self)                             -> EngineResult<Vec<ProcessInstance>>;
    async fn delete_instance(&self, id: &str)                  -> EngineResult<()>;
    async fn save_definition(&self, def: &ProcessDefinition)   -> EngineResult<()>;
    async fn list_definitions(&self)                           -> EngineResult<Vec<ProcessDefinition>>;
    
    // Task Queues
    async fn save_user_task(&self, task: &PendingUserTask)           -> EngineResult<()>;
    async fn save_service_task(&self, task: &PendingServiceTask)     -> EngineResult<()>;
    async fn save_timer(&self, timer: &PendingTimer)                 -> EngineResult<()>;
    async fn save_message_catch(&self, catch: &PendingMessageCatch) -> EngineResult<()>;
    
    // File Storage (Object Store)
    async fn save_file(&self, key: &str, data: &[u8])  -> EngineResult<()>;
    async fn load_file(&self, key: &str)                -> EngineResult<Vec<u8>>;
    
    // BPMN XML Storage
    async fn save_bpmn_xml(&self, key: &str, xml: &str) -> EngineResult<()>;
    async fn load_bpmn_xml(&self, key: &str)             -> EngineResult<String>;
    
    // History
    async fn append_history_entry(&self, entry: &HistoryEntry) -> EngineResult<()>;
    async fn query_history(&self, query: HistoryQuery)         -> EngineResult<Vec<HistoryEntry>>;
    
    // Monitoring
    async fn get_storage_info(&self) -> EngineResult<Option<StorageInfo>>;
}
```

### 5.2 Implementations

| Backend | Crate | Storage |
|---|---|---|
| `InMemoryPersistence` | `persistence-memory` | `HashMap` + `Vec` (tests & dev) |
| `NatsPersistence` | `persistence-nats` | NATS JetStream KV + ObjectStore |

**NATS KV stores:**
| KV bucket | Contents | Key format |
|---|---|---|
| `bpm_definitions` | `ProcessDefinition` (JSON) | `def-{uuid}` |
| `bpm_instances` | `ProcessInstance` (JSON) | `inst-{uuid}` |
| `bpm_user_tasks` | `PendingUserTask` (JSON) | `ut-{uuid}` |
| `bpm_service_tasks` | `PendingServiceTask` (JSON) | `st-{uuid}` |
| `bpm_timers` | `PendingTimer` (JSON) | `tmr-{uuid}` |
| `bpm_msg_catches` | `PendingMessageCatch` (JSON) | `msg-{uuid}` |
| `bpm_tokens` | `Token` (JSON) | `tok-{uuid}` |
| `bpm_bpmn_xml` | BPMN 2.0 XML (string) | `xml-{uuid}` |
| `bpm_history` | `HistoryEntry` (JSON) | `hist-{uuid}` |
| **ObjectStore** `instance_files` | Binary files | `file:{instance}-{var}-{filename}` |

### 5.3 Fault-Tolerant Retry Queue (K6)

Since NATS can experience outages, the engine uses a two-stage retry mechanism for stateful I/O operations:
1. **Inline retry**: short backoff (e.g. 50ms) on the direct call. On success, execution continues immediately.
2. **Background retry queue**: if the inline retry fails (e.g. NATS is offline), a `RetryJob` is dispatched to an asynchronous background worker. That worker reads the most recent state from the in-memory state with *exponential backoff* and feeds it into NATS as soon as the system is back online.
This prevents state loss after a transient network failure.

---

## 6. REST API (engine-server)

> Complete OpenAPI 3.0 specification: **[docs/openapi.yaml](openapi.yaml)**

### 6.1 Route Overview (38 endpoints)

```mermaid
graph LR
    subgraph "Process Definitions"
        D1["POST /api/deploy"]
        D2["GET /api/definitions"]
        D3["GET /api/definitions/:id/xml"]
        D4["DELETE /api/definitions/:id"]
        D5["DELETE /api/definitions/bpmn/:bpmn_id"]
    end
    
    subgraph "Process Instances"
        I1["POST /api/start"]
        I1b["POST /api/start/latest"]
        I1c["POST /api/start/timer"]
        I2["GET /api/instances"]
        I3["GET /api/instances/:id"]
        I4["DELETE /api/instances/:id"]
        I5["PUT /api/instances/:id/variables"]
        I6["POST /api/instances/:id/suspend"]
        I7["POST /api/instances/:id/resume"]
    end
    
    subgraph "User Tasks"
        T1["GET /api/tasks"]
        T2["POST /api/complete/:id"]
    end
    
    subgraph "Service Tasks (Camunda-style)"
        S1["GET /api/service-tasks"]
        S2["POST /api/service-task/fetchAndLock"]
        S3["POST /api/service-task/:id/complete"]
        S4["POST /api/service-task/:id/failure"]
        S5["POST /api/service-task/:id/extendLock"]
        S6["POST /api/service-task/:id/bpmnError"]
        S7["POST /api/service-task/:id/retry"]
        S8["POST /api/service-task/:id/resolve"]
    end
    
    subgraph "Files"
        F1["POST /api/instances/:id/files/:var"]
        F2["GET /api/instances/:id/files/:var"]
        F3["DELETE /api/instances/:id/files/:var"]
    end
    
    subgraph "Events"
        E1["POST /api/message"]
        E2["POST /api/timers/process"]
    end
    
    subgraph "Monitoring & Health"
        M0["GET /api/health"]
        M0b["GET /api/ready"]
        M1["GET /api/info"]
        M2["GET /api/monitoring"]
        M2b["GET /api/monitoring/buckets/:bucket/entries"]
        M2c["GET /api/monitoring/buckets/:bucket/entries/:key"]
        M3["GET /api/instances/:id/history"]
        M3b["GET /api/instances/:id/history/:eid"]
    end
    
    style D1 fill:#2ed573,color:#fff
    style I1 fill:#2ed573,color:#fff
    style I1b fill:#2ed573,color:#fff
    style S2 fill:#ff9f43,color:#fff
    style M0 fill:#2ed573,color:#fff
    style M0b fill:#2ed573,color:#fff
```

### 6.2 Server Architecture

```rust
struct AppState {
    pub(crate) engine:       Arc<WorkflowEngine>,                           // Global shared instance (no RwLock needed!)
    pub(crate) persistence:  Option<Arc<dyn WorkflowPersistence>>,          // Optional NATS backend
    pub(crate) deployed_xml: Arc<RwLock<HashMap<String, String>>>,          // XML cache (key → XML)
    pub(crate) nats_url:     String,                                        // For /api/info endpoint
}
```

> The server shares the engine only via `Arc<WorkflowEngine>`. Because all inner collections (`DashMap`, `RwLock<HashMap>`) are thread-safe and mutations go through `&self`, there is no longer a monolithic read/write lock over the entire engine. This eliminates contention under heavy HTTP traffic. Instances are isolated via **K1 (per-instance locking)** through `InstanceStore`.

### 6.3 Background Timer Scheduler

The server spawns a Tokio background task that periodically calls `engine.process_timers()`:

```rust
// main.rs — automatic timer polling (lock-free via Arc<WorkflowEngine>)
let timer_interval_ms: u64 = env::var("TIMER_INTERVAL_MS")
    .ok().and_then(|v| v.parse().ok()).unwrap_or(1000);

tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_millis(timer_interval_ms)).await;
        match timer_engine.process_timers().await {
            Ok(n) => tracing::info!("Timer scheduler: processed {} expired timer(s)", n),
            Err(e) => tracing::warn!("Timer scheduler error: {}", e),
        }
    }
});
```

> **Configuration**: `TIMER_INTERVAL_MS` (default: 1000ms). No external cron required.

### 6.4 Health & Readiness

| Endpoint | Function | Check |
|----------|----------|-------|
| `GET /api/health` | Liveness probe | Always `200 OK` when the server is running |
| `GET /api/ready` | Readiness probe | Checks NATS connection, `503` when disconnected |

---

## 7. Desktop App (Tauri)

### 7.1 Frontend Components

| File | LoC | Purpose |
|---|---|---|
| `App.tsx` | ~180 | Main layout, tab navigation (7 tabs), timer-start detection |
| `ModelerPage.tsx` | ~350 | bpmn-js modeler with deploy, start & variable dialog |
| `InstancesPage.tsx` | ~245 | Instance list (grouped by definition), suspend icon |
| `InstanceDetailDialog.tsx` | ~345 | Instance details with suspend/resume button, timer cycle banner, auto-refresh |
| `InstanceViewer.tsx` | ~125 | Read-only BPMN viewer with active node highlighting + timer pulse |
| `HistoryTimeline.tsx` | ~225 | Event table with filters, detail dialog, diff display |
| `DeployedProcessesPage.tsx` | ~330 | Version grouping, accordion, cascade delete |
| `VariableEditor.tsx` | ~480 | Typed editor (6 types including file), upload/download |
| `MonitoringPage.tsx` | ~365 | Metric cards, NATS storage breakdown, KV browser, auto-refresh |
| `PendingTasksPage.tsx` | ~290 | User & service task lists with completion dialogs |
| `IncidentsPage.tsx` | ~165 | Incident cards with quick retry, detail link, auto-refresh |
| `IncidentDetailDialog.tsx` | ~160 | Retry (configurable retries) + resolve (with VariableEditor) |
| `SettingsPage.tsx` | ~165 | API URL config + connection verify |
| `ErrorBoundary.tsx` | ~72 | React error boundary |
| `MessageDialog.tsx` | ~93 | Message correlation dialog |
| `lib/tauri.ts` | ~170 | All Tauri command wrappers (typed API layer) |
| Custom Properties | ~290 | Condition, script, topic extensions for bpmn-js |
| `index.css` | ~165 | TailwindCSS + HSL design token variables |

### 7.2 Thin-Client Architecture

The desktop app operates as a **thin client** — all workflow logic lives in `engine-server`.

```mermaid
graph TD
    UI["React UI<br/>(desktop-tauri/src)"]
    UI -->|"invoke('deploy_definition')"| TC["Tauri Commands<br/>(src-tauri/src/, 623 LoC)"]
    TC -->|"HTTP REST (reqwest)"| SERVER["engine-server<br/>:8081"]
    SERVER --> ENGINE["WorkflowEngine"]
    ENGINE -.-> NATS[("NATS JetStream")]
    
    style UI fill:#ff6b81,color:#fff
    style TC fill:#a55eea,color:#fff
    style SERVER fill:#4a9eff,color:#fff
    style NATS fill:#2ed573,color:#fff
```

> **Configuration**: `ENGINE_API_URL` environment variable (default: `http://localhost:8081`).

---

## 8. Concurrency & Locking (K1)

### 8.1 Lock Hierarchy

```
WorkflowEngine (Arc)
├── DefinitionRegistry       → Arc<RwLock<HashMap>>          (1 global lock)
├── InstanceStore             → Arc<RwLock<HashMap>>          (1 global lock for the map)
│   └── ProcessInstance[i]   → Arc<RwLock<ProcessInstance>>  (per-instance lock!)
├── pending_user_tasks       → Arc<DashMap>                  (lock-free / sharded)
├── pending_service_tasks    → Arc<DashMap>                  (lock-free / sharded)
├── pending_timers           → Arc<DashMap>                  (lock-free / sharded)
└── pending_message_catches  → Arc<DashMap>                  (lock-free / sharded)
```

### 8.2 Deadlock Prevention Pattern

```rust
// ❌ FORBIDDEN: hold a lock across .await
let inst = instance_arc.write().await;
self.some_async_method().await;  // DEADLOCK!

// ✅ CORRECT: lock scoped before .await
{
    let mut inst = instance_arc.write().await;
    inst.state = InstanceState::Running;
}  // Lock dropped
self.some_async_method().await;  // Safe!
```

---

## 9. History & Audit Trail

Every state transition is stored as a `HistoryEntry`:

| Field | Type | Description |
|---|---|---|
| `event_type` | `HistoryEventType` | InstanceStarted, TaskCompleted, TokenForked, ... |
| `diff` | `Option<HistoryDiff>` | Automatically computed diff (variables, status, node) |
| `actor_type` | `ActorType` | Engine, User, ServiceWorker, Timer, Listener |
| `full_state_snapshot` | `Option<Value>` | Snapshot every 8 audit entries |

**Diff calculation:** `calculate_diff(old: &ProcessInstance, new: &ProcessInstance) → HistoryDiff`
- Variable diff: added, removed, changed (with value truncation >1KB)
- Status diff: "Running → Completed"
- Node diff: "task1 → end"
- File upload detection: "File 'report.pdf' uploaded (1.2 MB)"

---

## 10. Code Statistics

> As of: 2026-04-06 — measured via `wc -l` and `cargo test --workspace`

| Area | Files | LOC |
|---|---|---|
| engine-core (lib) | 25 | 7,191 |
| engine-core (tests) | 2 | 3,545 |
| bpmn-parser | 4 | 1,963 |
| persistence-nats | 5 | 1,149 |
| engine-server (lib + main) | 12 | 1,280 |
| engine-server (E2E tests) | 12 | 1,934 |
| **Rust workspace total** | **60** | **~17,062** |
| desktop-tauri (TypeScript + CSS) | 38 | 5,186 |
| desktop-tauri (Rust backend) | 10 | 623 |
| **Project total** | **~108** | **~22,871** |

### Test Overview (167 tests, all ✅)

| Crate | Unit | E2E | Total |
|---|---|---|---|
| engine-core | 102 | — | 102 |
| bpmn-parser | 27 | — | 27 |
| persistence-nats | 2 | — | 2 |
| engine-server | — | 36 | 36 |
| **Total** | **131** | **36** | **167** |
