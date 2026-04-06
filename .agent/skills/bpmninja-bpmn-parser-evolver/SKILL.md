---
name: bpmninja-bpmn-parser-evolver
description: Selbst-evolvierender Skill für das bpmn-parser Crate (XML → BPMN Model, Validation, Error-Recovery, Performance). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, bpmn-parser, xml, parsing, validation, evoskills]
requires: [cargo, devbox, cargo-fuzz]
---

# EvoSkills Self-Evolving BPMN Parser Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am bpmn-parser Crate arbeitest: neue BPMN-Elemente hinzufügen, XML-Performance verbessern, Error-Handling erweitern, Spec-Compliance prüfen oder Fuzzing einführen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert src/, tests/, Cargo.toml und aktuelle Benchmarks.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Parsing-Safety & Error-Recovery
   - BPMN 2.0 Spec-Compliance
   - Performance-Impact (keine Regressionen)
   - Testbarkeit & Fuzzing
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p bpmn-parser`
   - `cargo clippy -p bpmn-parser --all-targets -- -D warnings`
   - Coverage ≥ 95 %
   - Fuzzing ohne Crashes
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Parser-Dateien und führe `devbox run test -p bpmn-parser` aus.
- Erstelle Parser-Health-Report (Coverage, fehlende Elemente, Performance).

### Phase 2 – Improvement Generation (Prioritäten)
- XML-Parser-Optimierung & Error-Recovery
- Neues BPMN-Element-Parsing (Complex Gateway, Data Objects, Multi-Instance etc.)
- Fuzzing & Property-Based Testing
- Performance bei großen BPMN-Dateien

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Rust-Code.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere Tests und Benchmarks.

### Phase 4 – Documentation & Handover
- Aktualisiere crate-Doku und README.
- Erweitere `.agent/skills/` mit neuen Sub-Skills (self-referential).

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump im Cargo.toml.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Features müssen mit Unit- + Fuzz-Tests abgedeckt sein.
- Coverage darf nie unter 95 % fallen.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Verbesserungen ohne Oracle-Validierung.
- Ignorieren von Edge-Cases bei großen XML-Dateien oder fehlerhaften BPMN-Dokumenten.
- Performance-Optimierungen ohne Benchmark-Oracle.

## Beispiel-Aufruf
„Aktiviere bpmninja-bpmn-parser-evolver und implementiere vollständiges Parsing von Complex Gateways mit Fuzzing und 98% Coverage.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für das bpmn-parser Crate.
