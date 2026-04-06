---
name: bpmninja-documentation-evolver
description: Selbst-evolvierender Skill für die gesamte Dokumentation (README, Architecture, Mermaid-Diagramme, API-Docs, Contributor-Guide). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [documentation, docs, mermaid, architecture, contributor-guide, evoskills]
requires: [cargo, devbox, mdbook, mermaid-cli]
---

# EvoSkills Self-Evolving Documentation Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du die Dokumentation von bpmninja verbessern willst: Architecture-Overview, Mermaid-Diagramme, README, API-Docs, Contributor-Guide oder Inline-Rust-Doku.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert docs/, README.md, alle crate-Dokus, Cargo.toml und aktuelle Mermaid-Dateien.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Klarheit & Aktualität
   - Technische Genauigkeit (Token-Engine, NATS, Tauri)
   - Mermaid-Diagramm-Qualität
   - Contributor-Freundlichkeit
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - Docs-Health-Check
   - Mermaid-Rendering-Test
   - Coverage der Dokumentation
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Markdown-Dateien finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Dokumentations-Dateien und führe Docs-Checks aus.
- Erstelle Documentation-Health-Report (veraltete Abschnitte, fehlende Diagramme).

### Phase 2 – Improvement Generation (Prioritäten)
- Aktualisierte Architecture-Diagramme (Mermaid)
- Vollständiger Contributor-Guide
- API-Dokumentation mit rustdoc + mdBook
- Bessere README mit Screenshots und Quickstart
- Inline-Doku für alle wichtigen Rust-Module

### Phase 3 – Implementation & Verification
- Generiere Markdown- und Mermaid-Änderungen.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).

### Phase 4 – Documentation & Handover
- Aktualisiere alle relevanten Dateien.
- Erweitere `.agent/skills/` mit neuen Sub-Skills (self-referential).

## Strenge Regeln & Constraints
- Niemals falsche oder veraltete Informationen.
- Alle Mermaid-Diagramme müssen fehlerfrei rendern.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur Text-Änderungen ohne visuelle Diagramme.
- Ignorieren von Tauri- oder NATS-spezifischen Architekturdetails.
- Veraltete Screenshots oder Code-Beispiele.

## Beispiel-Aufruf
„Aktiviere bpmninja-documentation-evolver und erstelle ein vollständiges Mermaid-Diagramm der Token-Engine inklusive NATS-Persistenz.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und macht bpmninja zu einem der am besten dokumentierten Rust-Projekte.
