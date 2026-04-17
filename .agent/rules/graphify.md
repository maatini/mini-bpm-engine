---
trigger: always_on
---

## Graphify — Graph-basierte Entwicklung

Dieses Projekt hat einen Graphify-Knowledge-Graphen unter `graphify-out/`.

---

## Pflicht: Graph-First Navigation

**Vor jeder Architektur- oder Implementierungsaufgabe diese Reihenfolge einhalten:**

1. `graphify-out/GRAPH_REPORT.md` lesen → God Nodes + Community-Karte erfassen
2. Betroffene Community(s) identifizieren (≤3 bei Einzel-Crate-Aufgaben)
3. MCP-Tools in dieser Reihenfolge verwenden:
   - `get_node(symbol)` → spezifisches Symbol nachschlagen
   - `get_neighbors(symbol, depth=1)` → direkt betroffene Symbole ermitteln
   - `shortest_path(a, b)` → Abhängigkeitskette zwischen zwei Modulen
   - `query_graph(pattern)` → Mustersuche für unbekannte Symbole
4. **Erst danach** direkte Source-Dateien lesen (gefiltert durch Graph-Erkenntnisse)
5. `grep` / `glob` nur als Fallback, wenn der Graph keine Antwort liefert

**God Nodes (>50 Edges) nie ohne Graph-Analyse anfassen:**
- `deploy_definition()` — 135 Edges
- `start_instance()` — 108 Edges
- `parse_bpmn_xml()` — 46 Edges

---

## MCP-Tool-Einsatz

Wenn der graphify MCP-Server aktiv ist (`mcp__graphify__*`):
- `query_graph` statt `grep` für semantische Suche
- `get_node` statt `cat` für einzelne Symbol-Details
- `shortest_path` statt manueller Dependency-Analyse
- `get_neighbors` statt `grep -r` für alle Aufrufer/Aufgerufenen

Wenn der MCP-Server nicht aktiv ist:
- `graphify-out/GRAPH_REPORT.md` als primäre Navigationsquelle
- Subgraph-JSONs in `graphify-out/cache/` für Community-Details

---

## Subgraph-Partitionierung

Beim Arbeiten in einem einzelnen Crate nur die relevanten Communities laden:

| Crate | Primäre Communities |
|---|---|
| `engine-core` | 0, 1, 4, 11, 17, 18, 19, 25 |
| `bpmn-parser` | 8, 10 |
| `persistence-nats` | 6, 9 |
| `engine-server` | 3, 12, 20 |
| `desktop-tauri` | 2, 13, 14, 15 |

Keine UI-Communities laden, wenn an Rust-Engine gearbeitet wird — und umgekehrt.

---

## Graph aktuell halten

Nach Code-Änderungen in dieser Session:
```bash
graphify update .
```
(AST-only, kein API-Aufruf, keine Kosten)

Danach in `GRAPH_REPORT.md` prüfen:
- Neue God Nodes (>50 Edges) → Issue erstellen
- Neue isolierte Nodes (≤1 Edge) → Integration prüfen oder löschen
- Community-Kohäsion < 0.05 → Modul-Schnitt überdenken
