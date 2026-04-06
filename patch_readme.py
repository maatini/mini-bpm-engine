import re

with open("README.md", "r") as f:
    c = f.read()

c = c.replace("`engine-core` (Workflow-Engine, State-Machine mit 140+ Tests)", "`engine-core` (Workflow-Engine, State-Machine mit 160+ Tests)")
c = c.replace("2. **BPMN 2.0 Kompatibilität:** Ausführung der 18 gängigsten BPMN-Elemente", "2. **BPMN 2.0 Kompatibilität:** Ausführung der 21 gängigsten BPMN-Elemente")
c = c.replace("- Modeler lädt automatisch `example.bpmn` (max. 10MB Datei-Limit)", "- Modeler lädt automatisch `example.bpmn` (max. 5MB Datei-Limit)")
c = c.replace("Aktuell deckt die Suite **140 Tests** ab.", "Aktuell deckt die Suite **160 Tests** ab.")
c = c.replace("- `bpmn-parser`: 6+ Tests (Parsing, Flow-Resolution)", "- `bpmn-parser`: 26+ Tests (Parsing, Flow-Resolution, Flattening)")

with open("README.md", "w") as f:
    f.write(c)

print("Phase 4 README done")
