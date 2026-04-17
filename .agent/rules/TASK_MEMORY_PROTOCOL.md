---
trigger: always_on
---

## Task Memory Protocol

Bei jeder Aufgabe, die **mehr als 2 Dateien oder mehr als 1 Crate** betrifft, MUSS zu Beginn eine `TASK_MEMORY.md` im Projekt-Root angelegt werden.

---

## Wann anlegen

- Aufgabe berührt ≥2 Crates (Cross-Crate-Feature)
- Aufgabe berührt ≥3 Dateien innerhalb eines Crates
- Aufgabe umfasst Graph-Analyse + Implementierung + Verification
- Aufgabe wird voraussichtlich mehrere Gesprächsrunden dauern

---

## Pflichtinhalt der TASK_MEMORY.md

```markdown
# Task Memory — <Aufgabentitel>

## Aufgabe (1 Satz)
<Was soll erreicht werden>

## Betroffene Graph-Nodes
| Symbol | Community | Edges | Warum betroffen |
|---|---|---|---|
| deploy_definition() | 0 | 135 | Einstiegspunkt für Deploy-Logik |

## Betroffene Communities
- Community X: <Beschreibung>

## Implementierungsschritte
- [ ] Schritt 1: <Crate / Datei / Was>
- [ ] Schritt 2: ...
- [x] Schritt 0 (abgeschlossen): ...

## Offene Fragen / Risiken
- ...

## Letzter Stand
<Kurze Beschreibung wo die Arbeit steht — wird nach jedem Schritt aktualisiert>
```

---

## Lebenszyklus

1. **Zu Beginn:** Datei anlegen, alle Felder ausfüllen
2. **Nach jedem abgeschlossenen Schritt:** Checkbox abhaken, "Letzter Stand" aktualisieren
3. **Bei `/compact` oder Session-Reset:** `TASK_MEMORY.md` ist der **einzige Wiederherstellungspunkt** — immer zuerst lesen
4. **Nach Task-Abschluss:** Datei löschen, `graphify update .` ausführen

---

## Zwischen-Reset-Protokoll

Wenn der Kontext zu groß wird oder nach >3 abgeschlossenen Implementierungsschritten:

1. `TASK_MEMORY.md` auf aktuellen Stand bringen (alle Checkboxen, letzter Stand)
2. `/compact` ausführen
3. Nach dem Reset: `TASK_MEMORY.md` lesen, dann Graph-Query für Kontext-Wiederherstellung
4. **Niemals** nach Reset von vorne beginnen ohne `TASK_MEMORY.md` zu lesen

---

## Was NICHT in TASK_MEMORY.md gehört

- Vollständiger Code (→ in die tatsächlichen Dateien)
- Git-History oder bereits gemergte Änderungen
- Permanente Architektur-Entscheidungen (→ in CLAUDE.md oder SKILL.md)
