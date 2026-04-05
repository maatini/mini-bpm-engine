# bpmninja — Architektur-Dokumentation

> BPMN 2.0 Workflow Engine in Rust, token-basierte Execution
> Stand: 2026-04-05

---

## 1. Workspace-Überblick

Das Projekt ist ein Cargo-Workspace mit 6 Crates, einer Tauri Desktop-App und einem API-Spec:

| Crate | Lib LoC | Test LoC | Zweck |
|---|---|---|---|
| **engine-core** | ~5.450 | ~2.482 | Reine State Machine, Token-Execution, Gateways, Scripting |
| **bpmn-parser** | ~867 | (inline) | BPMN 2.0 XML → `ProcessDefinition` (quick-xml + serde) |
| **persistence-nats** | ~970 | (inline) | `WorkflowPersistence` via NATS JetStream KV/ObjectStore |
| **engine-server** | ~1.125 | ~1.649 | Axum REST API (HTTP-Adapter) + Background Timer Scheduler |
| **desktop-tauri** | ~5.187 (TS) + ~623 (Rust) | — | Tauri + React + TailwindCSS + bpmn-js Modeler (Thin Client) |
| **agent-orchestrator** | stub | — | External Worker Orchestrierung (geplant) |

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

## 2. engine-core — Kernarchitektur

### 2.1 Modul-Struktur

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

### 2.2 WorkflowEngine — Komponentenaufteilung (K2)

Die Engine ist in fokussierte Komponenten aufgeteilt:

```rust
pub struct WorkflowEngine {
    // K2: Komponenten statt God-Object
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

| Komponente | Struct | Locking-Strategie |
|---|---|---|
| **DefinitionRegistry** | `Arc<RwLock<HashMap<Uuid, Arc<ProcessDefinition>>>>` | Shared, immutable nach Deploy |
| **InstanceStore** | `Arc<RwLock<HashMap<Uuid, Arc<RwLock<ProcessInstance>>>>>` | Per-Instance fine-grained (K1) |
| **PendingTask-Queues** | `Arc<DashMap<Uuid, Pending*>>` | Lock-free Sharding, concurrent O(1) ops |

---

## 3. Datenmodell

### 3.1 ProcessInstance (nach K4-Refactoring)

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

    note for ProcessInstance "★ = K4 Refactoring: Tokens zentral gespeichert"
```

### 3.2 BPMN-Elementtypen

```rust
pub enum BpmnElement {
    StartEvent,
    TimerStartEvent(Duration),
    MessageStartEvent { message_name: String },
    EndEvent,
    ErrorEndEvent { error_code: String },
    UserTask(String),                               // assignee
    ServiceTask { topic: String },                  // Camunda-style
    ExclusiveGateway { default: Option<String> },   // XOR
    InclusiveGateway,                               // OR
    ParallelGateway,                                // AND
    EventBasedGateway,                              // waits for first catch event
    TimerCatchEvent(Duration),
    BoundaryTimerEvent { attached_to, duration, cancel_activity },
    BoundaryErrorEvent { attached_to, error_code },
    MessageCatchEvent { message_name: String },
    CallActivity { called_element: String },
    SubProcess { called_element: String },           // embedded sub-process
}
```

---

## 4. Execution-Architektur

### 4.1 Token-Lebenszyklus (K4)

Tokens existieren an **genau einer Stelle** zu jedem Zeitpunkt:

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

> **Central Store**: `instance.tokens: HashMap<Uuid, Token>` — PendingTasks halten nur `token_id: Uuid`.

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

### 4.3 Gateway-Routing

```mermaid
flowchart LR
    subgraph "ExclusiveGateway (XOR)"
        XOR_IN["Token eingehend"] --> XOR_EVAL["Conditions evaluieren<br/>(first match wins)"]
        XOR_EVAL -->|Match| XOR_OUT["1 Token → target"]
        XOR_EVAL -->|Kein Match| XOR_DEF{"Default?"}
        XOR_DEF -->|Ja| XOR_OUT2["1 Token → default"]
        XOR_DEF -->|Nein| XOR_ERR["❌ NoMatchingCondition"]
    end

    subgraph "ParallelGateway (AND)"
        AND_IN["Token eingehend"] --> AND_CHECK{"incoming ≥ 2<br/>und !is_merged?"}
        AND_CHECK -->|Ja| AND_WAIT["WaitForJoin<br/>(JoinBarrier)"]
        AND_CHECK -->|Nein| AND_FORK["Fork: N Tokens<br/>(eine pro Ausgang)"]
        AND_WAIT --> AND_MERGE["Merge variables<br/>is_merged = true"]
        AND_MERGE --> AND_FORK
    end

    subgraph "InclusiveGateway (OR)"
        OR_IN["Token eingehend"] --> OR_EVAL["Alle Conditions evaluieren"]
        OR_EVAL --> OR_FORK["N Tokens<br/>(pro Match eine)"]
    end

    style XOR_ERR fill:#ff4757,color:#fff
    style AND_WAIT fill:#ff9f43,color:#fff
```

---

## 5. Persistence-Architektur

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

### 5.2 Implementierungen

| Backend | Crate | Storage |
|---|---|---|
| `InMemoryPersistence` | `engine-core` | `HashMap` + `Vec` (Tests & Dev) |
| `NatsPersistence` | `persistence-nats` | NATS JetStream KV + ObjectStore |

**NATS KV-Stores:**
| KV-Bucket | Inhalt | Key-Format |
|---|---|---|
| `bpm_definitions` | `ProcessDefinition` (JSON) | `def-{uuid}` |
| `bpm_instances` | `ProcessInstance` (JSON) | `inst-{uuid}` |
| `bpm_user_tasks` | `PendingUserTask` (JSON) | `ut-{uuid}` |
| `bpm_service_tasks` | `PendingServiceTask` (JSON) | `st-{uuid}` |
| `bpm_timers` | `PendingTimer` (JSON) | `tmr-{uuid}` |
| `bpm_msg_catches` | `PendingMessageCatch` (JSON) | `msg-{uuid}` |
| `bpm_tokens` | `Token` (JSON) | `tok-{uuid}` |
| `bpm_bpmn_xml` | BPMN 2.0 XML (String) | `xml-{uuid}` |
| `bpm_history` | `HistoryEntry` (JSON) | `hist-{uuid}` |
| **ObjectStore** `instance_files` | Binärdateien | `file:{instance}-{var}-{filename}` |

### 5.3 Fault-Tolerant Retry Queue (K6)

Da NATS Ausfälle haben kann, verwendet die Engine einen zweistufigen Retry-Mechanismus für zustandsbehaltende I/O-Operationen:
1. **Inline-Retry**: Kurzes Backoff (z.B. 50ms) beim direkten Aufruf. Bei Erfolg geht es sofort weiter.
2. **Background Retry Queue**: Schlägt der Inline-Retry fehl (z.B. NATS ist offline), wird ein `RetryJob` an einen asynchronen Background-Worker übermittelt. Dieser Worker liest mit *exponentiellem Backoff* asynchron aus dem In-Memory-State den aktuellsten Stand aus und speist in NATS ein, sobald das System wieder online ist.
Dadurch entsteht kein State-Verlust nach einem transienten Netzwerkfehler.

---

## 6. REST API (engine-server)

> Vollständige OpenAPI 3.0 Spezifikation: **[docs/openapi.yaml](openapi.yaml)**

### 6.1 Route-Übersicht (32 Endpoints)

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
        I2["GET /api/instances"]
        I3["GET /api/instances/:id"]
        I4["DELETE /api/instances/:id"]
        I5["PUT /api/instances/:id/variables"]
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

### 6.2 Server-Architektur

```rust
struct AppState {
    pub(crate) engine:       Arc<WorkflowEngine>,                           // Global shared instance (no RwLock needed!)
    pub(crate) persistence:  Option<Arc<dyn WorkflowPersistence>>,          // Optional NATS backend
    pub(crate) deployed_xml: Arc<RwLock<HashMap<String, String>>>,          // XML cache (key → XML)
    pub(crate) nats_url:     String,                                        // For /api/info endpoint
}
```

> Der Server teilt die Engine lediglich über `Arc<WorkflowEngine>`. Da alle inneren Collections (`DashMap`, `RwLock<HashMap>`) Thread-Safe sind und Mutationen über `&self` ablaufen, gibt es keinen monolithischen Read/Write-Lock mehr für die gesamte Engine. Dies eliminiert Contention bei hohem HTTP-Traffic. Instanzen sind über **K1 (Per-Instance-Locking)** via `InstanceStore` isoliert.

### 6.3 Background Timer Scheduler

Der Server startet einen Tokio-Background-Task, der periodisch `engine.process_timers()` aufruft:

```rust
// main.rs — automatisches Timer-Polling (lock-free via Arc<WorkflowEngine>)
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

> **Konfiguration**: `TIMER_INTERVAL_MS` (Default: 1000ms). Kein externer Cron nötig.

### 6.4 Health & Readiness

| Endpoint | Funktion | Prüfung |
|----------|----------|---------|
| `GET /api/health` | Liveness Probe | Immer `200 OK` wenn Server läuft |
| `GET /api/ready` | Readiness Probe | Prüft NATS-Verbindung, `503` wenn disconnected |

---

## 7. Desktop-App (Tauri)

### 7.1 Frontend-Komponenten

| Datei | LoC | Zweck |
|---|---|---|
| `App.tsx` | 169 | Main Layout, Tab-Navigation (6 Tabs) |
| `Modeler.tsx` | 311 | bpmn-js Modeler mit Deploy, Start & Variable-Dialog |
| `Instances.tsx` | 518 | Instanz-Liste (grouped by Definition), Detail-Overlay |
| `InstanceViewer.tsx` | 108 | Read-only BPMN-Viewer mit aktiver Node-Markierung |
| `HistoryTimeline.tsx` | 225 | Event-Tabelle mit Filtern, Detail-Dialog, Diff-Anzeige |
| `DeployedProcesses.tsx` | 326 | Versions-Gruppierung, Accordion, Cascade Delete |
| `VariableEditor.tsx` | 479 | Typed Editor (6 Typen inkl. File), Upload/Download |
| `Monitoring.tsx` | 362 | Metric Cards, NATS Storage Breakdown, KV-Browser, Auto-Refresh (5s) |
| `PendingTasks.tsx` | 286 | User & Service Task Listen mit Completion-Dialogen |
| `Settings.tsx` | 161 | API URL Config + Connection Verify |
| `ErrorBoundary.tsx` | 72 | React Error Boundary |
| `MessageDialog.tsx` | 93 | Message-Korrelations-Dialog |
| `IncidentsView.tsx` | 120 | Incident-List (Persistence Errors) |
| `lib/tauri.ts` | 263 | Alle Tauri Command Wrappers (typisierte API-Schicht) |
| Custom Properties | ~337 | Condition, Script, Topic Extensions für bpmn-js |
| `index.css` | 161 | TailwindCSS + HSL Design-Token-Variablen |

### 7.2 Thin-Client Architektur

Die Desktop-App operiert als **Thin Client** — alle Workflow-Logik liegt im `engine-server`.

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

> **Konfiguration**: `ENGINE_API_URL` Environment-Variable (Default: `http://localhost:8081`).

---

## 8. Concurrency & Locking (K1)

### 8.1 Lock-Hierarchie

```
WorkflowEngine (Arc)
├── DefinitionRegistry       → Arc<RwLock<HashMap>>          (1 globaler Lock)
├── InstanceStore             → Arc<RwLock<HashMap>>          (1 globaler Lock für Map)
│   └── ProcessInstance[i]   → Arc<RwLock<ProcessInstance>>  (per-Instance Lock!)
├── pending_user_tasks       → Arc<DashMap>                  (lock-free / sharded)
├── pending_service_tasks    → Arc<DashMap>                  (lock-free / sharded)
├── pending_timers           → Arc<DashMap>                  (lock-free / sharded)
└── pending_message_catches  → Arc<DashMap>                  (lock-free / sharded)
```

### 8.2 Deadlock-Prevention Pattern

```rust
// ❌ VERBOTEN: Lock über .await halten
let inst = instance_arc.write().await;
self.some_async_method().await;  // DEADLOCK!

// ✅ KORREKT: Lock scoped vor .await
{
    let mut inst = instance_arc.write().await;
    inst.state = InstanceState::Running;
}  // Lock dropped
self.some_async_method().await;  // Safe!
```

---

## 9. History & Audit Trail

Jeder State-Übergang wird als `HistoryEntry` gespeichert:

| Feld | Typ | Beschreibung |
|---|---|---|
| `event_type` | `HistoryEventType` | InstanceStarted, TaskCompleted, TokenForked, ... |
| `diff` | `Option<HistoryDiff>` | Automatisch berechneter Diff (variables, status, node) |
| `actor_type` | `ActorType` | Engine, User, ServiceWorker, Timer, Listener |
| `full_state_snapshot` | `Option<Value>` | Snapshot alle 8 Audit-Einträge |

**Diff-Berechnung:** `calculate_diff(old: &ProcessInstance, new: &ProcessInstance) → HistoryDiff`
- Variable-Diff: added, removed, changed (mit Wert-Truncation >1KB)
- Status-Diff: "Running → Completed"
- Node-Diff: "task1 → end"
- File-Upload-Erkennung: "File 'report.pdf' uploaded (1.2 MB)"

---

## 10. Code-Statistiken

> Stand: 05.04.2026 — gemessen via `wc -l` und `cargo test --workspace`

| Bereich | Dateien | LOC |
|---|---|---|
| engine-core (lib) | 24 | 5.450 |
| engine-core (tests) | 2 | 2.482 |
| bpmn-parser | 4 | 867 |
| persistence-nats | 5 | 970 |
| engine-server (lib + main) | 12 | 1.125 |
| engine-server (E2E tests) | 12 | 1.649 |
| **Rust Workspace Gesamt** | **59** | **~12.543** |
| desktop-tauri (TypeScript + CSS) | 38 | 5.187 |
| desktop-tauri (Rust Backend) | 10 | 623 |
| **Projekt Gesamt** | **~107** | **~18.353** |

### Test-Übersicht (140 Tests, alle ✅)

| Crate | Unit | E2E | Gesamt |
|---|---|---|---|
| engine-core | 96 | — | 96 |
| bpmn-parser | 6 | — | 6 |
| persistence-nats | 2 | — | 2 |
| engine-server | — | 36 | 36 |
| **Gesamt** | **104** | **36** | **140** |
