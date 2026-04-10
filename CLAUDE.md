# CLAUDE.md — BPMNinja Project Guide

## Project Mission
BPMNinja ist eine moderne, hochperformante, open-source BPMN 2.0 Workflow-Engine in Rust.  
Ziel: Eine Camunda-kompatible, aber deutlich schlankere Alternative mit Fokus auf  
- Token-basierte Ausführung  
- Lock-free Concurrency (DashMap)  
- Native NATS JetStream Persistence  
- Schöne Tauri Desktop-UI mit bpmn-js  
- Einfache External Task Worker (TypeScript)  

**Sprache im Projekt:** Deutsch (README, Commits, Issues, PRs, Dokumentation und Claude-Kommunikation).

## Tech-Stack

| Layer              | Technologie                          | Version      |
|--------------------|--------------------------------------|--------------|
| Rust Edition       | 2024 (Tauri: 2021)                   | stable       |
| Async Runtime      | Tokio                                | 1.x          |
| Web Framework      | Axum                                 | 0.8          |
| XML Parser         | quick-xml                            | 0.39         |
| Scripting          | Rhai                                 | 1.19         |
| Persistence        | NATS JetStream (async-nats)          | 0.47         |
| Concurrency        | DashMap                              | 6.x          |
| Desktop            | Tauri 2.10 + React 19 + bpmn-js 18  | -            |
| Styling            | Tailwind CSS 4                       | -            |
| Testing            | Playwright, cargo-fuzz, cargo-mutants| -            |

## Workspace-Struktur

```
engine-core/                  # Kern-Engine (Token-Execution, Gateways, Events, Rhai)
bpmn-parser/                  # BPMN 2.0 XML → ProcessDefinition
persistence-nats/             # NATS JetStream Adapter (KV, Object Store, Streams)
engine-server/                # Axum REST API + Incident Management
agent-orchestrator/           # Beispiel External Task Worker
desktop-tauri/                # Tauri Desktop-App (React + bpmn-js + Live-Tracking)
bpmn-ninja-external-task-client/ # TypeScript Camunda-kompatibler Client
api-spec/                     # OpenAPI-Spezifikation
fuzz/                         # Fuzz-Targets (Parser, Rhai, Cron etc.)
docs/                         # OpenAPI, Architektur-Diagramme
```

## MCP-Server (wichtig!)

```bash
# GitHub (Issues, PRs, Releases)
claude mcp add github -- npx -y @modelcontextprotocol/server-github

# Cargo (Rust-Entwicklung)
cargo install cargo-mcp --locked
claude mcp add cargo-mcp -- cargo-mcp serve

# Tauri (Desktop-App)
npx -y install-mcp @hypothesi/tauri-mcp-server --client claude-code

# NATS (Persistence-Layer)
claude mcp add nats -e NATS_URL="nats://localhost:4222" -- npx -y @daanrongen/nats-mcp
```

Danach mit `claude mcp list` prüfen.

## Claude Workflow (unbedingt einhalten)

1. **Immer mit Plan Mode starten** (`/plan` oder Shift+Tab)
2. Plan von mir prüfen lassen → erst dann implementieren
3. Nach jeder großen Task: `/clear` oder neue Session
4. Bei Kontext-Problemen: `/compact`
5. Claude soll **nie** selbst `git push --force` oder READMEs ohne Auftrag ändern

## Wichtige Befehle

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check

# Desktop
cd desktop-tauri && npm install && npm run tauri dev
cd desktop-tauri && npx playwright test

# Docker + Server
docker compose up -d
cargo run -p engine-server
```

## Architektur-Regeln

- Hexagonale Architektur mit `port::WorkflowPersistence` Trait
- Token-basierte Ausführung (`tokens: HashMap<Uuid, Token>`)
- Sub-Prozesse werden beim Parsen geflattened (kein Runtime-Nesting)
- `BpmnElement` ist ein geschlossener Enum → exhaustive matching
- `NextAction` Enum steuert den Executor-Loop
- Rhai-Scripte: `let x = 1;` für neue Variablen, `x = 2;` für bestehende
- Compensation- und Escalation-Events voll unterstützt
- Multi-Instance Tasks und Incident Management implementiert

## Coding-Konventionen

- Rust 2024 Edition + Clippy mit `-D warnings`
- `thiserror` + `anyhow`, kein `unwrap()` in Prod
- Immer `tracing` statt `println!`
- Keine unnötigen `.clone()`
- Tests: `ProcessDefinitionBuilder` statt Raw-XML

## Nie machen

- Keine `unwrap()` in Produktionscode
- Keine `println!` / `eprintln!`
- Kein `#[allow(unused)]` oder `#[allow(clippy::...)]` in Commits
- Kein `git push --force` auf main
- Keine neuen Dateien ohne expliziten Auftrag (Edit > Write)
- Keine Mock-Datenbanken in Integration-Tests

## Git & Release Workflow

- Commits auf Deutsch, konventionell (`feat:`, `fix:`, `refactor:` etc.)
- PRs immer mit Linked Issue
- Releases nur über GitHub Releases + automatische Tauri-Builds
- Main-Branch ist immer stabil

## Versionierung

Alle Crates synchron auf Version **0.7.9** (aktuell).  
Git-Tags: `v0.7.9`
