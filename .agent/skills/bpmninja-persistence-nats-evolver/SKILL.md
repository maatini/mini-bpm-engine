---
name: bpmninja-persistence-nats-evolver
description: Selbst-evolvierender Skill für das persistence-nats Crate (JetStream, Token-State-Serialisierung, Crash-Recovery, Multi-Node Clustering). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, persistence, nats, jetstream, clustering, recovery, evoskills]
requires: [cargo, devbox, nats-server]
---

# EvoSkills Self-Evolving NATS Persistence Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am persistence-nats Crate arbeitest: JetStream-Integration, Token-State-Persistenz, Crash-Recovery, Multi-Node Clustering oder Performance-Optimierungen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert persistence-nats/src/, tests/, Cargo.toml und aktuelle NATS-Setup.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Lock-free Safety & Concurrency
   - Crash-Recovery-Korrektheit
   - Multi-Node Clustering
   - Serialisierungs-Performance & Kompatibilität
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p persistence-nats`
   - Recovery-Tests mit NATS
   - Clippy & Coverage
   - Benchmarks
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Persistence-Dateien und starte lokalen NATS-Server.
- Führe `devbox run test -p persistence-nats` aus.
- Erstelle Persistence-Health-Report (Recovery-Zeit, Throughput, fehlende Features).

### Phase 2 – Improvement Generation (Prioritäten)
- Multi-Node JetStream Clustering
- Verbesserte Token-State Serialisierung (bincode / custom)
- Schnellere Crash-Recovery
- Lock-free Queue-Optimierungen
- High-Availability & Failover-Tests

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Rust-Code.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere Tests und Benchmarks.

### Phase 4 – Documentation & Handover
- Aktualisiere crate-Doku und Integration-Beispiele.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Features müssen mit Unit- + Integration- + Recovery-Tests abgedeckt sein.
- Coverage > 92 %.
- Devbox + lokaler NATS bleibt primäre Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Cluster-Verbesserungen ohne echte Recovery-Oracle.
- Ignorieren von Netzwerk-Edge-Cases oder NATS JetStream Limits.
- Serialisierungs-Änderungen ohne Backward-Compatibility.

## Beispiel-Aufruf
„Aktiviere bpmninja-persistence-nats-evolver und implementiere Multi-Node JetStream Clustering mit vollständiger Crash-Recovery.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für das persistence-nats Crate.
