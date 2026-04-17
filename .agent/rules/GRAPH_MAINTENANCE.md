---
trigger: always_on
---

## Graph-Wartung & Aktualität

Der Graphify-Knowledge-Graph unter `graphify-out/` ist ein **lebendiges Dokument** — er veraltet, sobald Code geändert wird. Diese Rule definiert wann und wie er aktuell gehalten wird.

---

## Wann `graphify update .` ausführen

| Auslöser | Pflicht |
|---|---|
| Nach jedem `git commit` | Ja |
| Nach jeder abgeschlossenen Task-Gruppe (≥3 Dateien geändert) | Ja |
| Vor einer Architektur-Analyse-Session | Empfohlen |
| Nach `/compact` oder Session-Reset | Empfohlen |
| Nach einzelner kleiner Datei-Änderung (<3 Zeilen) | Optional |

```bash
# AST-only Update — kein API-Aufruf, keine Kosten
graphify update .
```

---

## Staleness erkennen

Der Graph ist veraltet, wenn:
- `git diff --name-only HEAD~1` Rust- oder TypeScript-Dateien enthält, die seit dem letzten `graphify update` geändert wurden
- `GRAPH_REPORT.md` ein älteres Datum trägt als der jüngste Commit

Staleness-Check:
```bash
# Letzter Graph-Build:
head -2 graphify-out/GRAPH_REPORT.md

# Letzte Code-Änderung:
git log --oneline -1
```

---

## Nach jedem Graph-Update prüfen

In `graphify-out/GRAPH_REPORT.md` nachsehen:

1. **Neue God Nodes (>50 Edges)?**
   → Issue erstellen: "Refactor: <Symbol> hat zu viele Abhängigkeiten"

2. **Neue isolierte Nodes (≤1 Edge)?**
   → Prüfen ob toter Code oder fehlende Integration

3. **Community-Kohäsion < 0.05?**
   → Modul-Schnitt überdenken (zu große, zu heterogene Community)

4. **Inferred Edges > 70%?**
   → Graph-Qualität niedrig — mehr Extraktions-Quellen hinzufügen

---

## Graph-Rebuild (Vollständig, mit API)

Nur nötig wenn sich die Struktur des Projekts grundlegend geändert hat (neue Crates, größere Refactorings):

```bash
graphify build .
```

Dies erzeugt neue LLM-Inferenzen und kostet API-Credits. Nur nach expliziter Absprache ausführen.
