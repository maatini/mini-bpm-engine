# BPMNinja

[![Rust](https://img.shields.io/badge/Rust-stable-brightgreen.svg?style=flat-square)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/Tests-175_passing-success?style=flat-square)]()
[![Mutation Score](https://img.shields.io/badge/Mutation_Score-~87%25-blue?style=flat-square)]()
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg?style=flat-square)](#lizenz)

<div align="center">
  <img src="desktop-tauri/public/logo.png" alt="BPMNinja Logo" width="300" />
</div>

**Eine BPMN 2.0 Workflow-Engine in Rust** — token-basierte Ausführung, NATS-Persistenz, REST-API und Desktop-UI.

---

## Inhaltsverzeichnis

- [Überblick](#überblick)
- [Crates (Module)](#crates-module)
- [Unterstützte BPMN-Elemente](#unterstützte-bpmn-elemente)
- [Architektur](#architektur)
- [Schnellstart](#schnellstart)
- [REST API](#rest-api)
- [Desktop-Anwendung (UI)](#desktop-anwendung-ui)
- [Docker Compose](#docker-compose)
- [Test-Metriken](#test-metriken)
- [Roadmap](#roadmap)
- [Lizenz](#lizenz)

---

## Überblick

bpmninja ist eine leichtgewichtige BPMN 2.0 Engine mit folgenden Kernfeatures:

- **Token-basierte Ausführung** — jeder Pfad wird als eigenständiger Token verfolgt
- **18 BPMN-Elemente** — Start/End Events, User/Service Tasks, Gateways (XOR, AND, OR, Event-Based), Timer, Messages, Boundary Events, Call Activities, Sub-Processes
- **Vollständige ISO 8601 Timer** — Duration (`PT30S`), AbsoluteDate (`2026-04-06T14:30:00Z`), Cron (`0 9 * * MON-FRI`), Repeating Interval (`R3/PT10M`)
- **Lock-Free Concurrency** — Multi-threaded Skalierung dank `DashMap` Wait-State Queues
- **NATS JetStream Persistenz** — KV-Stores für Instanzen, Object Store für Dateien, Event-Streaming für History
- **Fault-Tolerant Retry Queue** — 2-stufiges Retry-System mit Background-Worker gegen NATS-Ausfälle
- **Automatischer Timer-Scheduler** — Background-Task verarbeitet abgelaufene Timer (konfigurierbar via `TIMER_INTERVAL_MS`)
- **Camunda-kompatible Service Tasks** — Fetch-and-Lock Pattern mit Long-Polling
- **Rhai Script Engine** — Execution Listeners für dynamische Variablenmanipulation
- **Desktop-UI** — Tauri-App mit bpmn-js Modeler und Live-Instanzverfolgung (inkl. plattformübergreifender GitHub Actions CI-Releases)

---

## Crates (Module)

| Crate | Zweck |
|-------|-------|
| **`engine-core`** | Kernbibliothek — State Machine, Token-Registry, Gateway-Routing, Condition-Evaluator, Script-Engine |
| **`bpmn-parser`** | Parst BPMN 2.0 XML (`quick-xml` + `serde`) zu internen `ProcessDefinition`-Structs |
| **`persistence-nats`** | NATS JetStream-basierte `WorkflowPersistence`-Implementierung (KV, Object Store, Streams) |
| **`engine-server`** | Axum HTTP REST-API mit typsicherem Error-Handling (`AppError` → HTTP-Statuscodes) |
| **`desktop-tauri`** | Tauri Desktop-App (React + bpmn-js) mit Modeler, Instanzen-Dashboard und Event-Historie |
| **`agent-orchestrator`** | Beispiel-Worker für externe Service-Task-Verarbeitung |

---

## Unterstützte BPMN-Elemente

### Basis-Elemente

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/start-event.svg" width="28"> | **StartEvent** | Einfacher Startpunkt — Prozess wird sofort gestartet. |
| <img src="readme-assets/bpmn-icons/timer-start-event.svg" width="28"> | **TimerStartEvent** | Timer-gesteuerter Start — unterstützt ISO 8601 Duration (`PT30S`), AbsoluteDate, Cron-Cycle und Repeating Intervals. |
| <img src="readme-assets/bpmn-icons/message-start-event.svg" width="28"> | **MessageStartEvent** | Prozess wird durch eingehende Nachricht (via `messageName`) gestartet. |
| <img src="readme-assets/bpmn-icons/end-event.svg" width="28"> | **EndEvent** | Endpunkt — Prozessinstanz wird als abgeschlossen markiert. |
| <img src="readme-assets/bpmn-icons/terminate-end-event.svg" width="28"> | **TerminateEndEvent** | Endpunkt — Bricht alle aktiven Tokens sofort ab. |
| <img src="readme-assets/bpmn-icons/error-end-event.svg" width="28"> | **ErrorEndEvent** | Terminiert den Prozess mit einem BPMN-Fehlercode (`errorCode`). |
| <img src="readme-assets/bpmn-icons/user-task.svg" width="34"> | **UserTask** | Erstellt einen Pending-Task, der extern abgeschlossen werden muss. |
| <img src="readme-assets/bpmn-icons/service-task.svg" width="34"> | **ServiceTask** | Externe Verarbeitung via Fetch-and-Lock Pattern (Camunda-kompatibel). |
| <img src="readme-assets/bpmn-icons/script-task.svg" width="34"> | **ScriptTask** | Führt inline verankerte Scripte über die Rhai Engine aus. |
| <img src="readme-assets/bpmn-icons/send-task.svg" width="34"> | **SendTask** | Versendet via Throw Event eine Message und läuft direkt weiter. |

### Gateways

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/exclusive-gateway.svg" width="28"> | **ExclusiveGateway (XOR)** | Genau ein Pfad wird gewählt (Bedingungsauswertung). Optionaler Default-Flow. |
| <img src="readme-assets/bpmn-icons/parallel-gateway.svg" width="28"> | **ParallelGateway (AND)** | Alle Pfade werden parallel verfolgt (Token-Fork). Join wartet auf alle Tokens (JoinBarrier). |
| <img src="readme-assets/bpmn-icons/inclusive-gateway.svg" width="28"> | **InclusiveGateway (OR)** | Alle Pfade mit `true`-Bedingung werden parallel verfolgt. Join wartet auf erwartete Tokens. |
| <img src="readme-assets/bpmn-icons/event-based-gateway.svg" width="28"> | **EventBasedGateway** | Execution pausiert bis genau eines der Ziel-Catch-Events (Timer/Message) auslöst. |

### Intermediate Events

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/timer-catch-event.svg" width="28"> | **TimerCatchEvent** | Pausiert den Prozess bis ein Timer abläuft. Unterstützt Duration, AbsoluteDate, Cron und Repeating Intervals. Wird automatisch vom Timer-Scheduler verarbeitet. |
| <img src="readme-assets/bpmn-icons/message-catch-event.svg" width="28"> | **MessageCatchEvent** | Pausiert den Prozess bis eine passende Nachricht via `POST /api/message` korreliert wird. |
| <img src="readme-assets/bpmn-icons/boundary-timer-event.svg" width="28"> | **BoundaryTimerEvent** | An einen Task angeheftetes Timer-Event (interrupting/non-interrupting). Timer wird bei Task-Abschluss automatisch storniert. |
| <img src="readme-assets/bpmn-icons/boundary-message-event.svg" width="28"> | **BoundaryMessageEvent** | An einen Task angeheftetes Message-Event (interrupting/non-interrupting). Wartet asynchron auf externe Nachrichten. |
| <img src="readme-assets/bpmn-icons/boundary-error-event.svg" width="28"> | **BoundaryErrorEvent** | Fängt BPMN-Fehler (`errorCode`) eines ServiceTasks ab und leitet auf einen alternativen Pfad. |

### Aktivitäten & Sub-Prozesse

| BPMN | Element | Beschreibung |
|:---:|---|---|
| <img src="readme-assets/bpmn-icons/call-activity.svg" width="34"> | **CallActivity** | Ruft eine andere Prozessdefinition auf (`calledElement`). Variablen werden propagiert. |
| <img src="readme-assets/bpmn-icons/embedded-subprocess.svg" width="34"> | **EmbeddedSubProcess** | Eingebetteter Sub-Prozess (wird in den Graph geflattened). |
| <img src="readme-assets/bpmn-icons/subprocess-end-event.svg" width="28"> | **SubProcessEndEvent** | Internes End-Event eines Embedded-Sub-Process (generiert beim Flattening). |

### Zusätzliche Konzepte

| Feature | Beschreibung |
|---------|-------------|
| **Conditional Flows** | Kanten mit Bedingungen (`amount > 100`, `status == 'approved'`). Operatoren: `==`, `!=`, `>`, `>=`, `<`, `<=`, Truthy-Checks. |
| **Execution Listeners** | Start-/End-Scripts auf Nodes (Rhai). Können Variablen lesen und mutieren. |
| **Scope Event Listeners** | Timer-/Message-/Error-Event-Sub-Prozesse auf Scope-Ebene (interrupting/non-interrupting). |
| **Datei-Variablen** | Upload/Download von Dateien als Prozessvariablen via NATS Object Store. |
| **Message Correlation** | Matching über `messageName` + optionalem `businessKey`. |
| **BPMN Error Handling** | ServiceTasks melden Fehler via `bpmnError`. Routing an passendes `BoundaryErrorEvent`. |
| **Detail-Historie** | Lückenloses Event-Log mit Diffs, Snapshots und Aktoren (`User`, `Engine`, `Timer`, `ServiceWorker`). |
| **Persistente Wait-States** | Timer, Messages, User/Service Tasks überleben Server-Neustarts via NATS KV. |
| **Structured JSON Logging** | Konfigurierbar via `tracing-subscriber` mit JSON-Feature und `RUST_LOG` Filter. |

### Abweichungen vom BPMN 2.0 Standard

Aus Performance- und Architekturgründen (Keep-It-Simple) weicht bpmninja in einigen Punkten vom strikten BPMN 2.0 Standard ab:

- **Service Tasks (External Task Pattern):** Anstatt synchron Code innerhalb der Engine auszuführen, pausieren `Service Tasks` die Ausführung. Sie stellen den Task asynchron in eine Fetch-And-Lock-Queue (ähnlich Camunda), von wo aus externe Worker den Task abrufen (`topic`-basiert) und via API den Abschluss melden.
- **Embedded Sub-Processes (Flattening):** Eingebettete Sub-Prozesse werden direkt beim Parsen aufgelöst und tief in den Hauptgraphen eingefügt (**Flattening**). Es gibt zur Laufzeit keine komplex verschachtelten Instanz-Strukturen, sondern nur direkte Knotenfolgen. Rücksprünge aus dem Sub-Prozess erfolgen über simulierte `SubProcessEndEvent`s in demselben Variablen-Scope.
- **Script Tasks:** Die Auswertung von Skripten erfolgt nicht per JavaScript oder Groovy, sondern nativ in Rust via **Rhai Engine**.
- **Multi-Instance (Parallel):** Statt gekapselte Execution-Scopes pro Iteration zu öffnen, erzeugt das Engine-Forking simple parallele Tokens auf demselben Task-Objekt innerhalb der globalen Instanzvariablen.

### Aktuell nicht unterstützte BPMN-Elemente

Die Engine orientiert sich an einem zweckmäßigen und performanten Kern-Feature-Set. Folgende BPMN-Elemente werden derzeit **nicht** unterstützt und führen beim Deployment zu Parser-Fehlern oder werden vollständig ignoriert:

- **Weitere Task-Typen:** `BusinessRuleTask` (kein DMN-Support), `ManualTask`, `ReceiveTask`.
- **Spezifische Intermediate/Boundary Events:** `SignalEvent`, `EscalationEvent`, `CompensationEvent`, `CancelEvent`, `LinkEvent`.
- **Erweiterte Sub-Prozesse:** `Transaction Sub-Process`, `Ad-Hoc Sub-Process`.
- **Spezialisierte Gateways:** `Complex Gateway`.
- **Data Objects / Data Stores:** Visuelle Datenobjekte und Assoziationen (`Data Input/Output Association`) werden ignoriert. Der Datenaustausch erfolgt ausnahmslos über den JSON-Variablen-State (`HashMap<String, serde_json::Value>`).

---

## Architektur

> Ausführliche Dokumentation mit 8 Mermaid-Diagrammen: **[docs/architecture.md](docs/architecture.md)**

```mermaid
flowchart TD
    classDef core fill:#e2e8f0,stroke:#64748b,stroke-width:2px,color:#0f172a;
    classDef server fill:#bae6fd,stroke:#0284c7,stroke-width:2px,color:#0c4a6e;
    classDef persistence fill:#bbf7d0,stroke:#16a34a,stroke-width:2px,color:#14532d;
    classDef desktop fill:#fef08a,stroke:#ca8a04,stroke-width:2px,color:#713f12;
    classDef agent fill:#fbcfe8,stroke:#db2777,stroke-width:2px,color:#831843;
    classDef storage fill:#f0fdf4,stroke:#16a34a,stroke-width:1px,color:#14532d;

    subgraph "Clients"
        UI["desktop-tauri<br>(Tauri + React + bpmn-js)"]:::desktop
        Agent["agent-orchestrator<br>(External Workers)"]:::agent
        ExtMsg["External Systems<br>(Messages / Timers)"]:::agent
    end

    subgraph "Server Layer"
        Axum["engine-server<br>(Axum REST API)"]:::server
    end

    subgraph "Core Engine"
        Parser["bpmn-parser<br>(XML → ProcessDefinition)"]:::core
        Engine["engine-core<br>(State Machine / Tokens)"]:::core
        Trait["WorkflowPersistence<br>(Trait)"]:::core
    end

    subgraph "Storage"
        NatsImpl["persistence-nats"]:::persistence
        Nats[("NATS JetStream<br>KV + Object Store")]:::storage
    end

    UI -- "HTTP REST" --> Axum
    Agent -- "fetchAndLock / complete" --> Axum
    ExtMsg -- "POST /api/message" --> Axum
    Axum --> Parser
    Axum --> Engine
    Engine -. "uses" .-> Trait
    Trait -. "implemented by" .-> NatsImpl
    NatsImpl --> Nats
```

---

## Schnellstart

### Voraussetzungen

**Variante A: Devbox** (empfohlen)
```bash
# Installiert automatisch Rust, Node.js und NATS
devbox shell
```

**Variante B: Manuell**
- Rust (via `rustup`)
- Node.js ≥ 18
- Docker & Docker Compose

### Build, Test & Lint

| Aktion | Devbox | Shell |
|--------|--------|-------|
| **Build** | `devbox run build` | `cargo build --workspace` |
| **Test** | `devbox run test` | `cargo test --workspace` |
| **Lint** | `devbox run lint` | `cargo clippy --workspace -- -D warnings` |
| **Format** | `devbox run fmt` | `cargo fmt --all --check` |

### Engine-Server starten

```bash
# 1. NATS starten
docker compose up -d nats

# 2. Engine-Server starten
cargo run -p engine-server
```

Der Server läuft auf `http://localhost:8081`.

#### Umgebungsvariablen

| Variable | Default | Beschreibung |
|----------|---------|-------------|
| `NATS_URL` | `nats://localhost:4222` | NATS Server URL |
| `PORT` | `8081` | HTTP Server Port |
| `TIMER_INTERVAL_MS` | `1000` | Timer-Scheduler Polling-Intervall (ms) |

---

## REST API

> Vollständige OpenAPI 3.0 Spezifikation: **[docs/openapi.yaml](docs/openapi.yaml)** | 🌐 **[API Portal (Redoc)](https://maatini.github.io/bpmninja/)** *(benötigt aktives GitHub Pages Deploy via /docs)*

### Definitionen

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `POST` | `/api/deploy` | BPMN-Definition deployen (max. 10MB) |
| `GET` | `/api/definitions` | Alle Definitionen auflisten |
| `GET` | `/api/definitions/:id/xml` | BPMN-XML einer Definition abrufen |
| `DELETE` | `/api/definitions/:id` | Definition löschen (`?cascade=true` für inkl. Instanzen) |
| `DELETE` | `/api/definitions/bpmn/:bpmn_id` | Alle Versionen einer BPMN-ID löschen |

### Instanzen

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `POST` | `/api/start` | Instanz starten (mit `definition_key`) |
| `POST` | `/api/start/latest` | Instanz der neuesten Version starten (mit `bpmn_process_id`) |
| `GET` | `/api/instances` | Alle Instanzen auflisten |
| `GET` | `/api/instances/:id` | Instanz-Details abrufen |
| `DELETE` | `/api/instances/:id` | Instanz löschen |
| `PUT` | `/api/instances/:id/variables` | Variablen aktualisieren |

### User Tasks

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `GET` | `/api/tasks` | Alle pending User Tasks auflisten |
| `POST` | `/api/complete/:id` | User Task abschließen |

### Service Tasks (Camunda-kompatibel)

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `GET` | `/api/service-tasks` | Alle Service Tasks auflisten |
| `POST` | `/api/service-task/fetchAndLock` | Tasks abrufen und sperren (Long-Polling) |
| `POST` | `/api/service-task/:id/complete` | Task erfolgreich abschließen |
| `POST` | `/api/service-task/:id/failure` | Task als fehlgeschlagen markieren |
| `POST` | `/api/service-task/:id/extendLock` | Lock verlängern |
| `POST` | `/api/service-task/:id/bpmnError` | BPMN-Fehler melden |

### Dateien

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `POST` | `/api/instances/:id/files/:var` | Datei hochladen (multipart) |
| `GET` | `/api/instances/:id/files/:var` | Datei herunterladen |
| `DELETE` | `/api/instances/:id/files/:var` | Dateivariable löschen |

### Events & Messages

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `POST` | `/api/message` | Nachricht korrelieren |
| `POST` | `/api/timers/process` | Abgelaufene Timer manuell verarbeiten |

### Monitoring & Health

| Methode | Pfad | Beschreibung |
|---------|------|-------------|
| `GET` | `/api/health` | Liveness Check → `200 OK` |
| `GET` | `/api/ready` | Readiness Check (prüft NATS-Verbindung) |
| `GET` | `/api/info` | Backend-Informationen (Typ, NATS-URL, Status) |
| `GET` | `/api/monitoring` | Engine-Statistiken (Instanzen, Tasks, Storage, Fehler) |
| `GET` | `/api/monitoring/buckets/:bucket/entries` | KV-Bucket Einträge auflisten |
| `GET` | `/api/monitoring/buckets/:bucket/entries/:key` | Einzelnen KV-Eintrag laden |
| `GET` | `/api/instances/:id/history` | Event-Historie einer Instanz |
| `GET` | `/api/instances/:id/history/:eid` | Einzelnes History-Event |

### Fehlerbehandlung

Alle Fehler folgen einem einheitlichen JSON-Format:

```json
{ "error": "Human-readable error message" }
```

| HTTP-Code | Bedeutung |
|-----------|-----------|
| `400` | Ungültige Anfrage (Bad XML, ungültige UUID, fehlende Felder) |
| `404` | Ressource nicht gefunden (Definition, Instanz, Task, Node) |
| `409` | Konflikt (Task nicht pending, bereits gesperrt, bereits abgeschlossen) |
| `500` | Interner Serverfehler |

---

## Desktop-Anwendung (UI)

Die Tauri-App verbindet sich über HTTP mit dem `engine-server`.

> **Voraussetzung**: Ein laufendes Backend (NATS + Engine-Server). Standardmäßig erwartet die App das Backend auf `http://localhost:8081` (konfigurierbar via `ENGINE_API_URL`).

### 1. Systemvoraussetzungen (Backend) ausführen

Speichere folgende `docker-compose.yml` lokal auf deinem Rechner und starte sie via `docker compose up -d`. Das fertige Backend-Image wird dabei automatisch von der GitHub Container Registry bezogen:

```yaml
services:
  nats:
    image: nats:alpine
    command: ["--js", "--sd", "/data"]
    ports:
      - "4222:4222"
      - "8222:8222"
    volumes:
      - nats-data:/data

  engine-server:
    image: ghcr.io/maatini/bpmninja/engine-server:latest
    ports:
      - "8081:8081"
    environment:
      - PORT=8081
      - NATS_URL=nats://nats:4222
    depends_on:
      - nats

volumes:
  nats-data:
```

### 2. Binary-Release der Desktop-App installieren

Die fertigen Apps findest du auf der [GitHub Releases Seite](https://github.com/maatini/bpmninja/releases).

*   **macOS (.dmg):** Öffne die Datei und ziehe das Icon in deinen `Applications`/`Programme`-Ordner. _Hinweis:_ Da Open-Source Apps meist nicht kostenpflichtig signiert sind, kann eine Gatekeeper-Warnung auftreten. Mache einen **Rechtsklick** auf die App und wähle **"Öffnen"**.
*   **Windows (.exe / .msi):** Führe den Installer per Doppelklick aus. Falls eine Microsoft SmartScreen-Warnung erscheint, klicke auf "Weitere Informationen" und dann auf "Trotzdem ausführen".
*   **Linux (.AppImage / .deb):** Installiere das `.deb` Paket via `sudo dpkg -i package.deb`. Nutzt du das `.AppImage`, muss dieses ggf. mit `chmod +x app.AppImage` vorher ausführbar gemacht werden.

### 3. Ausführung aus dem Quellcode (Für Entwickler)

Wer die UI direkt aus dem Source Repository ausführen möchte:

```bash
# Devbox
devbox run ui:dev

# Oder manuell
cd desktop-tauri && npm install && npm run tauri dev
```

---

## Docker Compose

Startet NATS + Engine-Server als Container:

```bash
# Devbox
devbox run engine:docker

# Oder manuell
docker compose up --build
```

Services erreichbar unter `localhost:8081` (API) und `localhost:4222` (NATS).

---

## Test-Metriken

> Ermittelt via `cargo test --workspace` am 07.04.2026 — **175 Tests, 0 Fehler**

### Workspace-Übersicht

| Crate | Unit | E2E | Gesamt |
|-------|------|-----|--------|
| **engine-core** | 105 | 5 | 110 |
| **bpmn-parser** | 27 | — | 27 |
| **persistence-nats** | 2 | — | 2 |
| **engine-server** | — | 36 | 36 |
| **Gesamt** | **134** | **41** | **175** ✅ |

### engine-core Breakdown (110 Tests)

| Modul | Tests | Abdeckung |
|-------|-------|-----------|
| `engine::tests` | 56 | State Machine, Gateways, User/Service Tasks, Boundary Events, Call Activities, EventBasedGateway, Timers, Messages |
| `engine::stress_tests` | 24 | Throughput, Gateway-Korrektheit, Crash Recovery, Concurrency, Race Conditions, Memory, Infinite Loops |
| `model::tests` | 17 | ProcessDefinition Builder, Token-Serialisierung, Validation |
| `history::tests` | 5 | Diff-Berechnung, Human-Readable Text |
| `condition::tests` | 3 | Bedingungsevaluierung anhand von Token-Variablen |
| Integration Tests | 5 | BPMN-Compliance, Complex Gateways |

### bpmn-parser Tests (27 Tests)

| Bereich | Tests | Abdeckung |
|---------|-------|-----------|
| Basis-Parsing | 6 | Simple BPMN, Conditional Flows, XOR Gateway, Timer Start, Interleaved Output, Execution Listeners |
| Gateways | 3 | Parallel, Inclusive, Event-Based |
| Events | 5 | MessageStart, MessageCatch, ErrorEnd, TimerCatch, BoundaryTimer |
| Boundary Events | 2 | BoundaryTimer, BoundaryError |
| ISO 8601 Timer | 4 | TimeDate, CronCycle, RepeatingInterval, Duration-Reject |
| Task-Typen | 3 | ScriptTask, SendTask, IntermediateMessageThrow |
| Sub-Prozesse | 2 | EventSubProcess, RegularSubProcess |
| Sonstiges | 1 | TerminateEndEvent |

### engine-server E2E Tests (36 Tests, 12 Dateien)

| Testdatei | Tests | Abdeckung |
|-----------|-------|-----------|
| `e2e_deploy.rs` | 3 | Deploy, Start, Parallel Gateway |
| `e2e_file_variables.rs` | 3 | File Upload, Task Completion mit Files, Multi-File + Delete |
| `e2e_files.rs` | 1 | Upload/Download/Delete Lifecycle |
| `e2e_gateways.rs` | 1 | Parallel Gateway über HTTP |
| `e2e_history.rs` | 1 | History-Generierung und -Abfrage |
| `e2e_lifecycle.rs` | 6 | Instanz löschen, Definition löschen, Unbekannte Instanzen, Timer verarbeiten, Message korrelieren |
| `e2e_monitoring.rs` | 4 | Health, Ready (verbunden/unverbunden), Info, Monitoring Stats |
| `e2e_service_tasks.rs` | 7 | List Service Tasks, FetchAndLock, ExtendLock, Complete, Complete with Failure, BPMN-Error, Lock Conflict |
| `e2e_start_errors.rs` | 4 | Invalid UUID, Unknown Definition, Unknown BPMN-ID, Timer-Start Rejection |
| `e2e_stress.rs` | 2 | Concurrent Deployments, Concurrent Starts (multi-thread) |
| `e2e_variables.rs` | 1 | Variable-Updates mid-execution |
| `e2e_versioning.rs` | 3 | Version-Inkrement, Start-Latest, Instance-Isolation |

### Code-Statistiken

| Bereich | Dateien | LoC |
|---------|---------|-----|
| engine-core (Lib) | 25 | 7.708 |
| engine-core (Tests) | 2 | 3.628 |
| bpmn-parser | 4 | 2.039 |
| persistence-nats | 5 | 1.130 |
| engine-server (Lib) | 12 | 1.548 |
| engine-server (E2E Tests) | 12 | 1.934 |
| desktop-tauri (TypeScript + CSS) | 38 | 5.186 |
| desktop-tauri (Rust Backend) | 10 | 623 |
| **Rust Workspace** | **60** | **~18.610** |
| **Projekt Gesamt** | **~108** | **~23.796** |

### Mutation Score (Stichprobe)
Eine Stichprobe via [`cargo-mutants`](https://mutants.rs) auf geschäftskritischen Komponenten (`condition.rs`, `script_runner.rs`, `history.rs`) ergab einen initialen **Mutation Score von ~87%** (41 von 47 Mutanten durch Tests erkannt). Eine vollständige Evaluierung aller 945 Mutanten (Laufzeit ~3.5h) ist für spätere CI/CD-Phasen vorgesehen.

---

## Roadmap

| Feature | Status |
|---------|--------|
| Embedded Subprozesse (BPMN Scopes) | ✅ Implementiert |
| Event-Based Gateway | ✅ Implementiert |
| Structured JSON Logging (`tracing-subscriber` + JSON) | ✅ Implementiert |
| Multi-Node Cluster (NATS-basiertes Token-Locking) | 🔲 Geplant |
| OIDC/OAuth2 Middleware | 🔲 Geplant |
| Prometheus Metrics Endpoint | 🔲 Geplant |

---

## Lizenz

Dieses Projekt ist unter einer der folgenden Lizenzen lizenziert, nach deiner Wahl:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

