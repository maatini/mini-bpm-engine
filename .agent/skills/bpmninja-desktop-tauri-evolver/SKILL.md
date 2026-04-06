---
name: bpmninja-desktop-tauri-evolver
description: Selbst-evolvierender Skill für das desktop-tauri Crate (Tauri Desktop App, bpmn-js Integration, Auto-Updates, UI-Performance). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [rust, tauri, desktop, bpmn-js, ui, auto-update, evoskills]
requires: [cargo, devbox, tauri-cli, node]
---

# EvoSkills Self-Evolving Tauri Desktop Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am desktop-tauri Crate arbeitest: bpmn-js Integration, UI-Performance, Auto-Updates, Tauri-Builds, Native Rust-JS-Bridge oder Accessibility-Verbesserungen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert src/, frontend/, tauri.conf.json, Cargo.toml und aktuelle UI-Tests.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - bpmn-js Canvas Performance & Reactivität
   - Auto-Update Sicherheit & UX
   - Native Rust → JS Bridge
   - Tauri-Build-Stabilität & Bundle-Größe
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `cargo test -p desktop-tauri`
   - UI-Tests + bpmn-js Integration
   - Tauri Build (debug + release)
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Tauri-Dateien und führe `devbox run test -p desktop-tauri` aus.
- Starte die App lokal und prüfe bpmn-js Performance.
- Erstelle Tauri-Health-Report (Build-Zeit, Bundle-Größe, offene Issues).

### Phase 2 – Improvement Generation (Prioritäten)
- bpmn-js Canvas Performance Optimierungen (WebGL, Chunking)
- Automatischer Update-Mechanismus mit Tauri Updater
- Bessere Rust-JS-Bridge (invoke commands für Engine)
- Accessibility (ARIA, Keyboard Navigation)
- Tauri v2 Migration & Bundle-Optimierungen

### Phase 3 – Implementation & Verification
- Generiere diff-fähigen Rust + TypeScript-Code.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).
- Aktualisiere tauri.conf.json und frontend.

### Phase 4 – Documentation & Handover
- Aktualisiere Desktop-README und Screenshots.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Breaking Changes ohne Major-Version-Bump.
- Immer `cargo fmt` + `cargo clippy -- -D warnings`.
- Alle neuen Features müssen mit UI- + Integration-Tests abgedeckt sein.
- Tauri Build muss erfolgreich sein (debug + release).
- Devbox + Tauri CLI bleibt primäre Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische UI-Verbesserungen ohne reale bpmn-js Performance-Messung.
- Ignorieren von macOS/Windows/Linux spezifischen Tauri-Eigenheiten.
- Auto-Update ohne Security-Checks.

## Beispiel-Aufruf
„Aktiviere bpmninja-desktop-tauri-evolver und optimiere die bpmn-js Canvas Performance mit Auto-Update Support.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und wird präziser für das desktop-tauri Crate.
