---
name: bpmninja-rust-test-evolver
description: Selbst-evolvierender Skill zur kontinuierlichen Verbesserung des gesamten Rust-Testprozesses im bpmninja-Workspace (Unit, Integration, E2E, Fuzzing, Property-Based Testing, Coverage, Benchmarks). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle). Fokussiert auf Testqualität, Geschwindigkeit, Flakiness-Reduktion und BPMN-spezifische Testabdeckung.
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, testing, cargo-test, coverage, fuzzing, benchmarks, evoskills]
requires: [cargo, devbox, cargo-llvm-cov, cargo-fuzz, criterion]
---

# EvoSkills Self-Evolving Rust Test Process Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am Testprozess des bpmninja-Workspaces arbeitest:
- Testabdeckung steigern (aktuell ~80-90 %)
- Neue Tests für fehlende BPMN-Elemente schreiben
- Flaky Tests eliminieren
- Fuzzing & Property-Based Testing einführen
- Benchmarks optimieren oder automatisieren
- CI-Test-Zeit reduzieren

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
Jede Test-Verbesserung **muss** diesen 4-Schritt-Zyklus durchlaufen:

1. **Skill Generator**  
   Analysiere alle `tests/`, `benches/`, `Cargo.toml` (dev-dependencies), `.github/workflows/test.yml` und aktuelle Coverage/Benchmark-Ergebnisse.  
   Generiere neue Tests, Test-Helpers oder Test-Workflow-Änderungen.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Test-Qualität & Determinismus
   - Abdeckung kritischer Pfade (Token-Engine, Concurrency, NATS)
   - Performance des Test-Runs
   - Wartbarkeit & BPMN-Spec-Compliance
   - Vermeidung von False-Positives
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `devbox run test --workspace`
   - Coverage ≥ 92 %
   - Fuzzing-Runs ohne Crashes
   - Benchmarks ohne Regression
   - Oracle gibt nur **PASS / FAIL + Metriken**.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Führe `devbox run test` und Coverage aus.
- Identifiziere schwache Bereiche (z. B. missing Complex Gateway tests, Concurrency edge cases).

### Phase 2 – Improvement Generation (Prioritäten)
- Erweiterung auf Property-Based Testing (proptest)
- Cargo-Fuzz für Token-Engine & Parser
- Automatische Flaky-Test-Erkennung
- Parallelisierung von Tests (`cargo test -- --test-threads=8`)
- Test-Report-Integration (JUnit + Codecov)
- BPMN-Compliance-Test-Suite (gegen offizielle Test-Cases)

### Phase 3 – Implementation & Verification
- Generiere neue Test-Dateien oder Test-Helpers in Rust.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere CI-Workflows.

### Phase 4 – Documentation & Handover
- Aktualisiere `README.md` mit neuen Test-Befehlen.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Keine Tests, die länger als 30 Sekunden dauern (außer explizite Benchmarks).
- Alle neuen Tests müssen deterministisch sein.
- Coverage darf nie unter 92 % fallen.
- Devbox bleibt die einzige Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Tests ohne Assertion auf BPMN-Semantik.
- Flaky Tests durch Sleeps oder unkontrollierte Zeit.
- Nur Coverage steigern ohne echte Test-Tiefe.

## Beispiel-Aufruf
„Aktiviere bpmninja-rust-test-evolver und füge Property-Based Testing + Fuzzing für die Token-State-Machine hinzu.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und macht den Rust-Testprozess von bpmninja immer robuster.
