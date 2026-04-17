# TASK_MEMORY — E2E-Test-Fixes

## Aufgabe
11 fehlschlagende Playwright-E2E-Tests in `desktop-tauri/tests/e2e/app.spec.ts` reparieren
sowie React-Warnungen (fehlende `key`-Props, fehlende `DialogDescription`) beheben.

## Identifizierte Fehlergruppen

### Gruppe 1 — ConditionPropertiesProvider-Bug (1 Test-Fix + 1 Komponenten-Fix)

**Problem A (Komponenten-Bug in `ConditionPropertiesProvider.ts`):**  
`CustomConditionGroup` zeigt die Condition-Gruppe auch für Default-Flows (`Flow_Default`),
obwohl laut Camunda-7-Kompatibilität Default-Flows keine Condition haben sollen.

**Fix:** `if (isDefaultFlow(element)) return null;` in `CustomConditionGroup` nach dem SequenceFlow-Check einfügen.

**Problem B (Test-Locator-Bug in `app.spec.ts`):**  
Tests 4+5 (`expression mode`, `script mode`) klicken den Condition-Header per `conditionHeader.click()`.
Da `shouldOpen: true` gesetzt ist, startet die Gruppe OFFEN. Ein Klick SCHLIESST sie →
Entries danach nicht auffindbar.

**Fix:** `conditionHeader.click()` + `waitForTimeout` aus Tests 4+5 entfernen.

### Gruppe 2 — Suspend/Resume-Tests (2 Failures)

**Problem:** Tests navigieren zum Instances-Tab und suchen `table tbody tr`.  
`InstancesPage.tsx` rendert aber `div.instance-list-item`-Karten, keine Tabelle.

**Fix in `app.spec.ts`:** `page.locator('table tbody tr').first()` → `page.locator('.instance-list-item').first()`

### Gruppe 3 — Migration-Dialog-Tests (3 Failures)

**Problem:** Gleicher Selektor-Fehler: `table tbody tr` statt `.instance-list-item`.

**Fix in `app.spec.ts`:** Gleiche Änderung wie Gruppe 2.

### Gruppe 4 — DataViewer/Base64-Tests (2 Failures)

**Problem:** Unklar — Logik in `DataViewer.tsx` und `MonitoringPage.tsx` ist korrekt.  
Mögliche Ursache: `DialogContent` ohne `DialogDescription` (Radix-UI-Warnung),
oder Timing beim Dialog-Wechsel (Bucket-Entries-Dialog → Entry-Detail-Dialog).

**Zu prüfen:** Tests tatsächlich ausführen und Fehlerausgabe auswerten.

## Zusätzliche React-Warnungen

### A — Fehlende `key`-Props in OverviewPage
Skeleton-Cards in `OverviewPage.tsx` nutzen `i` als Key — ggf. bereits vorhanden, nochmals prüfen.

### B — Fehlende `DialogDescription` / `aria-describedby`
Mehrere `<DialogContent>`-Komponenten ohne `<DialogDescription>`:
- `MonitoringPage.tsx`: Bucket-Entries-Dialog und Entry-Detail-Dialog
- `MigrationDialog.tsx`: Haupt-Dialog
- `InstanceDetailDialog.tsx`: Haupt-Dialog

**Fix:** `<DialogDescription className="sr-only">...</DialogDescription>` hinzufügen.

## Betroffene Dateien
1. `desktop-tauri/src/features/modeler/properties/ConditionPropertiesProvider.ts`
2. `desktop-tauri/tests/e2e/app.spec.ts`
3. `desktop-tauri/src/features/monitoring/MonitoringPage.tsx`
4. `desktop-tauri/src/features/instances/MigrationDialog.tsx`
5. `desktop-tauri/src/features/instances/InstanceDetailDialog.tsx`
