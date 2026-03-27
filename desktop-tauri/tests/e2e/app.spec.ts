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
  deployedXml: Record<string, string>;
  instances: string[];
  /** XML content returned by the read_bpmn_file mock */
  openFileXml: string | null;
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
    definition_key: string;
    business_key: string;
    state: string | { WaitingOnUserTask: { task_id: string } };
    current_node: string;
    audit_log: string[];
    variables: Record<string, unknown>;
  }>;
}

const DEFAULT_MOCK_STATE: MockState = {
  deployedDefs: [],
  deployedXml: {},
  instances: [],
  openFileXml: null,
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
              // Store XML for download mock
              mockState.deployedXml[defId] = (args.xml as string) || '<mock/>';
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

              // Check if this is the complex Script & Gateway test
              const isComplexTest = Object.values(mockState.deployedXml).some((xml: any) => xml.includes('ServiceTask_Script'));
              if (isComplexTest) {
                const defId = mockState.deployedDefs.length > 0 ? mockState.deployedDefs[mockState.deployedDefs.length - 1] : 'mock-def';
                mockState.processInstances.push({
                  id: instId,
                  definition_key: defId,
                  business_key: 'bk-' + Date.now(),
                  state: { WaitingOnUserTask: { task_id: 'mock-task-' + Date.now() } },
                  current_node: 'UserTask_Approval',
                  audit_log: [
                    "▶ Process started at node 'StartEvent_1'",
                    "⚙ Executed service task 'ServiceTask_Script' (handler: script)",
                    "📜 Executed end script on 'ServiceTask_Script'",
                    "🔀 Gateway 'XOR_Gateway_1' evaluated condition (score > 50) -> took path 'Flow_High'",
                    "⏳ Waiting at user task 'UserTask_Approval'",
                  ],
                  variables: { score: 85, status: "processed" },
                });

                mockState.pendingTasks.push({
                  task_id: 'mock-task-' + Date.now(),
                  instance_id: instId,
                  node_id: 'UserTask_Approval',
                  assignee: 'admin',
                  created_at: new Date().toISOString(),
                });
              }

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

            case 'list_definitions': {
              // Return a definition info entry per deployed def
              resolve(mockState.deployedDefs.map(id => ({ key: id, bpmn_id: id, node_count: 3 })));
              break;
            }

            case 'get_definition_xml': {
              const defId = args.definitionId as string;
              const xml = mockState.deployedXml[defId];
              if (xml) {
                resolve(xml);
              } else {
                reject('No XML for definition: ' + defId);
              }
              break;
            }

            case 'update_instance_variables': {
              const instId = args.instanceId as string;
              const newVars = args.variables as Record<string, unknown>;
              const inst = mockState.processInstances.find((i: any) => i.id === instId);
              if (inst) {
                for (const [k, v] of Object.entries(newVars)) {
                  if (v === null) {
                    delete inst.variables[k];
                  } else {
                    inst.variables[k] = v;
                  }
                }
                resolve(null);
              } else {
                reject('No such instance: ' + instId);
              }
              break;
            }

            case 'delete_instance': {
              const instId = args.instanceId as string;
              mockState.processInstances = mockState.processInstances.filter((i: any) => i.id !== instId);
              mockState.instances = mockState.instances.filter((i: any) => i !== instId);
              resolve(null);
              break;
            }

            case 'delete_definition': {
              const defId = args.definitionId as string;
              const cascade = args.cascade as boolean;
              
              const relatedInstances = mockState.processInstances.filter((i: any) => i.definition_key === defId);
              if (relatedInstances.length > 0 && !cascade) {
                reject('Cannot delete definition: instances still exist');
                break;
              }
              
              if (cascade) {
                mockState.processInstances = mockState.processInstances.filter((i: any) => i.definition_key !== defId);
              }
              
              mockState.deployedDefs = mockState.deployedDefs.filter((id: any) => id !== defId);
              delete mockState.deployedXml[defId];
              resolve(null);
              break;
            }

            // Tauri built-in dialog/save
            case 'plugin:dialog|save': {
              // Return a fake file path so writeTextFile can proceed
              resolve('/tmp/mock-download.bpmn');
              break;
            }

            // Tauri built-in dialog/open
            case 'plugin:dialog|open': {
              // Return a fake file path for the open-file mock
              resolve(mockState.openFileXml ? '/tmp/mock-open.bpmn' : null);
              break;
            }

            // Read BPMN file from local filesystem
            case 'read_bpmn_file': {
              if (mockState.openFileXml) {
                resolve(mockState.openFileXml);
              } else {
                reject('Mock: no openFileXml configured');
              }
              break;
            }

            // Tauri built-in fs/writeTextFile
            case 'plugin:fs|write_file':
            case 'plugin:fs|write_text_file': {
              resolve(null);
              break;
            }

            default:
              // Silently resolve null for Tauri-internal commands
              // (dialog, fs, etc.) that use `tauri` as the cmd.
              if (cmd === 'tauri' || cmd.startsWith('plugin:')) {
                resolve(null);
              } else {
                reject(`command ${cmd} not found`);
              }
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
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
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
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();

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
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    const alerts = await collectAlerts(page, async () => {
      // Open variables dialog
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      // Confirm with default empty object
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
    });

    expect(alerts.length).toBe(1);
    expect(alerts[0]).toBe('Please deploy a process first.');
  });

  // ---- 4. Start Instance after Deploy ----------------------------------

  test('should start an instance after deploying and show success alert', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy first
    const deployAlerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(deployAlerts[0]).toContain('Deployed definition!');

    // Now start instance via variables dialog
    const startAlerts = await collectAlerts(page, async () => {
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
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
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Step 1: Deploy
    let alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(alerts[0]).toContain('Deployed definition!');

    // Step 2: Start Instance via variables dialog
    alerts = await collectAlerts(page, async () => {
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
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
          definition_key: 'order-process-key',
          business_key: 'bk-order-001',
          state: 'Running',
          current_node: 'ServiceTask_1',
          audit_log: ['▶ Process started at node \'start\''],
          variables: { order_id: 42 },
        },
        {
          id: 'inst-bbb-222',
          definition_key: 'approval-flow-key',
          business_key: 'bk-approval-001',
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
    await expect(cards.first().getByText(/Definition:/)).toBeVisible();

    // Second instance should show Completed badge
    await expect(cards.nth(1).locator('.state-completed')).toBeVisible();
  });

  // ---- 11. Instances Tab – click to see details -------------------------

  test('should show instance details with audit log and variables when clicked', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-detail-001',
          definition_key: 'review-process-key',
          business_key: 'bk-review-001',
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

    // Variables Table
    const varTable = detail.locator('.variables-table');
    const tbody = varTable.locator('tbody');
    await expect(tbody).toBeVisible();

    // Check 'validated' (Boolean)
    const validatedRow = tbody.locator('tr').nth(1);
    await expect(validatedRow.locator('input').first()).toHaveValue('validated');
    await expect(validatedRow.locator('.var-checkbox')).toBeChecked();

    // Check 'score' (Number)
    const scoreRow = tbody.locator('tr').nth(0);
    await expect(scoreRow.locator('input').first()).toHaveValue('score');
    await expect(scoreRow.locator('input[type="number"]')).toHaveValue('95');

    // Close button
    await detail.locator('button', { hasText: 'Close' }).click();
    await expect(detail).not.toBeVisible();
  });

  // ---- 12. Deployed Processes – empty state --------------------------------

  test('should navigate to Deployed Processes tab and show empty state', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.getByText('No deployed processes.')).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
  });

  // ---- 13. Deployed Processes – pre-seeded definitions ---------------------

  test('should display pre-seeded definitions with node count', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['order-process', 'approval-flow'],
      deployedXml: {
        'order-process': '<bpmn>order</bpmn>',
        'approval-flow': '<bpmn>approval</bpmn>',
      },
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();

    const cards = page.locator('.card');
    await expect(cards).toHaveCount(2, { timeout: 5_000 });

    await expect(cards.first().getByText('Definition: order-process')).toBeVisible();
    await expect(cards.nth(1).getByText('Definition: approval-flow')).toBeVisible();
  });

  // ---- 13.5 Deployed Processes – instances mapping & navigation --------------

  test('should display running instances under a deployed process and navigate on click', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['mapping-test-def'],
      deployedXml: {
        'mapping-test-def': '<bpmn>mapping</bpmn>',
      },
      processInstances: [
        {
          id: 'inst-mapped-001',
          definition_key: 'mapping-test-def',
          business_key: 'MyTestBusinessKey',
          state: 'Running',
          current_node: 'StartEvent_1',
          audit_log: [],
          variables: { business_key: 'MyTestBusinessKey' },
        },
      ],
    });
    await page.goto('/');

    // 1. Navigate to Deployed Processes
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();

    // 2. The definition card should show "Running Instances" and the mapped instance
    const defCard = page.locator('.card').filter({ hasText: 'Definition: mapping-test-def' });
    await expect(defCard).toBeVisible({ timeout: 5_000 });
    await expect(defCard.getByText('Running Instances')).toBeVisible();

    // The business key fallback logic we implemented should prefer the business_key variable
    const instanceButton = defCard.locator('ul button', { hasText: 'MyTestBusinessKey' });
    await expect(instanceButton).toBeVisible();

    // 3. Click the instance
    await instanceButton.click();

    // 4. It should switch to the Instances tab and open the detailed view of "inst-mapped-001"
    const detailPanel = page.locator('.instance-detail');
    await expect(detailPanel).toBeVisible({ timeout: 5_000 });
    await expect(detailPanel.getByText('Instance Details: inst-map')).toBeVisible();
  });

  // ---- 14. Deployed Processes – download button ---------------------------

  test('should click Download BPMN without error', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['download-test'],
      deployedXml: { 'download-test': '<bpmn>test</bpmn>' },
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Click Download – should complete without crashing.
    // In E2E (non-Tauri), the dialog mock resolves null so the
    // writeTextFile silently succeeds or is skipped.
    await page.locator('button', { hasText: 'Download BPMN' }).click();
    // After click, button should return to normal state (not stuck in "Downloading...")
    await expect(page.locator('button', { hasText: 'Download BPMN' })).toBeVisible({ timeout: 5_000 });
  });

  // ---- 15. Deploy then view in Deployed Processes -------------------------

  test('should show deployed definition after deploying from modeler', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy a process first
    const deployAlerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(deployAlerts[0]).toContain('Deployed definition!');

    // Switch to Deployed Processes tab
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();

    // Should see at least one definition card
    const cards = page.locator('.card');
    await expect(cards).toHaveCount(1, { timeout: 5_000 });
  });

  // ---- 16. View deployed definition in Modeler ---------------------------

  test('should open deployed definition in modeler when clicking View in Modeler', async ({ page }) => {
    const sampleXml = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="ViewTest_1" isExecutable="true">
    <bpmn:startEvent id="StartEvent_View"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="ViewTest_1">
      <bpmndi:BPMNShape id="Shape_Start" bpmnElement="StartEvent_View">
        <dc:Bounds x="180" y="160" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    await injectTauriMock(page, {
      deployedDefs: ['view-test-def'],
      deployedXml: { 'view-test-def': sampleXml },
    });
    await page.goto('/');

    // Navigate to Deployed Processes tab
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Click "View in Modeler" button
    await page.locator('button', { hasText: 'View in Modeler' }).click();

    // Should switch back to Modeler tab and show the bpmn-js container
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // The BPMN Modeler nav-item should be active
    const modelerNav = page.locator('.nav-item', { hasText: 'BPMN Modeler' });
    await expect(modelerNav).toHaveClass(/active/, { timeout: 5_000 });
  });

  // ---- 17. New Diagram resets modeler ------------------------------------

  test('should reset modeler to empty diagram when clicking New Diagram', async ({ page }) => {
    const sampleXml = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="NewDiagramTest_1" isExecutable="true">
    <bpmn:startEvent id="StartEvent_ND"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="NewDiagramTest_1">
      <bpmndi:BPMNShape id="Shape_Start" bpmnElement="StartEvent_ND">
        <dc:Bounds x="180" y="160" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    await injectTauriMock(page, {
      deployedDefs: ['nd-test-def'],
      deployedXml: { 'nd-test-def': sampleXml },
    });
    await page.goto('/');

    // Step 1: View a deployed definition in the Modeler
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });
    await page.locator('button', { hasText: 'View in Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Step 2: Click "New Diagram" to reset
    await page.locator('button', { hasText: 'New Diagram' }).click();

    // Canvas should still be visible (empty diagram loaded)
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 5_000 });

    // Step 3: Clicking "Start Instance" (via dialog) should warn because defId was cleared
    const warnAlerts = await collectAlerts(page, async () => {
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
    });
    expect(warnAlerts.length).toBe(1);
    expect(warnAlerts[0]).toContain('Please deploy a process first');
  });

  // ---- 18. Delete Instance -----------------------------------------------

  test('should delete an instance from the instance list', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-to-delete',
          definition_key: 'def-1',
          business_key: 'bk-1',
          state: 'Running',
          current_node: 'node-1',
          audit_log: [],
          variables: {},
        },
      ],
    });
    
    // Auto-accept the window.confirm dialog
    page.on('dialog', dialog => dialog.accept());
    
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    
    // Card should exist
    const card = page.locator('.card', { hasText: 'inst-to-' });
    await expect(card).toBeVisible();
    
    // Click Delete button on the card list
    await card.locator('button[title="Delete Instance"]').click();
    
    // Instance should disappear
    await expect(card).not.toBeVisible();
    await expect(page.getByText('No instances found.')).toBeVisible();
  });

  // ---- 19. Delete Definition ---------------------------------------------

  test('should delete a definition and cascade instances', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['def-to-delete'],
      deployedXml: { 'def-to-delete': '<bpmn/>' },
      processInstances: [
        {
          id: 'inst-related',
          definition_key: 'def-to-delete',
          business_key: 'bk-rel',
          state: 'Running',
          current_node: 'n1',
          audit_log: [],
          variables: {},
        }
      ]
    });
    
    // Auto-accept cascade confirmation dialog
    page.on('dialog', dialog => dialog.accept());
    
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    
    const card = page.locator('.card', { hasText: 'Definition: def-to-delete' });
    await expect(card).toBeVisible();
    
    // Click Delete Definition
    await card.locator('button[title="Delete Definition"]').click();
    
    // Definition should disappear
    await expect(card).not.toBeVisible();
    await expect(page.getByText('No deployed processes.')).toBeVisible();
  });

  // ---- 20. Start Instance with custom variables --------------------------

  test('should start an instance with custom variables', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy first
    const deployAlerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(deployAlerts[0]).toContain('Deployed definition!');

    // Open variables dialog
    await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
    await expect(page.locator('.vars-dialog')).toBeVisible({ timeout: 3_000 });

    // Type custom variables JSON
    const textarea = page.locator('.vars-textarea');
    await textarea.fill('{"orderId": "ORD-42", "amount": 99.5}');

    // Click Start and expect success
    const startAlerts = await collectAlerts(page, async () => {
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
    });
    expect(startAlerts.length).toBe(1);
    expect(startAlerts[0]).toContain('Started instance! ID: mock-instance-');

    // Dialog should be closed now
    await expect(page.locator('.vars-dialog')).not.toBeVisible();
  });

  // ---- 19. Edit instance variables in detail panel -----------------------

  test('should edit instance variables via Instances detail panel', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-edit-vars-001',
          definition_key: 'edit-vars-key',
          business_key: 'bk-edit-001',
          state: 'Running',
          current_node: 'Task_1',
          audit_log: ['▶ Process started'],
          variables: { status: 'new', priority: 1 },
        },
      ],
    });
    await page.goto('/');

    // Navigate to Instances tab
    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.locator('.card')).toBeVisible({ timeout: 5_000 });

    // Click the instance card to open detail view
    await page.locator('.card').first().click();
    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // Verify current variables in table
    const varTable = detail.locator('.variables-table');
    const tbody = varTable.locator('tbody');
    await expect(tbody).toBeVisible();
    
    // Check initial values
    const priorityRow = tbody.locator('tr').nth(0);
    await expect(priorityRow.locator('input').first()).toHaveValue('priority');
    await expect(priorityRow.locator('input[type="number"]')).toHaveValue('1');
    
    const statusRow = tbody.locator('tr').nth(1);
    await expect(statusRow.locator('input').first()).toHaveValue('status');
    await expect(statusRow.locator('input').nth(1)).toHaveValue('new');

    // Edit variables: update status
    await statusRow.locator('input').nth(1).fill('in-progress');
    
    // delete priority
    await priorityRow.locator('button[title="Delete Variable"]').click();

    // add a new key ('assignee': 'alice')
    await detail.locator('button', { hasText: '+ Add Variable' }).click();
    const newRow = tbody.locator('tr').last();
    await newRow.locator('input').first().fill('assignee');
    await newRow.locator('select').selectOption('String');
    await newRow.locator('input').nth(1).fill('alice');

    // Click Save Variables
    const alerts = await collectAlerts(page, async () => {
      await detail.locator('button', { hasText: 'Save Variables' }).click();
    });
    expect(alerts.length).toBe(1);
    expect(alerts[0]).toContain('Variables saved successfully');

    // After save, verify table state (assignee = 0, status = 1)
    await expect(tbody.locator('tr').nth(0).locator('input').first()).toHaveValue('assignee');
    await expect(tbody.locator('tr').nth(0).locator('input').nth(1)).toHaveValue('alice');
    await expect(tbody.locator('tr').nth(1).locator('input').first()).toHaveValue('status');
    await expect(tbody.locator('tr').nth(1).locator('input').nth(1)).toHaveValue('in-progress');
    await expect(tbody.locator('tr')).toHaveCount(2);
  });
  // ---- 20. Complex Workflow: Script + XOR Gateway --------------------------

  test('complex workflow with rhai script and xor gateway', async ({ page }) => {
    const complexXml = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:camunda="http://camunda.org/schema/1.0/bpmn" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="ComplexProcess_1" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1" />
    <bpmn:serviceTask id="ServiceTask_Script" name="Script Task" camunda:type="script">
      <bpmn:extensionElements>
        <camunda:executionListener event="end">
          <camunda:script scriptFormat="rhai">status = "processed"; score = 85;</camunda:script>
        </camunda:executionListener>
      </bpmn:extensionElements>
    </bpmn:serviceTask>
    <bpmn:exclusiveGateway id="XOR_Gateway_1" default="Flow_Low" />
    <bpmn:userTask id="UserTask_Approval" name="Approval" camunda:assignee="admin" />
    <bpmn:endEvent id="EndEvent_Fail" />
    <bpmn:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="ServiceTask_Script" />
    <bpmn:sequenceFlow id="Flow_2" sourceRef="ServiceTask_Script" targetRef="XOR_Gateway_1" />
    <bpmn:sequenceFlow id="Flow_High" sourceRef="XOR_Gateway_1" targetRef="UserTask_Approval">
      <bpmn:conditionExpression xsi:type="bpmn:tFormalExpression">score &gt; 50</bpmn:conditionExpression>
    </bpmn:sequenceFlow>
    <bpmn:sequenceFlow id="Flow_Low" sourceRef="XOR_Gateway_1" targetRef="EndEvent_Fail" />
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="ComplexProcess_1">
      <bpmndi:BPMNShape id="Shape_UserTask" bpmnElement="UserTask_Approval">
        <dc:Bounds x="300" y="100" width="100" height="80" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    await injectTauriMock(page, {
      deployedDefs: ['mock-complex-def'],
      deployedXml: { 'mock-complex-def': complexXml },
    });
    
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // 1. "Lade das oben beschriebene BPMN-XML"
    // We achieve this in the UI by viewing the pre-injected def:
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await page.locator('button', { hasText: 'View in Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // 2. Klicke "Deploy Process"
    let alerts = await collectAlerts(page, async () => {
      await page.locator('button', { hasText: 'Deploy Process' }).click();
    });
    expect(alerts[0]).toContain('Deployed definition!');

    // 3. Klicke "Start Instance" (ohne initiale Variablen)
    alerts = await collectAlerts(page, async () => {
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
    });
    expect(alerts[0]).toContain('Started instance!');

    // 4. Validierung: Navigiere zum Tab "Instances", klicke auf die neue Instanz
    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.locator('.card').first()).toBeVisible({ timeout: 5_000 });
    await page.locator('.card').first().click();

    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // In the Instances view, details are toggled by clicking the active node in the viewer
    const activeNode = page.locator('[data-element-id="UserTask_Approval"]');
    await expect(activeNode).toBeVisible({ timeout: 10_000 });
    await activeNode.click();

    // 5. Prüfe Variables
    const varTableComplex = detail.locator('.variables-table');
    const tbodyComplex = varTableComplex.locator('tbody');
    
    const scoreRowComplex = tbodyComplex.locator('tr').nth(0); // 'score' is 0, 'status' is 1
    await expect(scoreRowComplex.locator('input').first()).toHaveValue('score');
    await expect(scoreRowComplex.locator('input[type="number"]')).toHaveValue('85');
    
    const statusRowComplex = tbodyComplex.locator('tr').nth(1);
    await expect(statusRowComplex.locator('input').first()).toHaveValue('status');
    await expect(statusRowComplex.locator('input').nth(1)).toHaveValue('processed');

    // 6. Prüfe Audit-Log
    await expect(detail.getByText("Executed end script on 'ServiceTask_Script'")).toBeVisible();
    await expect(detail.getByText("evaluated condition (score > 50) -> took path 'Flow_High'")).toBeVisible();

    // Close detail
    await detail.locator('button', { hasText: 'Close' }).click();

    // 7. Navigiere zu "Pending Tasks" und verifiziere, dass UserTask_Approval dort erscheint
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    const taskCard = page.locator('.card').filter({ hasText: 'Task: UserTask_Approval' });
    await expect(taskCard).toBeVisible({ timeout: 5_000 });
    await expect(taskCard.getByText('Assignee: admin')).toBeVisible();
  });

  // ---- 21. Open BPMN file from filesystem ---------------------------------

  test('should open a BPMN file from filesystem and load it into the modeler', async ({ page }) => {
    const fileXml = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="OpenFileTest_1" isExecutable="true">
    <bpmn:startEvent id="StartEvent_Open"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="OpenFileTest_1">
      <bpmndi:BPMNShape id="Shape_Start" bpmnElement="StartEvent_Open">
        <dc:Bounds x="180" y="160" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

    await injectTauriMock(page, { openFileXml: fileXml });
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Click "Open File" – the mock dialog returns a path, read_bpmn_file returns XML
    await page.locator('button', { hasText: 'Open File' }).click();

    // Canvas should still be visible after loading
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // defId was reset, so "Start Instance" should warn about missing deploy
    const alerts = await collectAlerts(page, async () => {
      await page.locator('.header-actions button', { hasText: 'Start Instance' }).click();
      await page.locator('.vars-dialog button', { hasText: 'Start' }).click();
    });
    expect(alerts.length).toBe(1);
    expect(alerts[0]).toBe('Please deploy a process first.');
  });
});
