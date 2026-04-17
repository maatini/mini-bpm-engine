# Graph Maintenance & Metriken

Der Knowledge-Graph veraltet sofort bei jeder Code-Änderung. Es ist deine Aufgabe, ihn aktuell zu halten.

### Wann `graphify update .` ausgeführt werden MUSS:
- Nach jedem abgeschlossenen Task-Intervall (≥3 betroffene Dateien).
- Nach jedem Git Commit.
- Vor einem `/compact` Kontext-Reset.
- Bei der Anlage von neuen Funktionen/Architekturelementen.
*(Dies führt nur ein AST-Update durch und generiert keine API-Kosten.)*

### Nach jedem Update – Metriken in `GRAPH_REPORT.md` prüfen:
Überprüfe zwingend die Auswirkungen deiner Arbeit:
1. **God Nodes (>50 Edges):** Sind neue God Nodes entstanden? -> Erstelle ein "Refactor"-Issue.
2. **Isolierte Nodes (≤1 Edge):** Hast du toten Code geschrieben? -> Fehlende Integration prüfen.
3. **Community-Kohäsion (< 0.05):** Hat sich die Modulgrenze verschlechtert? -> Modulschnitt überdenken.

### Manueller Full Re-Build
Führe `graphify build .` (voller LLM-Rebuild inklusive Semantik/Inferenz) **niemals** eigenmächtig aus, da dies API-Kosten verursacht. Nur nach explizitem Nutzerbefehl auslösen.
