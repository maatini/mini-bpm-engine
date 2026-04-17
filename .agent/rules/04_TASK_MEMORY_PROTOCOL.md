# Task Memory Protocol & Context Resets

Um das unweigerliche Vergessen (Halluzinieren) bei großen Tasks und Context-Resets (`/compact`) zu überlisten, nutzt du einen isolierten Task-Memory.

### Wann wird `TASK_MEMORY.md` im Projekt-Root angelegt?
- Wenn eine Aufgabe **≥ 2 Crates** überschreitet.
- Wenn eine Aufgabe **≥ 3 Dateien** berührt.

### Zwingender Inhalt
1. Titel & 1-Satz Zieldefinition.
2. Gefundene, betroffene Graph-Communities & God Nodes.
3. Checkliste mit granularen Implementierungsschritten.
4. "Letzter Stand": Eine Kurzzusammenfassung der unmittelbaren laufenden Arbeit.

### Der `/compact` Re-Entry Workflow
1. **Vor einem Reset:** Sorge dafür, dass alle Checkboxen und der "Letzte Stand" in `TASK_MEMORY.md` akribisch aktuell sind. 
2. **Nach einem Reset:** Ignoriere deine vagen Erinnerungen! Beginne deine Arbeit SOFORT damit, `TASK_MEMORY.md` zu lesen, gefolgt von `graphify-out/GRAPH_REPORT.md`. Mache erst danach mit dem nächsten Checklist-Punk weiter.

### Abschluss
Ist die Aufgabe durch, löschst du `TASK_MEMORY.md` eigenständig als Clean-Up-Schritt und führst `graphify update .` aus.
