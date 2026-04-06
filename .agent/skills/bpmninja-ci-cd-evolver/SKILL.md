---
name: bpmninja-ci-cd-evolver
description: Selbst-evolvierender Skill für den gesamten CI/CD-Prozess (GitHub Actions, Semantic Release, Coverage, Docker, Security). Nutzt das EvoSkills-Framework (arXiv 2604.01687) mit Co-Evolutionary Verification (Surrogate Verifier + External Oracle).
version: 1.0.0
author: Grok (via EvoSkills)
tags: [ci-cd, github-actions, release, coverage, docker, evoskills]
requires: [cargo, devbox, gh-cli]
---

# EvoSkills Self-Evolving CI/CD Optimizer

## Wann diesen Skill aktivieren?
Immer wenn du am CI/CD-Prozess des bpmninja-Workspaces arbeitest: neue GitHub Workflows, Semantic Releases, Coverage-Reports, Docker-Publishing oder Security-Scanning.

## EvoSkills Co-Evolutionary Verification Loop (Pflicht!)
1. **Skill Generator**  
   Analysiert .github/workflows/, Cargo.toml, devbox.json und aktuelle CI-Ergebnisse.

2. **Surrogate Verifier (co-evolviert)**  
   Selbstkritik (0–10 Punkte) zu:
   - Workflow-Stabilität & Caching
   - Semantic Release & Changelog-Qualität
   - Coverage-Reporting & Security
   - Build-Zeit & Kosten
   Gib **konkretes, actionables Feedback**.

3. **External Oracle (binär)**  
   Führe echte Validierung aus (siehe scripts/):
   - `devbox run test --workspace`
   - Release-Simulation
   - Coverage-Upload-Check
   - Oracle gibt nur **PASS / FAIL + Metriken** zurück.

4. **Evolution**  
   Bei FAIL → zurück zu Schritt 1 mit Verifier-Feedback.  
   Bei PASS → Code finalisieren und PR-ready machen.  
   Mindestens 3 Evolutions-Runden pro Verbesserung.

## Schritt-für-Schritt Workflow

### Phase 1 – Assessment
- Lade alle Workflow-Dateien und führe `devbox run test --workspace` aus.
- Analysiere aktuelle CI-Laufzeiten und Coverage.

### Phase 2 – Improvement Generation (Prioritäten)
- Semantic Release mit automatischem Changelog
- Codecov + Coverage-Badges
- Multi-Arch Docker Builds & Publish
- Automatische Dependency-Updates (Dependabot + Renovate)
- Security Scanning (cargo-audit, trivy)
- Parallelisierte Jobs & Caching-Optimierungen

### Phase 3 – Implementation & Verification
- Generiere neue oder geänderte .github/workflows/*.yml Dateien.
- Führe Surrogate + Oracle Loop (≥ 3 Runden).

### Phase 4 – Documentation & Handover
- Aktualisiere README mit neuen CI-Befehlen.
- Erweitere `.agent/skills/` mit neuen Sub-Skills.

## Strenge Regeln & Constraints
- Niemals Breaking Changes an bestehenden Workflows ohne Major-Bump.
- Alle neuen Jobs müssen deterministisch und cache-optimiert sein.
- Coverage darf nie unter 92 % fallen.
- Devbox bleibt die einzige lokale Entwicklungs-Umgebung.
- Alle Änderungen als PR mit EvoSkills-Trace dokumentieren.

## Häufige Fehler vermeiden
- Nur „nice-to-have“-Workflows ohne echte Oracle-Validierung.
- Ignorieren von Windows/macOS-spezifischen CI-Problemen.
- Manuelle Release-Schritte beibehalten.

## Beispiel-Aufruf
„Aktiviere bpmninja-ci-cd-evolver und implementiere Semantic Release + automatische Codecov-Reports.“

**Dieser Skill ist selbst-evolvierend.** Bei jeder Nutzung verbessert er sich selbst durch den EvoSkills-Loop und macht den CI/CD-Prozess von bpmninja immer schneller und robuster.
