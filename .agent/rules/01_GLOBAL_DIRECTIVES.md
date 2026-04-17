# Global Agent Directives & Workflow

1. **Plan & Wait:** Immer erst einen Plan vorschlagen via `implementation_plan.md` -> Auf `GO` warten.
2. **Sprache:** Kommunikation, Commits und Dokumentation erfolgen auf Deutsch. API-Bezeichner und Code bleiben auf Englisch.
3. **No Temp Files:** Niemals `tmp/`, `temp/` oder den Desktop nutzen. Code muss in gut benannten Ziel-Modulen oder In-Memory getestet/geschrieben werden.
4. **Architektur & Handoff-Order:** Dependencies zwingend immer zuerst bauen!
   Die Implementierungs-Reihenfolge bei Cross-Crate-Features ist strikt:
   `engine-core` (Traits/pure) → `bpmn-parser` → `persistence-nats` → `engine-server` (Axum) → `desktop-tauri` (Rust/React).
5. **Traits over Types:** Cross-Crate-Kommunikation erfolgt ausschließlich über Rust Traits (z. B. im `port/` Modul). Niemals konkrete Typen aus anderen Crates importieren.
