# mini-bpm

[![GitHub stars](https://img.shields.io/github/stars/maatini/mini-bpm-engine.svg?style=flat-square)](https://github.com/maatini/mini-bpm-engine/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/maatini/mini-bpm-engine.svg?style=flat-square)](https://github.com/maatini/mini-bpm-engine/network/members)
[![GitHub issues](https://img.shields.io/github/issues/maatini/mini-bpm-engine.svg?style=flat-square)](https://github.com/maatini/mini-bpm-engine/issues)
[![Rust](https://img.shields.io/badge/Rust-cargo-brightgreen.svg?style=flat-square)](https://www.rust-lang.org/)

![mini-bpm-engine](readme-assets/mini-bpm-engine.jpeg)

Eine einbettbare BPMN 2.0 Workflow-Engine in Rust.

## Crates (Module)

* `bpmn-parser`: Parst BPMN 2.0 XML-Definitionen in interne Rust-Strukturen.
* `engine-core`: Die Hauptbibliothek der Workflow-Engine — token-basierte Ausführung, Gateway-Routing mit Condition-Evaluator, Script-Engine (Execution Listeners), Service-Task-Support und umfassendes Error-Handling via `EngineError` (thiserror). Tests sind in ein separates Modul (`tests.rs`) ausgelagert.
* `persistence-nats`: (Optional) Bietet NATS-basierte Persistenz. Nutzt JetStream KV-Stores für Instanzen, Definitionen und Pending-Tasks, sowie einen Object Store (`bpmn_xml`) für die originalen BPMN-Dateien. Darüber hinaus wird ein Event-Sourcing-Ansatz via JetStream Publishing unterstützt.
* `engine-server`: Ein Axum-basierter HTTP-Server mit REST-API. Nutzt einen typsicheren `AppError`-Enum für konsistente HTTP-Fehlercodes (400/404/409/500).
* `desktop-tauri`: Eine Tauri-Desktop-Anwendung (React, bpmn-js), die mit der Workflow-Engine interagiert. Bietet einen integrierten Modeler, eine Instanzen-Ansicht mit automatisch zentrierender Diagramm-Visualisierung und eine tabellarische Event-Historie inklusiver JSON-Diffs.
* `agent-orchestrator`: Ein Crate zur Orchestrierung von externen Agenten/Workern, die mit der Engine interagieren.

## Unterstützte BPMN-Elemente

### Basis-Elemente

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/start-event.svg" width="28"> | **StartEvent** | Einfacher Startpunkt — Prozess wird sofort gestartet. |
| <img src="readme-assets/bpmn-icons/timer-start-event.svg" width="28"> | **TimerStartEvent** | Timer-gesteuerter Start nach einer konfigurierbaren ISO 8601 Dauer (`PT30S`, `PT5M`). |
| <img src="readme-assets/bpmn-icons/message-start-event.svg" width="28"> | **MessageStartEvent** | Prozess wird durch eine eingehende Nachricht (via `messageName`) gestartet. |
| <img src="readme-assets/bpmn-icons/end-event.svg" width="28"> | **EndEvent** | Endpunkt — Prozessinstanz wird als abgeschlossen markiert. |
| <img src="readme-assets/bpmn-icons/error-end-event.svg" width="28"> | **ErrorEndEvent** | Terminiert den Prozess mit einem BPMN-Fehlercode (`errorCode`). |
| <img src="readme-assets/bpmn-icons/user-task.svg" width="34"> | **UserTask** | Erstellt einen Pending-Task, der extern abgeschlossen werden muss. |
| <img src="readme-assets/bpmn-icons/service-task.svg" width="34"> | **ServiceTask** | Tasks, die von externen Workern (z.B. agent-orchestrator) per fetch-and-lock abgearbeitet werden. |

### Gateways

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/exclusive-gateway.svg" width="28"> | **ExclusiveGateway (XOR)** | Genau ein ausgehender Pfad wird gewählt (Bedingungsauswertung). Optionaler Default-Flow. |
| <img src="readme-assets/bpmn-icons/parallel-gateway.svg" width="28"> | **ParallelGateway (AND)** | Alle ausgehenden Pfade werden bedingungslos verfolgt (Token-Fork). Als Join wartet es auf **alle** eingehenden Tokens (JoinBarrier) und mergt deren Variablen. |
| <img src="readme-assets/bpmn-icons/inclusive-gateway.svg" width="28"> | **InclusiveGateway (OR)** | Alle Pfade, deren Bedingung `true` ergibt, werden parallel verfolgt (Token-Forking). Als Join wartet es auf alle erwarteten Tokens. |

### Events (Phase 1)

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/timer-catch-event.svg" width="28"> | **TimerCatchEvent** | Intermediate Catch Event — pausiert den Prozess bis ein Timer abläuft. Auflösung via `POST /api/timers/process`. |
| <img src="readme-assets/bpmn-icons/message-catch-event.svg" width="28"> | **MessageCatchEvent** | Intermediate Catch Event — pausiert den Prozess bis eine Nachricht mit passendem `messageName` korreliert wird. |
| <img src="readme-assets/bpmn-icons/boundary-timer-event.svg" width="28"> | **BoundaryTimerEvent** | An einen Task angeheftetes Timer-Event (interrupting). Unterbricht den Task wenn der Timer abläuft. Timer wird bei Task-Abschluss automatisch storniert. |
| <img src="readme-assets/bpmn-icons/boundary-error-event.svg" width="28"> | **BoundaryErrorEvent** | An einen ServiceTask angeheftetes Error-Event. Fängt BPMN-Fehler (`errorCode`) ab und leitet den Token auf einen alternativen Pfad um. |

### Zusätzliche Konzepte

* **Conditional Sequence Flows** — Kanten können Bedingungsausdrücke tragen (z.B. `amount > 100`, `status == 'approved'`). Der integrierte Condition-Evaluator unterstützt `==`, `!=`, `>`, `>=`, `<`, `<=` sowie Truthy-Checks.
* **Execution Listeners** — Nodes können Start- und End-Scripts besitzen, die Prozessvariablen lesen und mutieren (z.B. `x = x * 2; if x > 10 { result = "big" }`).
* **Dynamische Prozessvariablen** — Variablen laufender Instanzen können zur Laufzeit via REST-API aktualisiert werden. Änderungen werden in der NATS-Persistenz automatisch mit pausierten Tokens von Pending-Tasks synchronisiert.
* **Message Correlation** — Eingehende Nachrichten werden über `messageName` und optional `businessKey` an wartende Instanzen oder als Startimpuls an passende Definitionen korreliert.
* **Timer-Verarbeitung** — Pending-Timer werden via Polling (`process_timers()`) aufgelöst. Boundary-Timer werden bei Task-Abschluss automatisch storniert (`cancel_boundary_timers`).
* **BPMN Error Handling** — Service-Tasks können via `bpmnError`-Endpunkt Fehler melden. Die Engine routet den Token an das passende `BoundaryErrorEvent` (Matching via `errorCode`).
* **Detail-Historie** — Das Audit-Log der Engine liefert ein lückenloses Playback aller Token-Routings und State-Veränderungen, detailliert aufgeschlüsselt nach den zugehörigen Aktoren (`User`, `Engine`, `Timer`, `ServiceWorker`).
* **Persistente Wait-States** — Timer und Message Catches werden in NATS KV-Stores persistiert und überleben damit Server-Neustarts.

## Architektur

Das folgende Diagramm nutzt Mermaid, um die hochauflösende Vektor-Struktur des mini-bpm Projekts darzustellen:

```mermaid
flowchart TD
    %% Styling
    classDef core fill:#e2e8f0,stroke:#64748b,stroke-width:2px,color:#0f172a;
    classDef server fill:#bae6fd,stroke:#0284c7,stroke-width:2px,color:#0c4a6e;
    classDef persistence fill:#bbf7d0,stroke:#16a34a,stroke-width:2px,color:#14532d;
    classDef desktop fill:#fef08a,stroke:#ca8a04,stroke-width:2px,color:#713f12;
    classDef agent fill:#fbcfe8,stroke:#db2777,stroke-width:2px,color:#831843;
    classDef storage fill:#f0fdf4,stroke:#16a34a,stroke-width:1px,color:#14532d;

    subgraph "Clients / External"
        UI["desktop-tauri\n(Tauri + React + bpmn-js)"]:::desktop
        Agent["agent-orchestrator\n(External Workers)"]:::agent
        ExtMsg["External Systems\n(Messages / Timers)"]:::agent
    end

    subgraph "Server Layer"
        Axum["engine-server\n(Axum HTTP REST API)"]:::server
    end

    subgraph "Core Workflow Engine"
        Parser["bpmn-parser\n(BPMN 2.0 XML → ProcessDefinition)"]:::core
        Engine["engine-core\n(State Machine / Token Execution)"]:::core
        Trait["WorkflowPersistence\n(Trait)"]:::core
    end

    subgraph "Storage Layer"
        NatsImpl["persistence-nats\n(impl WorkflowPersistence)"]:::persistence
        Nats[("NATS JetStream\nKV: instances, definitions,\ntimers, messages, tokens")]:::storage
    end

    %% Client → Server
    UI -- "HTTP REST API" --> Axum
    Agent -- "fetchAndLock / complete\n/ bpmnError" --> Axum
    ExtMsg -- "POST /api/message\nPOST /api/timers/process" --> Axum

    %% Server → Core
    Axum -- "parse_bpmn_xml()" --> Parser
    Axum -- "deploy / start / complete\n/ correlate_message\n/ process_timers" --> Engine

    %% Core internals
    Engine -. "uses" .-> Trait

    %% Persistence
    Trait -. "implemented by" .-> NatsImpl
    Axum -- "injects persistence" --> NatsImpl
    NatsImpl -- "KV get/put/delete\nJetStream publish" --> Nats
```

## Voraussetzungen

Du kannst dieses Projekt entweder in einer isolierten Devbox-Umgebung (empfohlen) oder mit lokal installierten Tools entwickeln.

### Variante A: Devbox (Empfohlen)
Dieses Projekt nutzt [Devbox](https://www.jetify.com/devbox) für eine reproduzierbare Umgebung.
1. Installiere Devbox auf deinem System.
2. Führe im Projektverzeichnis `devbox shell` aus. Dies installiert automatisch Rust, Node.js und den NATS-Server in der isolierten Umgebung.

### Variante B: Manuell (Shell)
Folgende Tools müssen auf deinem System installiert sein:
- Rust (via `rustup`)
- Node.js (≥ 18)
- Docker & Docker Compose (für NATS)

## Test-Metriken

### Workspace Test Summary

| Crate | Unit Tests | Integration Tests | Gesamt |
|---|---|---|---|
| **engine-core** | 55 | — | 55 |
| **bpmn-parser** | 12 | — | 12 |
| **persistence-nats** | 2 | — | 2 |
| **engine-server** | — | 5 | 5 |
| **Gesamt** | **69** | **5** | **74** ✅ |

### Code Coverage (cargo-llvm-cov)

| Crate / Modul | Lines | Covered | Line Coverage |
|---|---|---|---|
| **engine-core** `model.rs` | 335 | 322 | **96.1%** ✅ |
| **engine-core** `engine.rs` | 1 840 | — | — ² |
| **engine-core** `condition.rs` | 112 | 60 | **81.0%** |
| **engine-core** `script_runner.rs` | 86 | 54 | **94.7%** ✅ |
| **engine-core** `service_task.rs` | 389 | 225 | **93.3%** ✅ |
| **engine-core** `history.rs` | 302 | 173 | **92.5%** ✅ |
| **engine-core** `tests.rs` | 1 209 | — | **~99%** ✅ |
| **bpmn-parser** | 909 | 306 | **91.6%** ✅ |
| **persistence-nats** | 668 | 45 | **8.8%** ¹ |
| **engine-server** | 705 | 12 | **2.7%** ¹ |

¹ *Benötigen laufende NATS-Instanz bzw. HTTP-Server für vollständige Integration Tests.*
² *engine.rs wurde in Phase 1 signifikant erweitert — ein neuer Coverage-Lauf steht aus.*

### Mutation Testing (cargo-mutants, engine-core)

| Metrik | Wert |
|---|---|
| Generierte Mutanten | 301 |
| Unviable (kompiliert nicht) | 158 (52.4%) |
| Caught (von Tests erkannt) | 133 |
| Missed (nicht erkannt) | 10 |
| **Mutation Score** | **93.0%** ✅ |

> [!NOTE]
> Einer der härtesten Prüfsteine in Rust: Durch unsere fokussierten Edge-Case-Tests (Verifizieren von iterativen Listen, Inkrement-Zuweisungen und String-Exaktheiten) konnte der PIT / Mutanten-Score erfolgreich auf **>90%** angehoben werden!

### API Integration Tests (engine-server)

| Metrik | Wert |
|---|---|
| Tests | 5 |
| Passed | 5 |
| **Pass Rate** | **100%** ✅ |

> [!NOTE]
> Die Rust `engine-server` E2E-Tests validieren Deployments, Variablen-Updates und Event-Historie über den echten Axum REST-API Stack inkl. In-Memory Persistence Mock (`tests/e2e_deploy.rs`, `e2e_variables.rs`, `e2e_history.rs`).

### E2E Tests (Playwright, desktop-tauri)

| Metrik | Wert |
|---|---|
| Tests | 24 |
| Passed | 24 |
| **E2E Pass Rate** | **100%** ✅ |

### Coverage ermitteln

```bash
# Voraussetzung: cargo-llvm-cov installiert
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov

# Coverage Report
cargo llvm-cov --workspace --exclude mini-bpm-desktop

# Mutation Testing (engine-core)
cargo install cargo-mutants
cargo mutants --package engine-core
```

## Build, Test & Lint

| Aktion | Variante A: Devbox | Variante B: Manuell (Shell) |
|---|---|---|
| **Build** | `devbox run build` | `cargo build --workspace` |
| **Test** | `devbox run test` | `cargo test --workspace` |
| **Lint** | `devbox run lint` | `cargo clippy --workspace -- -D warnings` |
| **Format Check** | `devbox run fmt` | `cargo fmt --all --check` |

## Engine-Server starten

Der Engine-Server benötigt eine laufende NATS-Instanz für die Persistenz. 

### Variante A: Devbox

```bash
# NATS lokal via Docker starten
devbox run nats:up

# Engine-Server ausführen
devbox run engine:run
```

### Variante B: Manuell (Shell)

```bash
# NATS im Hintergrund starten
docker compose up -d nats

# Engine-Server ausführen
cargo run -p engine-server
```

Der Server lauscht standardmäßig auf `http://localhost:8081`. 
*Hinweis: Wenn NATS auf einem anderen Port als 4222 läuft, kann dies via Umgebungsvariable `NATS_URL` angepasst werden.*

### REST API Endpunkte

* `POST /api/deploy` - Eine BPMN-Definition bereitstellen
* `POST /api/start` - Eine neue Prozessinstanz starten
* `GET /api/tasks` - Alle ausstehenden Benutzer-Tasks (User Tasks) auflisten
* `POST /api/complete/:id` - Einen Benutzer-Task abschließen
* `GET /api/instances` - Alle Prozessinstanzen auflisten
* `GET /api/instances/:id` - Details einer einzelnen Instanz abrufen
* `GET /api/instances/:id/history` - Event-Historie einer Instanz abrufen (mit Filter-Query-Params)
* `GET /api/instances/:id/history/:event_id` - Einzelnes History-Event abrufen
* `PUT /api/instances/:id/variables` - Variablen einer Prozessinstanz aktualisieren
* `DELETE /api/instances/:id` - Eine Prozessinstanz löschen
* `GET /api/definitions` - Alle bereitgestellten Definitionen auflisten
* `GET /api/definitions/:id/xml` - Das originale BPMN-XML einer Definition abrufen
* `DELETE /api/definitions/:id` - Eine Prozessdefinition löschen (Query `?cascade=true` zum Mitlöschen aller Instanzen)

#### Service Tasks
* `GET /api/service-tasks` - Alle ausstehenden Service Tasks auflisten
* `POST /api/service-task/fetchAndLock` - Tasks für Worker abrufen und sperren (inkl. Long-Polling)
* `POST /api/service-task/:id/complete` - Einen Service Task erfolgreich abschließen
* `POST /api/service-task/:id/failure` - Einen Service Task als fehlgeschlagen markieren
* `POST /api/service-task/:id/extendLock` - Die Sperrdauer eines Tasks verlängern
* `POST /api/service-task/:id/bpmnError` - Einen BPMN-Fehler für einen Task melden

#### Messages & Timers
* `POST /api/message` - Eine Nachricht korrelieren (löst wartende `MessageCatchEvents` auf oder startet `MessageStartEvent`-Prozesse)
* `POST /api/timers/process` - Alle abgelaufenen Timer verarbeiten (löst wartende `TimerCatchEvents` und `BoundaryTimerEvents` auf)

#### Info & Monitoring
* `GET /api/info` - Backend-Informationen abrufen (Typ, NATS-URL, Verbindungsstatus)
* `GET /api/monitoring` - Monitoring-Daten abrufen (Zähler für Definitionen, Instanzen, Tasks, Storage-Info)

## Desktop-Anwendung (UI) starten

Die `mini-bpm-desktop` Anwendung ist ein "Thin Client", der sich ausschließlich über HTTP mit der `engine-server` Instanz verbindet. 

> [!CAUTION]  
> Stelle sicher, dass der Engine-Server läuft, bevor die UI gestartet wird. Du kannst den API-Endpunkt über die Umgebungsvariable `ENGINE_API_URL` umleiten (Standard: `http://localhost:8081`).

### Variante A: Devbox

```bash
devbox run ui:dev
```

### Variante B: Manuell (Shell)

```bash
cd desktop-tauri
npm install
npm run tauri dev
```

### Tauri-Kommandos
Das Frontend der Desktop-Anwendung nutzt folgende Tauri-Kommandos zur Interaktion mit dem eigenen Backend, welches wiederum die HTTP Rest-Aufrufe zum Engine-Server macht:
* **Deployment & Start**: `deploy_definition`, `deploy_simple_process`, `start_instance`
* **Instanzen**: `list_instances`, `get_instance_details`, `get_instance_history`, `update_instance_variables`, `delete_instance`
* **User Tasks**: `get_pending_tasks`, `complete_task`
* **Service Tasks**: `get_pending_service_tasks`, `fetch_and_lock_service_tasks`, `complete_service_task`
* **Definitionen**: `list_definitions`, `get_definition_xml`, `delete_definition`
* **Konfiguration & Monitoring**: `get_api_url`, `set_api_url`, `get_monitoring_data`
* **Dateisystem**: `read_bpmn_file`

## Komplette Infrastruktur starten (Docker Compose)

Um NATS und den Engine-Server gemeinsam und isoliert auszuführen:

### Variante A: Devbox

```bash
devbox run engine:docker
```

### Variante B: Manuell (Shell)

```bash
docker compose up --build
```
*(Die Services sind anschließend unter `localhost:8081` und `localhost:4222` erreichbar)*

## Agent Orchestrator (Externer Worker)

Der `agent-orchestrator` ist ein Beispiel für einen externen Microservice, der periodisch nach anstehenden "Service Tasks" im Engine-Server fragt (`fetchAndLock`), diese abarbeitet und anschließend bei der Engine als erledigt meldet.

Um das Zusammenspiel zu testen, stellen Sie sicher, dass NATS und der Engine-Server bereits laufen. Öffnen Sie dann ein neues Terminal:

### Variante A & B: Identischer Befehl (Cargo)

*(Sollte `cargo` nicht installiert sein, vorher `devbox shell` aufrufen)*

```bash
cargo run -p agent-orchestrator
```
