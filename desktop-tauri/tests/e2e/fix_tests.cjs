const fs = require('fs');
const file = 'app.spec.ts';
let content = fs.readFileSync(file, 'utf8');

// Replace standard setup that waits for modeler
content = content.replace(
  /await page\.goto\('\/'\);\s+await expect\(page\.locator\('\.bjs-container'\)\)\.toBeVisible\(\{ timeout: 10_000 \}\);/g,
  "await page.goto('/');\n    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();\n    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });"
);

// Specifically fix the first test
content = content.replace(
  /await page\.goto\('\/'\);\s+const canvas = page\.locator\('\.canvas'\);/g,
  "await page.goto('/');\n    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();\n    const canvas = page.locator('.canvas');"
);

// specifically fix full workflow test case
content = content.replace(
  /test\('full workflow: deploy, start, view tasks, complete', async \(\{ page \}\) => \{\s+await injectTauriMock\(page\);\s+await page\.goto\('\/'\);\s+await expect\(page\.locator\('\.bjs-container'\)\)\.toBeVisible\(\{ timeout: 10_000 \}\);/g,
  "test('full workflow: deploy, start, view tasks, complete', async ({ page }) => {\n    await injectTauriMock(page);\n    await page.goto('/');\n    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();\n    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });"
);


// The other tests that just do await page.goto('/') and then click a tab are totally fine!
// Because they click their own tab immediately anyway!

fs.writeFileSync(file, content);
console.log("Fixed!");
