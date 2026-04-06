---
name: bpmninja-engine-server-evolver
description: Selbst-evolvierender Skill für das engine-server Crate (Axum REST API, External Tasks, OIDC, Prometheus Metrics, Security). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, axum, engine-server, rest-api, external-tasks, oidc, prometheus, evoskills]
requires: [cargo, devbox]
---

# EvoSkills Self-Evolving Engine Server Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am engine-server Crate arbeitest: neue REST-Endpunkte, External Task API, OIDC-Auth, Prometheus Metrics, Rate-Limiting oder Security-Verbesserungen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert engine-server/src/, routes/, middleware/, Cargo.toml und aktuelle API-Tests.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - API-Sicherheit & Rate-Limiting
   - External-Task-Kompatibilität (Camunda)
   - OIDC + Metrics-Integration
   - Performance & Error-Handling
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p engine-server`
   - API-Integration-Tests
   - Clippy, Coverage, Metrics-Endpoint-Check
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Server-Dateien und führe `devbox run test -p engine-server` aus.
- Erstelle Server-Health-Report (Coverage, offene Endpunkte, Metrics-Status).

### Phase 2 – Improvement Generation (Prioritäten)
- Vollständige OIDC-Auth Middleware
- Prometheus Metrics-Endpoint + Grafana-Ready Labels
- Erweiterte External Task API (Claim, Complete, HandleError)
- Rate-Limiting & Security Headers
- OpenAPI/Swagger Dokumentation

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Rust-Code.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere Tests und Middleware.

### Phase 4 – Documentation & Handover
- Aktualisiere crate-Doku und API-Beispiele.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Endpunkte müssen mit Integration-Tests abgedeckt sein.
- Coverage > 90 % für das Server-Crate.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Security-Verbesserungen ohne echte API-Tests.
- Ignorieren von Camunda-kompatiblen External-Task-Feldern.
- Metrics ohne korrekte Labels oder ohne Oracle-Validierung.

## Beispiel-Aufruf
„Aktiviere bpmninja-engine-server-evolver und implementiere Prometheus Metrics + OIDC-Auth mit vollständiger Testabdeckung.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für das engine-server Crate.
