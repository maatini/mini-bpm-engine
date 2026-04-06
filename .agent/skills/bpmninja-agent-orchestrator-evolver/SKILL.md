---
name: bpmninja-agent-orchestrator-evolver
description: Selbst-evolvierender Skill für das agent-orchestrator Crate (External Task Worker, Polling, Heartbeat, Retry-Logik, Task-Claiming). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, agent, orchestrator, external-tasks, worker, polling, retry, evoskills]
requires: [cargo, devbox]
---

# EvoSkills Self-Evolving Agent Orchestrator Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am agent-orchestrator Crate arbeitest: External Task Worker, Task Polling, Heartbeat, Retry-Logik, Task-Claiming, Completion oder Resilience-Verbesserungen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert agent-orchestrator/src/, Cargo.toml und aktuelle Worker-Tests.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Polling-Effizienz & Heartbeat-Stabilität
   - Retry- & Backoff-Logik
   - Task-Claiming & Error-Handling
   - Skalierbarkeit & Resilience
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p agent-orchestrator`
   - Polling- & Retry-Tests
   - Clippy & Coverage
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Orchestrator-Dateien und führe `devbox run test -p agent-orchestrator` aus.
- Erstelle Orchestrator-Health-Report (Polling-Latenz, Retry-Erfolgsrate, offene Issues).

### Phase 2 – Improvement Generation (Prioritäten)
- Intelligentes Task Polling mit dynamischem Backoff
- Verbesserte Heartbeat- & Long-Polling-Logik
- Exponentieller Retry mit Jitter
- Bessere Error-Propagation & Dead-Letter-Queue
- Worker-Skalierung & Multi-Thread-Support

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Rust-Code.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere Tests und Konfiguration.

### Phase 4 – Documentation & Handover
- Aktualisiere Worker-Doku und Beispiele.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Features müssen mit Unit- + Integration-Tests abgedeckt sein.
- Coverage > 90 %.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Retry-Verbesserungen ohne echte Polling-Oracle.
- Ignorieren von Netzwerk-Timeouts oder Server-Seitigen Fehlern.
- Unkontrollierte Polling-Frequenzen ohne Backoff.

## Beispiel-Aufruf
„Aktiviere bpmninja-agent-orchestrator-evolver und implementiere exponentiellen Retry mit Jitter und verbessertem Heartbeat.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und macht den External Task Worker von bpmninja immer robuster und produktionsreif.
