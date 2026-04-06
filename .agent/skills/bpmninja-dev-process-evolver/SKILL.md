---
name: bpmninja-dev-process-evolver
description: Selbst-evolvierender Meta-Skill zur kontinuierlichen Verbesserung des gesamten Entwicklungsprozesses der BPMNinja Rust-Engine (Cargo Workspace, Devbox, CI/CD, Testing, BPMN-Compliance, Release-Automatisierung, Metrics, Clustering). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, bpmn, devops, ci-cd, self-evolving, evoskills]
requires: [cargo, devbox, docker, github-actions]
---

# EvoSkills Self-Evolving Dev Process Optimizer für bpmninja

## Wann diesen Skill aktivieren?
Immer wenn du am bpmninja-Repo arbeitest und eine der folgenden Aufgaben hast:
- Entwicklungsprozess analysieren und verbessern
- CI/CD, Testing, Release-Automatisierung, Code-Qualität, Dokumentation, Performance-Benchmarks
- Roadmap-Items umsetzen (Clustering, OIDC, Prometheus, vollständige BPMN-Compliance)

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
Jede Verbesserung **muss** diesen 4-Schritt-Zyklus durchlaufen:

1. **Skill Generator**  
   Analysiere aktuellen Zustand (README, Cargo.toml, .github/workflows, devbox.json, docs/architecture.md, Test-Ergebnisse).  
   Generiere konkrete, umsetzbare Verbesserungsvorschläge (Code-Änderungen, neue Workflows, Scripts).

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik mit folgenden Kriterien (Punkte 0–10 vergeben):
   - Kompatibilität mit bestehendem Rust-Workspace & Devbox
   - Sicherheit / keine Breaking Changes
   - ROI (Aufwand vs. Nutzen)
   - Testbarkeit / Messbarkeit
   - BPMN-Engine-spezifische Anforderungen (Concurrency, NATS, Tauri)
   Gib **konkretes, actionables Feedback** und schlage Verbesserungen vor.

3. **External Oracle (binär)**  
   Führe echte Validierung aus:
   - `devbox run test` → alle Tests grün?
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo build --release --workspace`
   - Coverage-Report + Benchmark (siehe scripts/)
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Vorschlag finalisieren und PR-ready Code generieren.  
   Wiederhole bis Metriken (Test-Coverage, Build-Time, Release-Frequenz) sich messbar verbessern.

## Schritt-für-Schritt Workflow (Progressive Disclosure)

### Phase 1 – Assessment
- Lies alle relevanten Dateien (Cargo.toml, devbox.json, .github/workflows/*, docs/*).
- Führe `devbox run test`, `cargo test --workspace`, `cargo clippy` aus.
- Erstelle Dev-Health-Report (Coverage, Build-Zeit, offene TODOs aus Roadmap).

### Phase 2 – Improvement Generation
Mögliche Verbesserungsbereiche (priorisiere nach Impact):
- Automatisierte Coverage-Reports + Codecov/GitHub
- Semantic Release + Changelog
- Performance-Benchmarks für Token-Engine (stress tests)
- Automatisierte BPMN-Compliance-Checks (gegen Spec)
- Prometheus-Metrics-Endpoint + Grafana-Dashboard
- OIDC-Auth Middleware
- Multi-Node NATS Clustering
- Tauri-spezifische Build-Optimierungen & Auto-Updates
- bessere Architecture-Docs (mermaid diagrams)

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Code / neue Dateien.
- Führe Surrogate + Oracle Loop aus (mind. 3 Runden).
- Erstelle oder aktualisiere `.github/workflows/` und `scripts/`.

### Phase 4 – Documentation & Handover
- Aktualisiere README und `docs/architecture.md`.
- Füge neue Skills in `.agent/skills/` hinzu (self-referential evolution).

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump.
- Immer Rust 1.XX stable (aktuellste stabile Version).
- Alle neuen Features müssen mit Tests abgedeckt sein.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Keine externen Dependencies ohne klare Begründung und Security-Check.
- Alle Änderungen müssen als PR mit EvoSkills-Trace (welche Runden durchlaufen) dokumentiert werden.

## Häufige Fehler vermeiden
- Nur „nice-to-have“-Vorschläge ohne Oracle-Validierung.
- Ignorieren von Tauri-/NATS-spezifischen Edge-Cases.
- Manuelle Schritte beibehalten statt zu automatisieren.

## Beispiel-Aufruf
„Aktiviere bpmninja-dev-process-evolver und optimiere den CI/CD-Prozess für Releases und Coverage-Reporting.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für das bpmninja-Projekt.
