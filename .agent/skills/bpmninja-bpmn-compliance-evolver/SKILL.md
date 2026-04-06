---
name: bpmninja-bpmn-compliance-evolver
description: Selbst-evolvierender Cross-Crate Skill für vollständige BPMN 2.0 Compliance Tests (Spec-Validation, Execution-Compliance, fehlende Elemente). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [bpmn, compliance, spec, testing, cross-crate, evoskills]
requires: [cargo, devbox]
---

# EvoSkills Self-Evolving BPMN Compliance Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du BPMN-Compliance verbessern willst: neue Elemente hinzufügen (Complex Gateway, Data Objects, Multi-Instance), Spec-Tests erweitern oder Execution-Compliance prüfen.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert alle Crates (engine-core, bpmn-parser, persistence-nats etc.), tests/bpmn_compliance und aktuelle fehlende Elemente.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - BPMN 2.0 Spec-Konformität
   - Execution-Semantik
   - Abdeckung kritischer Edge-Cases
   - Integration mit Token-Engine
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - Compliance-Test-Suite
   - Spec-Validation
   - Coverage der Compliance-Tests
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle relevanten Test-Dateien und führe `devbox run test --test bpmn_compliance` aus.
- Erstelle Compliance-Report (unterstützte Elemente vs. BPMN 2.0 Spec).

### Phase 2 – Improvement Generation (Prioritäten)
- Hinzufügen neuer Compliance-Tests für fehlende Elemente
- Automatisierte Tests gegen offizielle BPMN Test-Cases
- Execution-Compliance für Token-Engine
- Property-Based Testing für BPMN-Semantik

### Phase 3 – Implementation & Verification
- Generiere neue Test-Dateien oder erweitere bestehende.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).

### Phase 4 – Documentation & Handover
- Aktualisiere Compliance-Übersicht in docs/ und README.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Tests, die gegen die offizielle BPMN 2.0 Specification verstoßen.
- Alle neuen Compliance-Tests müssen deterministisch sein.
- Coverage der Compliance-Suite > 95 %.
- Devbox bleibt primäre Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur theoretische Compliance ohne echte Execution-Tests.
- Ignorieren von BPMN-Edge-Cases (z. B. Timer + Multi-Instance Kombinationen).
- Tests ohne klare Zuordnung zur BPMN Spec.

## Beispiel-Aufruf
„Aktiviere bpmninja-bpmn-compliance-evolver und erweitere die Suite um vollständige Complex Gateway + Multi-Instance Support.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und bringt bpmninja schrittweise zur 100% BPMN 2.0 Compliance.
