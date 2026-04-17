# Implementation Plan — E2E-Test-Fixes + React-Warnungen

## Überblick

11 fehlschlagende Playwright-E2E-Tests beheben + React-Warnungen (fehlende `key`-Props,
fehlende `DialogDescription`/`aria-describedby`) beseitigen.

---

## Änderung 1 — ConditionPropertiesProvider.ts (Komponenten-Bug)

**Datei:** `desktop-tauri/src/features/modeler/properties/ConditionPropertiesProvider.ts`

**Problem:** `CustomConditionGroup` zeigt die Condition-Gruppe auch für Default-Flows.  
Camunda-7-Spezifikation: Default-Flows haben keine Condition (Gruppe soll ausgeblendet sein).

**Fix (eine Zeile):**
```typescript
function CustomConditionGroup(element: any, translate: any) {
  if (!is(element, 'bpmn:SequenceFlow')) return null;

  const sourceRef = element.businessObject.sourceRef;
  if (!sourceRef || (!is(sourceRef, 'bpmn:ExclusiveGateway') && !is(sourceRef, 'bpmn:InclusiveGateway'))) {
    return null;
  }

+ // Camunda 7: Default-Flows haben keine Condition-Gruppe
+ if (isDefaultFlow(element)) return null;

  return { id: 'ConditionGroup', label: translate('Condition'), shouldOpen: true, entries: [...] };
}
```

**Betroffener Failing-Test:** `condition group hidden for default flow on ExclusiveGateway`

---

## Änderung 2 — app.spec.ts: Condition-Group-Tests (Locator-Fix)

**Datei:** `desktop-tauri/tests/e2e/app.spec.ts`

**Problem:** Tests 4+5 klicken den Condition-Header per `conditionHeader.click()`.
Da `shouldOpen: true` die Gruppe beim Laden ÖFFNET, schließt dieser Klick sie wieder.

**Fix:** In `expression mode sets conditionExpression body without language`
und `script mode sets conditionExpression with language attribute` die Zeilen entfernen:
```typescript
- const conditionHeader = propsPanel.getByText('Condition');
- await conditionHeader.click();
- await page.waitForTimeout(300);
```

**Betroffene Failing-Tests:**
- `expression mode sets conditionExpression body without language`
- `script mode sets conditionExpression with language attribute`

---

## Änderung 3 — app.spec.ts: Suspend/Resume-Tests (Selektor-Fix)

**Datei:** `desktop-tauri/tests/e2e/app.spec.ts`

**Problem:** Beide Tests verwenden `page.locator('table tbody tr').first()` um Instanzen
im Instances-Tab zu finden. `InstancesPage.tsx` rendert jedoch `div.instance-list-item`-Karten.

**Fix:**
```typescript
- await expect(page.locator('table tbody tr').first()).toBeVisible({ timeout: 5_000 });
- await page.locator('table tbody tr').first().click();
+ await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
+ await page.locator('.instance-list-item').first().click();
```

**Betroffene Failing-Tests:**
- `zeigt Suspend-Button und setzt Instanz auf Suspended`
- `zeigt Resume-Button für suspendierte Instanz`

---

## Änderung 4 — app.spec.ts: Migration-Dialog-Tests (Selektor-Fix)

**Datei:** `desktop-tauri/tests/e2e/app.spec.ts`

**Problem:** Alle drei Migration-Tests verwenden ebenfalls `table tbody tr`.

**Fix:** Gleiche Änderung wie Änderung 3 (2× pro Test = 6 Zeilen gesamt).

**Betroffene Failing-Tests:**
- `Migrate-Button ist deaktiviert wenn keine andere Version vorhanden`
- `Migrationsdialog zeigt Versions-Dropdown wenn Kandidaten vorhanden`
- `Migrate-Button im Dialog bleibt deaktiviert ohne Zielauswahl`

---

## Änderung 5 — DialogDescription hinzufügen (React-Warnungen)

**Problem:** Radix-UI `<DialogContent>` ohne `<DialogDescription>` bzw. `aria-describedby`
erzeugt Konsolen-Warnungen. Kann auch Dialog-Zugänglichkeitsprobleme verursachen.

### 5a — MonitoringPage.tsx (2 Dialoge)
```tsx
// Bucket-Entries-Dialog
<DialogContent ...>
  <DialogHeader>
    <DialogTitle>Bucket Entries: {selectedBucket}</DialogTitle>
+   <DialogDescription className="sr-only">Einträge im Bucket {selectedBucket}</DialogDescription>
  </DialogHeader>

// Entry-Detail-Dialog  
<DialogContent ...>
  <DialogHeader>
    <DialogTitle>Detail: {selectedEntryKey}</DialogTitle>
+   <DialogDescription className="sr-only">Details für Eintrag {selectedEntryKey}</DialogDescription>
  </DialogHeader>
```

### 5b — MigrationDialog.tsx
```tsx
<DialogContent ...>
  <DialogHeader ...>
    <DialogTitle>Instanz migrieren</DialogTitle>
+   <DialogDescription className="sr-only">Instanz zu einer anderen Prozessversion migrieren</DialogDescription>
  </DialogHeader>
```

### 5c — InstanceDetailDialog.tsx
```tsx
<DialogContent ...>
  <DialogHeader ...>
    <DialogTitle>Instance Details: ...</DialogTitle>
+   <DialogDescription className="sr-only">Details und Variablen der Prozessinstanz</DialogDescription>
  </DialogHeader>
```

---

## Änderung 6 — DataViewer-Tests (zu prüfen)

**Status:** Unklar ob diese 2 Tests tatsächlich scheitern oder aus anderen Gründen.  
Nach Änderungen 1-5 die Tests ausführen. Falls DataViewer-Tests noch scheitern:
- Mögliche Ursache: Dialog-Übergangs-Timing beim Wechsel Bucket-Dialog → Entry-Dialog
- Fix: `page.waitForTimeout(200)` nach dem Entry-Klick einfügen

---

## Ausführungsreihenfolge

1. Änderung 1 (ConditionPropertiesProvider.ts)
2. Änderung 2 (app.spec.ts — Condition-Header-Klick entfernen)
3. Änderungen 3+4 gleichzeitig (app.spec.ts — table-Selektoren ersetzen)
4. Änderung 5 (DialogDescription in 3 Komponenten)
5. Tests ausführen: `cd desktop-tauri && npx playwright test`
6. Falls DataViewer noch scheitert: Änderung 6

---

## Verifikation

```bash
cd desktop-tauri && npx playwright test
# Erwartetes Ergebnis: alle Tests grün
```
