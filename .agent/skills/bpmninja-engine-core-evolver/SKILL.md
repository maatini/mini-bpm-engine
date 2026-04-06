---
name: bpmninja-engine-core-evolver
description: Selbst-evolvierender Skill zur kontinuierlichen Verbesserung des engine-core Crates (State-Machine, Token-Registry, Gateway-Routing, Condition-Evaluator, Rhai-Script-Engine). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle). Fokussiert auf Token-Execution, Lock-Free Concurrency, BPMN-Compliance und Performance.
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, engine-core, bpmn, token-engine, concurrency, performance, evoskills]
requires: [cargo, devbox, rust-nightly-for-bench]
---

# EvoSkills Self-Evolving Engine Core Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am `engine-core` Crate arbeitest:
- Neue BPMN-Elemente hinzufügen (Complex Gateway, Data Objects, Multi-Instance, etc.)
- Token-Execution, State-Machine oder Concurrency optimieren
- Rhai-Scripting oder Condition-Evaluator verbessern
- Performance-Benchmarks oder Testabdeckung steigern
- Persistenz-Integration (NATS Trait) oder History-Tracking erweitern

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
Jede Verbesserung **muss** diesen 4-Schritt-Zyklus durchlaufen:

1. **Skill Generator**  
   Analysiere `engine-core/Cargo.toml`, `src/lib.rs`, `src/engine.rs`, `src/gateway.rs`, `src/condition.rs`, `src/script.rs`, Tests und aktuelle Benchmarks.  
   Generiere konkrete, diff-fähige Rust-Änderungen.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Lock-Free Safety & Concurrency-Korrektheit
   - BPMN 2.0 Spec Compliance
   - Performance-Impact (keine Regressionen)
   - Testbarkeit & Fuzzing
   - Rhai-Integration & Error-Handling
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p engine-core`
   - `cargo clippy -p engine-core --all-targets -- -D warnings`
   - Performance-Benchmarks (criterion)
   - Compliance-Checks
   - Oracle gibt nur **PASS / FAIL + Metriken** (z. B. tokens/sec, coverage).

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade `engine-core` spezifische Dateien und führe `devbox run test -p engine-core` aus.
- Erstelle Core-Health-Report (Coverage, Benchmark-Ergebnisse, offene BPMN-Gaps).

### Phase 2 – Improvement Generation (Prioritäten)
- Hinzufügen fehlender BPMN-Elemente (Complex Gateway, Data Objects/Store, Multi-Instance)
- Token-Engine Optimierungen (bessere DashMap-Nutzung, reduzierte Allokationen)
- Erweiterte Rhai-Integration (Execution Listeners, Scope-Variablen)
- Lock-Free Wait-State Queues verbessern
- Fuzzing & Property-Based Testing für State-Machine
- Bessere Error-Propagation & History-Tracking

### Phase 3 – Implementation & Verification
- Generiere Rust-Code-Änderungen.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere Tests und Benchmarks.

### Phase 4 – Documentation & Handover
- Aktualisiere `engine-core/README.md` (falls vorhanden) und crate-Doku.
- Erweitere `.agent/skills/` mit neuen Sub-Skills (self-referential).

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump im Cargo.toml.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Features müssen mit Unit- + Integration-Tests abgedeckt sein.
- Performance darf nicht regressieren (Benchmark-Oracle muss bestehen).
- Devbox-Umgebung bleibt primär.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Verbesserungen ohne Oracle-Validierung.
- Ignorieren von Multi-Threading-Edge-Cases oder Wait-State-Persistenz.
- Rhai-Scripts ohne Error-Handling.

## Beispiel-Aufruf
„Aktiviere bpmninja-engine-core-evolver und implementiere Complex Gateway Support mit vollständiger Testabdeckung.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für engine-core.
