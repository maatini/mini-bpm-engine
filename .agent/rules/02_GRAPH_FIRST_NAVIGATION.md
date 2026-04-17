# Graph-First Development & Navigation (Graphify)

Der Workspace hat einen entscheidenden Meta-Index: `graphify-out/GRAPH_REPORT.md`.

**BEVOR du jeglichen Code liest, navigierst oder Architektur planst, gilt ZWINGEND folgende Reihenfolge:**

1. **Meta-Index lesen:** Beginne JEDE Aufgabe damit, den `GRAPH_REPORT.md` zu analysieren. Verstehe die God Nodes und die grobe Struktur.
2. **Subgraphen eingrenzen:** Identifiziere die anvisierten Subgraphen (Communities). Lade ausnahmslos **nur** die Communities deines Aufgaben-Crates.
   - `engine-core` = Communities 0, 1, 4, 11, 17, 18, 19, 25
   - `bpmn-parser` = Communities 8, 10
   - `persistence-nats` = Communities 6, 9
   - `engine-server` = Communities 3, 12, 20
   - `desktop-tauri` = Communities 2, 13, 14, 15
3. **Graph MCP Tools nutzen:** Benutze ZUERST die Graph-Tools zur Code-Navigation:
   - `query_graph` (für semantische Suche)
   - `get_node` (für spezifische Symbol-Details)
   - `get_neighbors(depth=1)` (für betroffene Aufrufer/Abhängigkeiten)
   - `shortest_path` (für Dependency-Pfade)
4. **Code lesen als Letztes:** ERST DANACH liest du Dateien über Tools wie `view_file`. `grep_search` ist ausschließlich als Fallback zu verwenden, falls der Graph die Antwort schuldig bleibt!

**WARNUNG:** God Nodes (>50 Edges, z.B. `deploy_definition()`) dürfen niemals ohne vorherige, tiefgehende Graphen-Analyse editiert werden!
