import { test, expect, type Page, type Dialog } from '@playwright/test';

// ---------------------------------------------------------------------------
// Mock setup: simulates the Tauri IPC layer so tests can run against
// the plain Vite dev-server (no Tauri backend needed).
// ---------------------------------------------------------------------------

/**
 * State tracked by our mock backend.
 * - deployedDefs: set of definition IDs that have been deployed
 * - instances: set of instance UUIDs that have been started
 * - pendingTasks: array of tasks returned by get_pending_tasks
 * - completedTasks: task IDs that were completed
 */
interface MockState {
  deployedDefs: string[];
  instances: string[];
  pendingTasks: Array<{
    task_id: string;
    instance_id: string;
    node_id: string;
    assignee: string;
    created_at: string;
  }>;
  completedTasks: string[];
  processInstances: Array<{
    id: string;
    definition_id: string;
    state: string | { WaitingOnUserTask: { task_id: string } };
    current_node: string;
    audit_log: string[];
    variables: Record<string, unknown>;
  }>;
}

const DEFAULT_MOCK_STATE: MockState = {
  deployedDefs: [],
  instances: [],
  pendingTasks: [],
  completedTasks: [],
  processInstances: [],
};

/**
 * Injects the __TAURI_IPC__ mock into the page *before* any app code runs.
 * The mock dispatches commands and calls the registered callback/error
 * functions exactly like the real Tauri runtime does.
 */
async function injectTauriMock(
  page: Page, 
  overrides: Partial<MockState> = {},
) {
  const state: MockState = { ...DEFAULT_MOCK_STATE, ...overrides };

  await page.addInitScript((serializedState: MockState) => {
    // Mutable state for the mock backend
    const mockState = serializedState;

    // The Tauri v1 API calls this function for every invoke()
    (window as any).__TAURI_IPC__ = (message: any) => {
      const { cmd, callback, error, ...args } = message;

      // Helper to resolve the invoke promise
      const resolve = (result: any) => {
        const fn = (window as any)[`_${callback}`];
        if (fn) fn(result);
      };
      // Helper to reject the invoke promise
      const reject = (err: string) => {
        const fn = (window as any)[`_${error}`];
        if (fn) fn(err);
      };

      // Dispatch on command name
      setTimeout(() => {
        try {
          switch (cmd) {
            case 'deploy_definition': {
              const defId = 'mock-def-' + Date.now();
              mockState.deployedDefs.push(defId);
              // If the process has a UserTask, seed a pending task
              if (args.xml && args.xml.includes('userTask')) {
                mockState.pendingTasks.push({
                  task_id: 'mock-task-' + Date.now(),
                  instance_id: 'mock-inst-' + Date.now(),
                  node_id: 'UserTask_1',
                  assignee: 'admin',
                  created_at: new Date().toISOString(),
                });
              }
              resolve(defId);
              break;
            }

            case 'deploy_simple_process': {
              mockState.deployedDefs.push('simple');
              resolve("Deployed 'simple' process");
              break;
            }

            case 'start_instance': {
              const instId = 'mock-instance-' + Date.now();
              mockState.instances.push(instId);
              resolve(instId);
              break;
            }

            case 'get_pending_tasks': {
              resolve(mockState.pendingTasks);
              break;
            }

            case 'complete_task': {
              const taskId = args.taskId as string;
              mockState.completedTasks.push(taskId);
              // Remove from pending
              mockState.pendingTasks = mockState.pendingTasks.filter(
                (t: any) => t.task_id !== taskId,
              );
              resolve(null);
              break;
            }

            case 'list_instances': {
              resolve(mockState.processInstances);
              break;
            }

            case 'get_instance_details': {
              const instId = args.instanceId as string;
              const found = mockState.processInstances.find((i: any) => i.id === instId);
              if (found) {
                resolve(found);
              } else {
                reject('No such instance: ' + instId);
              }
              break;
            }

            default:
              reject(`command ${cmd} not found`);
          }
        } catch (e: any) {
          reject(e.message ?? String(e));
        }
      }, 10); // simulate async
    };
  }, state);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Collect all alert messages that fire during a callback */
async function collectAlerts(
  page: Page,
  action: () => Promise<void>,
): Promise<string[]> {
  const alerts: string[] = [];
  const handler = (dialog: Dialog) => {
    alerts.push(dialog.message());
    dialog.accept();
  };
  page.on('dialog', handler);
  await action();
  // Give alerts time to fire
  await page.waitForTimeout(500);
  page.off('dialog', handler);
  return alerts;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe('mini-bpm Desktop App – E2E', () => {

  // ---- 1. Layout & Navigation -----------------------------------------

  test('should load the BPMN modeler with canvas and properties panel', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    const canvas = page.locator('.canvas');
    await expect(canvas).toBeVisible({ timeout: 10_000 });

    const bjsContainer = page.locator('.bjs-container');
    await expect(bjsContainer).toBeVisible({ timeout: 10_000 });

    const propsPanel = page.locator('.properties-panel-parent');
    await expect(propsPanel).toBeVisible({ timeout: 10_000 });
  });

  test('should navigate to Pending Tasks tab and show empty state', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    // Click "Pending Tasks" in sidebar
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();

    // Verify empty-state message
    await expect(page.getByText('No pending tasks.')).toBeVisible({ timeout: 5_000 });

    // Verify Refresh button exists
    await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
  });

  // ---- 2. Deploy Process ----------------------------------------------

  test('should deploy a BPMN process and show success alert', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    // Wait for modeler to load
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Click "Deploy Process"
    const alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });

    expect(alerts.length).toBe(1);
    expect(alerts[0]).toContain('Deployed definition! ID: mock-def-');
  });

  // ---- 3. Start Instance without Deploy --------------------------------

  test('should show warning when starting without deploying first', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    const alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Start Instance' }).click();
    });

    expect(alerts.length).toBe(1);
    expect(alerts[0]).toBe('Please deploy a process first.');
  });

  // ---- 4. Start Instance after Deploy ----------------------------------

  test('should start an instance after deploying and show success alert', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy first
    const deployAlerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(deployAlerts[0]).toContain('Deployed definition!');

    // Now start instance
    const startAlerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Start Instance' }).click();
    });

    expect(startAlerts.length).toBe(1);
    expect(startAlerts[0]).toContain('Started instance! ID: mock-instance-');
  });

  // ---- 5. View Pending Tasks -------------------------------------------

  test('should display pending tasks when tasks exist', async ({ page }) => {
    // Pre-seed with a pending task
    await injectTauriMock(page, {
      pendingTasks: [
        {
          task_id: 'task-abc-123',
          instance_id: 'inst-xyz-456',
          node_id: 'ReviewDocument',
          assignee: 'alice',
          created_at: '2026-03-15T12:00:00Z',
        },
      ],
    });
    await page.goto('/');

    // Navigate to tasks tab
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();

    // Verify task card renders
    const card = page.locator('.card');
    await expect(card).toBeVisible({ timeout: 5_000 });

    await expect(card.getByText('Task: ReviewDocument')).toBeVisible();
    await expect(card.getByText('Assignee: alice')).toBeVisible();
    await expect(card.getByText('Instance: inst-xyz-456')).toBeVisible();

    // Complete button should exist
    await expect(card.locator('button', { hasText: 'Complete Task' })).toBeVisible();
  });

  // ---- 6. Complete a Task -----------------------------------------------

  test('should complete a pending task and show success alert', async ({ page }) => {
    await injectTauriMock(page, {
      pendingTasks: [
        {
          task_id: 'task-to-complete',
          instance_id: 'inst-001',
          node_id: 'ApproveRequest',
          assignee: 'bob',
          created_at: '2026-03-15T12:00:00Z',
        },
      ],
    });
    await page.goto('/');

    // Navigate to tasks tab
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Complete the task
    const alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Complete Task' }).click();
    });

    expect(alerts.length).toBe(1);
    expect(alerts[0]).toBe('Task completed!');

    // After completion, the task list should refresh and show empty state
    await expect(page.getByText('No pending tasks.')).toBeVisible({ timeout: 5_000 });
  });

  // ---- 7. Refresh Tasks -------------------------------------------------

  test('should refresh task list when clicking Refresh button', async ({ page }) => {
    await injectTauriMock(page, {
      pendingTasks: [
        {
          task_id: 'task-refresh-1',
          instance_id: 'inst-ref-1',
          node_id: 'CheckInventory',
          assignee: 'carol',
          created_at: '2026-03-15T12:00:00Z',
        },
      ],
    });
    await page.goto('/');

    // Navigate to tasks tab  
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Click Refresh – task should still be visible (same state)
    await page.locator('button', { hasText: 'Refresh' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText('Task: CheckInventory')).toBeVisible();
  });

  // ---- 8. Full Workflow: Deploy → Start → View Tasks → Complete ----------

  test('full workflow: deploy, start, view tasks, complete', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Step 1: Deploy
    let alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(alerts[0]).toContain('Deployed definition!');

    // Step 2: Start Instance
    alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Start Instance' }).click();
    });
    expect(alerts[0]).toContain('Started instance!');

    // Step 3: Navigate to Pending Tasks
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    // The default BPMN XML from the modeler only has a StartEvent (no userTask),
    // so no tasks seeded — verify empty state
    await expect(page.getByText('No pending tasks.')).toBeVisible({ timeout: 5_000 });

    // Step 4: Navigate back to modeler
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 5_000 });
  });

  // ---- 9. Instances Tab – empty state -----------------------------------

  test('should navigate to Instances tab and show empty state', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.getByText('No instances found.')).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
  });

  // ---- 10. Instances Tab – pre-seeded instances -------------------------

  test('should display pre-seeded instances with state badges', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-aaa-111',
          definition_id: 'order-process',
          state: 'Running',
          current_node: 'ServiceTask_1',
          audit_log: ['▶ Process started at node \'start\''],
          variables: { order_id: 42 },
        },
        {
          id: 'inst-bbb-222',
          definition_id: 'approval-flow',
          state: 'Completed',
          current_node: 'end',
          audit_log: ['▶ Process started', '⏹ Process completed'],
          variables: {},
        },
      ],
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Instances' }).click();

    const cards = page.locator('.card');
    await expect(cards).toHaveCount(2, { timeout: 5_000 });

    // First instance should show Running badge
    await expect(cards.first().locator('.state-running')).toBeVisible();
    await expect(cards.first().getByText('Definition: order-process')).toBeVisible();

    // Second instance should show Completed badge
    await expect(cards.nth(1).locator('.state-completed')).toBeVisible();
  });

  // ---- 11. Instances Tab – click to see details -------------------------

  test('should show instance details with audit log and variables when clicked', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-detail-001',
          definition_id: 'review-process',
          state: 'Running',
          current_node: 'ReviewTask',
          audit_log: [
            '▶ Process started at node \'start\'',
            '⚙ Executed service task \'validate\' (handler: validate)',
          ],
          variables: { validated: true, score: 95 },
        },
      ],
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Click the instance card
    await page.locator('.card').first().click();

    // Detail view should appear
    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // Audit log entries
    await expect(detail.getByText('Process started')).toBeVisible();
    await expect(detail.getByText('Executed service task')).toBeVisible();

    // Variables JSON
    await expect(detail.locator('.variables-block')).toContainText('"validated": true');
    await expect(detail.locator('.variables-block')).toContainText('"score": 95');

    // Close button
    await detail.locator('button', { hasText: 'Close' }).click();
    await expect(detail).not.toBeVisible();
  });
});
