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
* `desktop-tauri`: Eine Tauri-Desktop-Anwendung, die mit der Workflow-Engine interagiert.
* `agent-orchestrator`: Ein Crate zur Orchestrierung von externen Agenten/Workern, die mit der Engine interagieren.

## Unterstützte BPMN-Elemente

| Element | Beschreibung |
|---|---|
| **StartEvent** | Einfacher Startpunkt — Prozess wird sofort gestartet. |
| **TimerStartEvent** | Timer-gesteuerter Start nach einer konfigurierbaren Dauer. |
| **EndEvent** | Endpunkt — Prozessinstanz wird als abgeschlossen markiert. |
| **ServiceTask** | Tasks, die von externen Workern (z.B. agent-orchestrator) per fetch-and-lock abgearbeitet werden. |
| **UserTask** | Erstellt einen Pending-Task, der extern abgeschlossen werden muss. |
| **ExclusiveGateway (XOR)** | Genau ein ausgehender Pfad wird gewählt (Bedingungsauswertung). Optionaler Default-Flow. |
| **InclusiveGateway (OR)** | Alle Pfade, deren Bedingung `true` ergibt, werden parallel verfolgt (Token-Forking). |

### Zusätzliche Konzepte

* **Conditional Sequence Flows** — Kanten können Bedingungsausdrücke tragen (z.B. `amount > 100`, `status == 'approved'`). Der integrierte Condition-Evaluator unterstützt `==`, `!=`, `>`, `>=`, `<`, `<=` sowie Truthy-Checks.
* **Execution Listeners** — Nodes können Start- und End-Scripts besitzen, die Prozessvariablen lesen und mutieren (z.B. `x = x * 2; if x > 10 { result = "big" }`).

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

    subgraph "Clients / External"
        UI["desktop-tauri\n(Desktop App)"]:::desktop
        Agent["agent-orchestrator\n(External Workers)"]:::agent
    end

    subgraph "Server Layer"
        Axum["engine-server\n(Axum HTTP REST API)"]:::server
    end

    subgraph "Core Workflow Engine"
        Engine["engine-core\n(Token & State Execution)"]:::core
        Parser["bpmn-parser\n(XML to Rust Structs)"]:::core
    end

    subgraph "Storage"
        Nats[(persistence-nats\nNATS JetStream)]:::persistence
    end

    %% Connections
    UI -- "HTTP REST API" --> Axum
    Agent -- "HTTP Fetch/Lock" --> Axum
    Axum -- "Calls" --> Engine
    Engine -- "Parses" --> Parser
    Engine -- "Stores State & Events" --> Nats
```

## Starten des Engine-Servers

Um den HTTP-REST-API-Server zu starten: 

```bash
# NATS starten (falls Persistenz genutzt werden soll)
docker-compose up -d nats

# Engine-Server ausführen
cargo run -p engine-server
```

Der Server lauscht standardmäßig auf `http://localhost:8081`.

### Endpunkte
* `POST /api/deploy` - Eine BPMN-Definition bereitstellen
* `POST /api/start` - Eine neue Prozessinstanz starten
* `GET /api/tasks` - Alle ausstehenden Benutzer-Tasks (User Tasks) auflisten
* `POST /api/complete/:id` - Einen Benutzer-Task abschließen
* `GET /api/instances` - Alle Prozessinstanzen auflisten
* `GET /api/instances/:id` - Details einer einzelnen Instanz abrufen
* `PUT /api/instances/:id/variables` - Variablen einer Prozessinstanz aktualisieren
* `DELETE /api/instances/:id` - Eine Prozessinstanz löschen
* `DELETE /api/definitions/:id` - Eine Prozessdefinition löschen (Query `?cascade=true` zum Mitlöschen aller zugehörigen Instanzen)

#### Service Tasks
* `POST /api/service-task/fetchAndLock` - Tasks für Worker abrufen und sperren (inkl. Long-Polling)
* `POST /api/service-task/:id/complete` - Einen Service Task erfolgreich abschließen
* `POST /api/service-task/:id/failure` - Einen Service Task als fehlgeschlagen markieren
* `POST /api/service-task/:id/extendLock` - Die Sperrdauer eines Tasks verlängern
* `POST /api/service-task/:id/bpmnError` - Einen BPMN-Fehler für einen Task melden

## Ausführen der Desktop-Anwendung

Die `mini-bpm-desktop` Anwendung fungiert als leichtgewichtiger "Thin Client", der sich ausschließlich über HTTP mit der `engine-server` Instanz verbindet. Sie baut keine eigene NATS-Verbindung auf und besitzt keine eingebettete Engine mehr.

```bash
# Starten für das Development (erfordert Node.js und npm)
cd desktop-tauri && npm install && npm run tauri dev
```

*Hinweis: Stelle sicher, dass `engine-server` läuft, bevor die App gestartet wird. Du kannst den API-Endpunkt über die Umgebungsvariable `ENGINE_API_URL` konfigurieren.*

### Tauri-Kommandos
Das Frontend der Desktop-Anwendung nutzt folgende Tauri-Kommandos zur Interaktion mit dem Backend:
* Deployment & Start: `deploy_definition`, `deploy_simple_process`, `start_instance`
* Instanzen: `list_instances`, `get_instance_details`, `update_instance_variables`, `delete_instance`
* Tasks: `get_pending_tasks`, `complete_task`
* Definitionen: `list_definitions`, `get_definition_xml`, `delete_definition`

## Docker Compose

Die gesamte Infrastruktur (NATS und `engine-server`) kann wie folgt gestartet werden:

```bash
docker-compose up --build
```
