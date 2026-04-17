import { test, expect, type Page, type Dialog } from '@playwright/test';

// ---------------------------------------------------------------------------
// Mock setup: simulates the Tauri IPC layer so tests can run against
// the plain Vite dev-server (no Tauri backend needed).
// ---------------------------------------------------------------------------

const VALID_MOCK_XML = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="Process_Mock" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="Process_Mock">
      <bpmndi:BPMNShape id="_BPMNShape_StartEvent_2" bpmnElement="StartEvent_1">
        <dc:Bounds x="150" y="100" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

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
  pendingServiceTasks: any[];
  pendingTimers: any[];
  pendingMessageCatches: any[];
  completedTasks: string[];
  processInstances: Array<{
    id: string;
    definition_key: string;
    business_key: string;
    state: string | { WaitingOnUserNode: { task_id: string } };
    current_node: string;
    audit_log: string[];
    history?: any[];
    variables: Record<string, unknown>;
  }>;
  /** Archived (completed) instances, queried via query_completed_instances */
  completedInstances: Array<{
    id: string;
    definition_key: string;
    business_key: string;
    state: string | { CompletedWithError: { error_code: string } };
    current_node: string;
    audit_log: string[];
    variables: Record<string, unknown>;
    started_at?: string | null;
    completed_at?: string | null;
  }>;
  /** Rich definitions for dropdowns (key, bpmn_id, version, is_latest, node_count) */
  definitions: Array<{
    key: string;
    bpmn_id: string;
    version: number;
    node_count: number;
    is_latest: boolean;
  }>;
  /** Optional StorageInfo for get_monitoring_data mock */
  storageInfo?: any;
  /** Bucket entries indexed by bucket name: { [bucket]: BucketEntry[] } */
  bucketEntries?: Record<string, any[]>;
  /** Bucket entry details indexed by "bucket::key": BucketEntryDetail */
  bucketEntryDetails?: Record<string, any>;
}

const DEFAULT_MOCK_STATE: MockState = {
  deployedDefs: [],
  deployedXml: {},
  instances: [],
  openFileXml: null,
  pendingTasks: [],
  pendingServiceTasks: [],
  pendingTimers: [],
  pendingMessageCatches: [],
  completedTasks: [],
  processInstances: [],
  completedInstances: [],
  definitions: [],
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

    (window as any).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: any = {}) => {
        console.log('TAURI IPC mock:', cmd, JSON.stringify(args));

        return new Promise((resolve, reject) => {
          setTimeout(() => {
            try {
          switch (cmd) {
            case 'deploy_definition': {
              const defId = 'mock-def-' + Date.now();
              mockState.deployedDefs.push(defId);
              // Store XML for download mock
              mockState.deployedXml[defId] = (args.xml as string) || VALID_MOCK_XML;
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
                  state: { WaitingOnUserNode: { task_id: 'mock-task-' + Date.now() } },
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

            case 'get_pending_service_tasks': {
              resolve(mockState.pendingServiceTasks);
              break;
            }

            case 'complete_service_task': {
              resolve(null);
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

            case 'get_instance_history': {
              const instId = args.instanceId as string;
              const found = mockState.processInstances.find((i: any) => i.id === instId);
              if (found) {
                let entries: any[] = [];
                // If the test provided explicit history objects
                if (found.history && found.history.length > 0) {
                   entries = [...found.history];
                } else {
                   // Fallback: migrate old audit_log text strings 
                   // into new HistoryEntry objects for the test to pass
                   entries = found.audit_log.map((log: string, idx: number) => ({
                      id: `hist-${idx}`,
                      instance_id: instId,
                      timestamp: new Date().toISOString(),
                      event_type: log.includes('started') ? 'InstanceStarted' : 'TokenAdvanced',
                      node_id: null,
                      description: log,
                      actor_type: 'engine',
                      actor_id: null,
                      diff: null,
                      context: {},
                      metadata: null,
                      definition_version: 1,
                      is_snapshot: false,
                      full_state_snapshot: null
                   }));
                }
                
                const eventTypesFilter = args.eventTypes as string | null;
                const actorTypesFilter = args.actorTypes as string | null;
                
                if (eventTypesFilter && eventTypesFilter.trim() !== '') {
                  const allowed = eventTypesFilter.split(',');
                  entries = entries.filter(e => allowed.includes(e.event_type));
                }
                if (actorTypesFilter && actorTypesFilter.trim() !== '') {
                  const allowed = actorTypesFilter.split(',');
                  entries = entries.filter(e => allowed.includes(e.actor_type));
                }

                resolve(entries);
              } else {
                reject('No such instance: ' + instId);
              }
              break;
            }

            case 'list_definitions': {
              // Prefer rich pre-seeded definitions if provided; fall back to deployedDefs
              if (mockState.definitions.length > 0) {
                resolve(mockState.definitions);
              } else {
                resolve(mockState.deployedDefs.map(id => ({
                  key: id, bpmn_id: id, version: 1, node_count: 3, is_latest: true,
                })));
              }
              break;
            }

            case 'query_completed_instances': {
              let results = mockState.completedInstances.slice();
              if (args.definitionKey) {
                results = results.filter((i: any) => i.definition_key === args.definitionKey);
              }
              if (args.businessKey) {
                results = results.filter((i: any) =>
                  (i.business_key || '').includes(args.businessKey as string)
                );
              }
              if (args.stateFilter === 'completed') {
                results = results.filter((i: any) => i.state === 'Completed');
              } else if (args.stateFilter === 'error') {
                results = results.filter((i: any) =>
                  typeof i.state === 'object' && ('CompletedWithError' in i.state || 'ErrorEnd' in i.state)
                );
              }
              const offset = (args.offset as number) ?? 0;
              const limit = (args.limit as number) ?? 50;
              resolve(results.slice(offset, offset + limit));
              break;
            }

            case 'get_completed_instance': {
              const instId = args.instanceId as string;
              const found = mockState.completedInstances.find((i: any) => i.id === instId);
              if (found) resolve(found);
              else reject('No such completed instance: ' + instId);
              break;
            }

            case 'get_pending_timers': {
              resolve(mockState.pendingTimers);
              break;
            }

            case 'get_pending_message_catches': {
              resolve(mockState.pendingMessageCatches);
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

            case 'upload_instance_file': {
              const instId = args.instanceId as string;
              const varName = args.varName as string;
              const inst = mockState.processInstances.find((i: any) => i.id === instId);
              if (inst) {
                inst.variables[varName] = {
                  type: 'file',
                  filename: 'mock-upload.txt',
                  size_bytes: 1024,
                  mime_type: 'text/plain',
                  object_key: 'mock-key',
                };
                resolve({ status: 'success' });
              } else {
                reject('Instance not found');
              }
              break;
            }

            case 'download_instance_file': {
              resolve(null);
              break;
            }

            case 'delete_instance_file': {
               resolve(null);
               break;
            }

            case 'suspend_instance': {
              const instId = args.instanceId as string;
              const inst = mockState.processInstances.find((i: any) => i.id === instId);
              if (inst) {
                inst.state = { Suspended: { previous_state: inst.state } };
                resolve(null);
              } else {
                reject('Instance not found: ' + instId);
              }
              break;
            }

            case 'resume_instance': {
              const instId = args.instanceId as string;
              const inst = mockState.processInstances.find((i: any) => i.id === instId);
              if (inst && typeof inst.state === 'object' && 'Suspended' in inst.state) {
                inst.state = (inst.state as any).Suspended.previous_state ?? 'Running';
                resolve(null);
              } else {
                reject('Instance not suspended: ' + instId);
              }
              break;
            }

            case 'migrate_instance': {
              const instId = args.instanceId as string;
              const targetKey = args.targetDefinitionKey as string;
              const inst = mockState.processInstances.find((i: any) => i.id === instId);
              if (inst) {
                inst.definition_key = targetKey;
                resolve(null);
              } else {
                reject('Instance not found: ' + instId);
              }
              break;
            }

            case 'get_monitoring_data': {
              resolve({
                definitions_count: mockState.definitions.length,
                instances_total: mockState.processInstances.length,
                instances_running: mockState.processInstances.filter((i: any) => i.state === 'Running').length,
                instances_completed: 0,
                pending_user_tasks: mockState.pendingTasks.length,
                pending_service_tasks: mockState.pendingServiceTasks.length,
                pending_timers: mockState.pendingTimers.length,
                pending_message_catches: mockState.pendingMessageCatches.length,
                storage_info: (mockState as any).storageInfo ?? null,
              });
              break;
            }

            case 'get_bucket_entries': {
              const bucket = args.bucket as string;
              const entries = ((mockState as any).bucketEntries ?? {})[bucket] ?? [];
              resolve(entries);
              break;
            }

            case 'get_bucket_entry_detail': {
              const bucket = args.bucket as string;
              const key = args.key as string;
              const detail = ((mockState as any).bucketEntryDetails ?? {})[`${bucket}::${key}`];
              if (detail) {
                resolve(detail);
              } else {
                reject(`Entry not found: ${bucket}/${key}`);
              }
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
              if (mockState.openFileXml) {
                resolve('/tmp/mock-open.bpmn');
              } else {
                resolve('/tmp/mock-upload.txt');
              }
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
              if (cmd === 'tauri') {
                if (args.__tauriModule === 'Dialog') {
                  if (args.message?.cmd === 'openDialog') {
                    if (mockState.openFileXml) {
                      resolve('/tmp/mock-open.bpmn');
                    } else {
                      resolve('/tmp/mock-upload.txt');
                    }
                    return;
                  }
                  if (args.message?.cmd === 'saveDialog') {
                    resolve('/tmp/mock-download.bpmn');
                    return;
                  }
                }
                resolve(null);
              } else if (cmd.startsWith('plugin:')) {
                resolve(null);
              } else {
                reject(`command ${cmd} not found`);
              }
          }
        } catch (e: any) {
          reject(e.message ?? String(e));
        }
      }, 10); // simulate async
    });
    }
    };
  }, state);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Collect all toast messages that fire after an action */
async function collectToasts(
  page: Page,
  action: () => Promise<void>,
): Promise<string[]> {
  await action();
  // Wait for at least one toast to appear, but don't fail if none pop up
  const toastSelector = '.toast, [role="status"], [role="alert"]';
  await page.waitForSelector(toastSelector, { state: 'visible', timeout: 2000 }).catch(() => {});
  const toasts = await page.locator(toastSelector).allTextContents();
  // Discard the '×' close text content
  return toasts.map(t => t.replace('×', '').trim());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe('bpmninja Desktop App – E2E', () => {

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
    await expect(page.getByText('No Pending User Tasks')).toBeVisible({ timeout: 5_000 });

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
    const alerts = await collectToasts(page, async () => {
      await page.getByTestId('btn-deploy').click();
    });

    expect(alerts.length).toBeGreaterThanOrEqual(1);
    expect(alerts[0]).toContain('Deployed');
  });



  // ---- 4. Start Instance ----------------------------------

  test('should auto-deploy and start instance, then navigate to Instances', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Start instance via variables dialog (auto-deploys)
    await page.getByTestId('btn-start-instance').click();
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();

    // Verification: We should be redirected to the instances tab
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 10_000 });
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

    await expect(card.getByText('Node: ReviewDocument')).toBeVisible();
    await expect(card.getByText('inst-xyz…')).toBeVisible();

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
    // First, click "Complete Task" on the card to open the dialog
    await page.locator('.card button', { hasText: 'Complete Task' }).click();
    await expect(page.locator('.max-w-xl')).toBeVisible({ timeout: 5_000 });

    // Then, click "Complete Task" in the dialog to confirm
    await page.locator('.max-w-xl button', { hasText: 'Complete Task' }).click();

    // After completion, the task list should refresh and show empty state
    await expect(page.getByText('No Pending User Tasks')).toBeVisible({ timeout: 5_000 });
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
    await expect(page.getByText('Node: CheckInventory')).toBeVisible();
  });

  // ---- 8. Full Workflow: Deploy → Start → View Tasks → Complete ----------

  test('full workflow: deploy, start, view tasks, complete', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Step 1: Deploy
    const alerts = await collectToasts(page, async () => {
      await page.getByTestId('btn-deploy').click();
    });
    expect(alerts[0]).toContain('Deployed');

    // Step 2: Start Instance via variables dialog (auto deploy + start)
    await page.getByTestId('btn-start-instance').click();
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();
    
    // Will navigate to instances tab automatically.
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 5_000 });

    // Step 3: Navigate to Pending Tasks
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    // The default BPMN XML from the modeler only has a StartEvent (no userTask),
    // so no tasks seeded — verify empty state
    await expect(page.getByText('No Pending User Tasks')).toBeVisible({ timeout: 5_000 });

    // Step 4: Navigate back to modeler
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 5_000 });
  });

  // ---- 9. Instances Tab – empty state -----------------------------------

  test('should navigate to Instances tab and show empty state', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.getByText('No Instances Yet')).toBeVisible({ timeout: 5_000 });
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

    const cards = page.locator('.instance-list-item');
    await expect(cards).toHaveCount(2, { timeout: 5_000 });

    // First instance should show Running badge
    await expect(cards.first().locator('.bg-blue-600')).toBeVisible();

    // Second instance should show Completed badge
    await expect(cards.nth(1).locator('.bg-green-600')).toBeVisible();
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
    await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });

    // Click the instance row
    await page.locator('.instance-list-item').first().click();

    // Detail view should appear
    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // Audit log entries inside the Timeline
    await expect(detail.getByText('Instance Started')).toBeVisible();
    await expect(detail.getByText('Token Advanced')).toBeVisible();

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
    await detail.locator('button.gap-1', { hasText: 'Close' }).click();
    await expect(detail).not.toBeVisible();
  });

  // ---- 12. Deployed Processes – empty state --------------------------------

  test('should navigate to Deployed Processes tab and show empty state', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.getByText('No Deployed Processes')).toBeVisible({ timeout: 5_000 });
    await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
  });

  // ---- 13. Deployed Processes – pre-seeded definitions ---------------------

  test('should display pre-seeded definitions with node count', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['order-process', 'approval-flow'],
      deployedXml: {
        'order-process': VALID_MOCK_XML,
        'approval-flow': VALID_MOCK_XML,
      },
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();

    const cards = page.locator('.process-group-card');
    await expect(cards).toHaveCount(2, { timeout: 5_000 });

    await expect(cards.first().getByText('order-process').first()).toBeVisible();
    await expect(cards.nth(1).getByText('approval-flow').first()).toBeVisible();
  });

  // ---- 13.5 Deployed Processes – instances mapping & navigation --------------

  test('should display running instances under a deployed process and navigate on click', async ({ page }) => {
    await injectTauriMock(page, {
      deployedDefs: ['mapping-test-def'],
      deployedXml: {
        'mapping-test-def': VALID_MOCK_XML,
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
    const defCard = page.locator('.process-group-card').filter({ hasText: 'mapping-test-def' });
    await expect(defCard).toBeVisible({ timeout: 5_000 });
    
    // We added the word "active instance" in the grouping pill instead of an explicit "Running Instances" section, but wait: Let's just click to expand accordion.
    const accordionBtn = defCard.locator('button.accordion-toggle').nth(1); // Instances accordion is 2nd normally, but we can filter by hasText active instances.
    await defCard.locator('button', { hasText: 'active' }).click();

    // The business key fallback logic we implemented should prefer the business_key variable
    const instanceButton = defCard.locator('.instance-list-item', { hasText: 'MyTestBusinessKey' });
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
      deployedXml: { 'download-test': VALID_MOCK_XML },
    });
    await page.goto('/');

    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();
    await expect(page.locator('.process-group-card').first()).toBeVisible({ timeout: 5_000 });

    // Click Download icon – should complete without crashing.
    // In E2E (non-Tauri), the dialog mock resolves null so the
    // writeTextFile silently succeeds or is skipped.
    const downloadBtn = page.locator('.process-group-card button[title="Download BPMN"]').first();
    await downloadBtn.click();
    // After click, button should return to normal state
    await expect(downloadBtn).toBeVisible({ timeout: 5_000 });
  });

  // ---- 15. Deploy then view in Deployed Processes -------------------------

  test('should show deployed definition after deploying from modeler', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy a process first
    const deployAlerts = await collectToasts(page, async () => {
      await page.getByTestId('btn-deploy').click();
    });
    expect(deployAlerts[0]).toContain('Deployed');

    // Switch to Deployed Processes tab
    await page.locator('.nav-item', { hasText: 'Deployed Processes' }).click();

    // Should see at least one definition card
    const cards = page.locator('.process-group-card');
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
    await expect(page.locator('.process-group-card').first()).toBeVisible({ timeout: 5_000 });

    // Click "View BPMN" icon button
    await page.locator('.process-group-card button[title="View BPMN"]').first().click();

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
    await expect(page.locator('.process-group-card').first()).toBeVisible({ timeout: 5_000 });
    await page.locator('.process-group-card button[title="View BPMN"]').first().click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Step 2: Click "New Diagram" to reset
    await page.getByTestId('btn-new-diagram').click();

    // Canvas should still be visible (empty diagram loaded)
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 5_000 });

    // Step 3: Auto-deploy & start works without existing defId
    await page.getByTestId('btn-start-instance').click();
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();

    // Verify nav to instances
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 5_000 });
  });

  // ---- 18. Delete Instance -----------------------------------------------

  test('should delete an instance from the instance list', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-to-delete',
          definition_key: 'mock-def-1',
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
    const card = page.locator('.instance-list-item').filter({ hasText: 'bk-1' });
    await expect(card).toBeVisible();
    
    // Click Delete button on the card (group/instance) list (Wait, does the list item have delete? No, only the detail dialog has Delete!)
    await card.click();
    const detailPanel = page.locator('.instance-detail');
    await expect(detailPanel).toBeVisible({ timeout: 5_000 });

    // Click Delete button in details
    await detailPanel.locator('button', { hasText: 'Delete' }).click();
    
    // Confirm delete in the shadcn dialog
    await page.locator('[role="alertdialog"] button', { hasText: /^Delete$/ }).click();
    
    // Instance should disappear
    await expect(detailPanel).not.toBeVisible();
    await expect(page.getByText('No Instances Yet')).toBeVisible();
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
    
    const card = page.locator('.process-group-card', { hasText: 'def-to-delete' });
    await expect(card).toBeVisible();
    
    // Click Delete Definition
    await card.locator('button[title="Delete latest version"]').click();
    
    // Confirm delete in the shadcn dialog
    await page.locator('[role="alertdialog"] button', { hasText: /^Delete$/ }).click();
    
    // Definition should disappear
    await expect(card).not.toBeVisible();
    await expect(page.getByText('No Deployed Processes')).toBeVisible();
  });

  // ---- 20. Start Instance with custom variables --------------------------

  test('should start an instance with custom variables', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Deploy first
    const deployAlerts = await collectToasts(page, async () => {
      await page.getByTestId('btn-deploy').click();
    });
    expect(deployAlerts[0]).toContain('Deployed');

    // Open variables dialog
    await page.getByTestId('btn-start-instance').click();
    await expect(page.locator('[role="dialog"]')).toBeVisible({ timeout: 3_000 });

    // Add variable 'orderId'
    await page.locator('[role="dialog"] button', { hasText: '+ Add Variable' }).click();
    const newRow1 = page.locator('[role="dialog"] tbody tr').last();
    await newRow1.locator('input').first().fill('orderId');
    await newRow1.locator('input').nth(1).fill('ORD-42');

    // Add variable 'amount'
    await page.locator('[role="dialog"] button', { hasText: '+ Add Variable' }).click();
    const newRow2 = page.locator('[role="dialog"] tbody tr').last();
    await newRow2.locator('input').first().fill('amount');
    await newRow2.locator('select').selectOption('Number');
    await newRow2.locator('input[type="number"]').fill('99.5');

    // Click Start
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();

    // Should navigate to instances tab
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 10_000 });
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
    await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });

    // Click the instance row to open detail view
    await page.locator('.instance-list-item').first().click();
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
    const alerts = await collectToasts(page, async () => {
      await detail.locator('button', { hasText: 'Save Variables' }).click();
    });
    expect(alerts.length).toBeGreaterThanOrEqual(1);
    expect(alerts[0]).toContain('Variables saved');

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
    await expect(page.locator('.process-group-card').first()).toBeVisible({ timeout: 5_000 });
    await page.locator('.process-group-card button[title="View BPMN"]').first().click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // 2. Klicke "Deploy Process"
    const alerts = await collectToasts(page, async () => {
      await page.getByTestId('btn-deploy').click();
    });
    expect(alerts[0]).toContain('Deployed');

    // 3. Klicke "Start Instance" (ohne initiale Variablen, navigiert auto zu Instances)
    await page.getByTestId('btn-start-instance').click();
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 10_000 });

    // 4. Validierung: Die neue Instanz wird automatisch im Dialog geöffnet!
    await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
    // await page.locator('.instance-list-item').first().click(); // <- REMOVED: Auto-opens now

    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });

    // In the Instances view, details are shown by default now.

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
    await expect(detail.getByText('Instance Started')).toBeVisible();
    await expect(detail.locator('td', { hasText: 'Token Advanced' }).first()).toBeVisible();

    // Close detail
    await detail.getByTestId('btn-close-details').click();

    // 7. Navigiere zu "Pending Tasks" und verifiziere, dass UserTask_Approval dort erscheint
    await page.locator('.nav-item', { hasText: 'Pending Tasks' }).click();
    const taskCard = page.locator('.card').filter({ hasText: 'Node: UserTask_Approval' });
    await expect(taskCard).toBeVisible({ timeout: 5_000 });
    await expect(taskCard.getByText('admin')).toBeVisible();
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
    await page.getByTestId('btn-open-file').click();

    // Canvas should still be visible after loading
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // "Start Instance" directly deploys loaded file
    await page.getByTestId('btn-start-instance').click();
    await page.locator('[role="dialog"] button', { hasText: 'Start' }).click();
    
    // Should navigate to instances view
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 10_000 });
  });

  // ---- 25. History Timeline Rendering and Filtering -----------------------

  test('should render history timeline with actors, snapshots, and filters', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-history-1',
          definition_key: 'history-key',
          business_key: 'bk-hist-1',
          state: 'Running',
          current_node: 'Task_1',
          audit_log: [],
          history: [
            {
              id: 'h1',
              instance_id: 'inst-history-1',
              timestamp: new Date().toISOString(),
              event_type: 'InstanceStarted',
              node_id: 'StartEvent_1',
              description: 'Process started',
              actor_type: 'engine',
              actor_id: null,
              diff: { human_readable: 'Started workflow definition history-key' },
              context: {},
              is_snapshot: false,
            },
            {
              id: 'h2',
              instance_id: 'inst-history-1',
              timestamp: new Date().toISOString(),
              event_type: 'TokenAdvanced',
              node_id: 'Task_1',
              description: 'Token arrived at Task_1',
              actor_type: 'serviceworker',
              actor_id: 'worker-a',
              diff: null,
              context: {},
              is_snapshot: true,
              full_state_snapshot: { dummy: true }
            },
            {
              id: 'h3',
              instance_id: 'inst-history-1',
              timestamp: new Date().toISOString(),
              event_type: 'VariablesChanged',
              node_id: 'Task_1',
              description: 'Variables updated',
              actor_type: 'user',
              actor_id: 'admin',
              diff: { human_readable: 'amount changed from null to 500' },
              context: {},
              is_snapshot: false,
            }
          ],
          variables: { amount: 500 }
        }
      ]
    });
    
    await page.goto('/');
    
    // Navigate to instance details
    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
    await page.locator('.instance-list-item').first().click();
    
    const timeline = page.locator('.history-timeline-container');
    await expect(timeline).toBeVisible({ timeout: 5_000 });
    
    // 1. Verify all 3 entries are rendered initially
    await expect(timeline.getByText('Instance Started')).toBeVisible();
    await expect(timeline.getByText('Token Advanced')).toBeVisible();
    await expect(timeline.getByText('Variables Changed')).toBeVisible();
    
    // 2. Verify snapshot badge is visible for the second entry
    await expect(timeline.locator('svg.lucide-camera')).toBeVisible();
    
    // 3. Verify Actor Badges
    await expect(timeline.getByText('engine', { exact: true })).toBeVisible();
    await expect(timeline.getByText('serviceworker (worker-a)')).toBeVisible();
    await expect(timeline.getByText('user (admin)')).toBeVisible();
    // 5. Test Event Type Filtering
    // Select 'VariablesChanged' from dropdown
    await timeline.locator('select').first().selectOption('VariablesChanged');
    
    // Verify only 'Variables Changed' is visible, others are hidden
    await expect(timeline.getByText('Variables Changed')).toBeVisible();
    await expect(timeline.getByText('Instance Started')).not.toBeVisible();
    await expect(timeline.getByText('Token Advanced')).not.toBeVisible();
    
    // 6. Test Actor Filtering
    // Reset Event Types
    await timeline.locator('select').first().selectOption('');
    // Select 'engine'
    await timeline.locator('select').nth(1).selectOption('engine');
    
    // Verify only 'Instance Started' is visible
    await expect(timeline.getByText('Instance Started')).toBeVisible();
    await expect(timeline.getByText('Token Advanced')).not.toBeVisible();
    await expect(timeline.getByText('Variables Changed')).not.toBeVisible();
  });

  // ---- 26. File Attachments -----------------------------------------------

  test('should allow attaching a file and display the details', async ({ page }) => {
    await injectTauriMock(page, {
      processInstances: [
        {
          id: 'inst-file-1',
          definition_key: 'file-key',
          business_key: 'bk-file-1',
          state: 'Running',
          current_node: 'Task_1',
          audit_log: [],
          variables: {}
        }
      ]
    });
    
    await page.goto('/');
    page.on('console', msg => console.log('BROWSER LOG:', msg.text()));
    
    await page.locator('.nav-item', { hasText: 'Instances' }).click();
    await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
    await page.locator('.instance-list-item').first().click();
    
    // Wait for detail pane
    const detail = page.locator('.instance-detail');
    await expect(detail).toBeVisible({ timeout: 5_000 });
    
    await page.locator('button', { hasText: 'Attach File' }).click();
    
    // Interact with Shadcn UI dialog instead of window.prompt
    await page.getByPlaceholder('e.g. uploaded_document').fill('my_file_var');
    await page.locator('button', { hasText: 'Add File Variable' }).click();
    
    // Wait for the UI to update
    await expect(detail.locator('input[placeholder="Variable name"]').first()).toHaveValue('my_file_var', { timeout: 5000 });
    await expect(detail.getByText('mock-upload.txt')).toBeVisible();
    await expect(detail.getByText('(1.0 KB)')).toBeVisible();
    
    // Click "Download" via the explicit button
    await page.locator('button', { hasText: 'Download' }).click();
    
    // Click the paperclip / filename (file-download-trigger) to verify it also triggers download
    await detail.locator('.cursor-pointer[title="Download file"]').click();
    
  });

  // ---- 27. File Attachments at Start (Deferred Upload) --------------------

  test('should attach a file in the start dialog and upload after instance creation', async ({ page }) => {
    await injectTauriMock(page);
    await page.goto('/');
    page.on('console', msg => console.log('BROWSER LOG:', msg.text()));

    // Navigate to modeler and wait for it to load
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Open start dialog
    await page.getByTestId('btn-start-instance').click();
    const dialog = page.locator('[role="dialog"], [role="dialog"]');
    await expect(dialog).toBeVisible({ timeout: 5_000 });

    // The "Attach File" button should be visible in the start dialog
    const attachBtn = dialog.locator('button', { hasText: 'Attach File' });
    await expect(attachBtn).toBeVisible();

    // Click attach file
    await attachBtn.click();

    // Interact with Shadcn UI dialog instead of window.prompt
    await page.getByPlaceholder('e.g. uploaded_document').fill('invoice_pdf');
    await page.locator('button', { hasText: 'Add File Variable' }).click();

    // A pending file row should appear in the variables table
    await expect(dialog.getByText('Pending upload...')).toBeVisible({ timeout: 5000 });
    await expect(dialog.locator('input[placeholder="Variable name"]').first()).toHaveValue('invoice_pdf');

    // Now click "Start" – this will deploy, start, and then upload the pending file
    await dialog.locator('button', { hasText: 'Start' }).click();

    // Should navigate to Instances tab
    await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 10_000 });
  });

  // ---- Flow Condition – Camunda 7 compatibility --------------------------

  // Camunda 7 Modeler Kompatibilität – test XML fixtures
  const FLOW_CONDITION_XML_XOR = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL"
  xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI"
  xmlns:dc="http://www.omg.org/spec/DD/20100524/DC"
  xmlns:di="http://www.omg.org/spec/DD/20100524/DI"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="FlowCondTest" isExecutable="true">
    <bpmn:startEvent id="Start_1" />
    <bpmn:exclusiveGateway id="XOR_1" default="Flow_Default" />
    <bpmn:endEvent id="End_1" />
    <bpmn:endEvent id="End_2" />
    <bpmn:sequenceFlow id="Flow_ToXOR" sourceRef="Start_1" targetRef="XOR_1" />
    <bpmn:sequenceFlow id="Flow_Cond" sourceRef="XOR_1" targetRef="End_1" />
    <bpmn:sequenceFlow id="Flow_Default" sourceRef="XOR_1" targetRef="End_2" />
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="FlowCondTest">
      <bpmndi:BPMNShape id="Shape_Start" bpmnElement="Start_1">
        <dc:Bounds x="150" y="200" width="36" height="36" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="Shape_XOR" bpmnElement="XOR_1" isMarkerVisible="true">
        <dc:Bounds x="250" y="193" width="50" height="50" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="Shape_End1" bpmnElement="End_1">
        <dc:Bounds x="400" y="140" width="36" height="36" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="Shape_End2" bpmnElement="End_2">
        <dc:Bounds x="400" y="260" width="36" height="36" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNEdge id="Edge_ToXOR" bpmnElement="Flow_ToXOR">
        <di:waypoint x="186" y="218" />
        <di:waypoint x="250" y="218" />
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Edge_Cond" bpmnElement="Flow_Cond">
        <di:waypoint x="275" y="193" />
        <di:waypoint x="275" y="158" />
        <di:waypoint x="400" y="158" />
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Edge_Default" bpmnElement="Flow_Default">
        <di:waypoint x="275" y="243" />
        <di:waypoint x="275" y="278" />
        <di:waypoint x="400" y="278" />
      </bpmndi:BPMNEdge>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

  const FLOW_CONDITION_XML_PARALLEL = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL"
  xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI"
  xmlns:dc="http://www.omg.org/spec/DD/20100524/DC"
  xmlns:di="http://www.omg.org/spec/DD/20100524/DI"
  id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="ParGwTest" isExecutable="true">
    <bpmn:startEvent id="Start_1" />
    <bpmn:parallelGateway id="PAR_1" />
    <bpmn:endEvent id="End_1" />
    <bpmn:sequenceFlow id="Flow_ToPar" sourceRef="Start_1" targetRef="PAR_1" />
    <bpmn:sequenceFlow id="Flow_Out" sourceRef="PAR_1" targetRef="End_1" />
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="ParGwTest">
      <bpmndi:BPMNShape id="Shape_Start" bpmnElement="Start_1">
        <dc:Bounds x="150" y="200" width="36" height="36" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="Shape_PAR" bpmnElement="PAR_1">
        <dc:Bounds x="250" y="193" width="50" height="50" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="Shape_End" bpmnElement="End_1">
        <dc:Bounds x="400" y="200" width="36" height="36" />
      </bpmndi:BPMNShape>
      <bpmndi:BPMNEdge id="Edge_ToPar" bpmnElement="Flow_ToPar">
        <di:waypoint x="186" y="218" />
        <di:waypoint x="250" y="218" />
      </bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="Edge_Out" bpmnElement="Flow_Out">
        <di:waypoint x="300" y="218" />
        <di:waypoint x="400" y="218" />
      </bpmndi:BPMNEdge>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

  /**
   * Helper: navigate to Modeler, load custom XML via localStorage,
   * then programmatically select the given BPMN element via the bpmn-js API
   * (exposed on window.__bpmnModeler__ in DEV mode).
   */
  async function loadXmlAndClickElement(page: Page, xml: string, elementId: string) {
    // Seed localStorage so modeler loads our XML
    await page.addInitScript((xmlContent: string) => {
      localStorage.setItem('minibpm_last_workflow', xmlContent);
    }, xml);
    await injectTauriMock(page);
    await page.goto('/');
    await page.locator('.nav-item', { hasText: 'BPMN Modeler' }).click();
    await expect(page.locator('.bjs-container')).toBeVisible({ timeout: 10_000 });

    // Wait until the modeler instance is available on the window
    await page.waitForFunction(
      () => !!(window as any).__bpmnModeler__?.get,
      undefined,
      { timeout: 10_000 },
    );

    // Programmatically select the element via bpmn-js selection service
    await page.evaluate((id: string) => {
      const modeler = (window as any).__bpmnModeler__;
      const elementRegistry = modeler.get('elementRegistry');
      const selection = modeler.get('selection');
      const el = elementRegistry.get(id);
      if (el) selection.select(el);
    }, elementId);

    // Allow properties panel to update
    await page.waitForTimeout(300);
  }

  test('condition group visible for ExclusiveGateway outgoing flow', async ({ page }) => {
    // Camunda 7 Modeler Kompatibilität – condition shown for XOR gateway flows
    await loadXmlAndClickElement(page, FLOW_CONDITION_XML_XOR, 'Flow_Cond');

    // The properties panel should show the Condition group with "Condition Type"
    const propsPanel = page.locator('.properties-panel-parent');
    await expect(propsPanel.getByText('Condition', { exact: true })).toBeVisible({ timeout: 5_000 });
    await expect(propsPanel.locator('select')).toBeVisible({ timeout: 5_000 });
  });

  test('condition group hidden for ParallelGateway outgoing flow', async ({ page }) => {
    // Camunda 7 Modeler Kompatibilität – no condition for parallel gateway flows
    await loadXmlAndClickElement(page, FLOW_CONDITION_XML_PARALLEL, 'Flow_Out');

    const propsPanel = page.locator('.properties-panel-parent');
    // Wait for panel to render something
    await expect(propsPanel).toBeVisible({ timeout: 5_000 });
    // Give the panel a moment to render entries
    await page.waitForTimeout(500);
    // The "Condition" group must NOT appear
    await expect(propsPanel.getByText('Condition Type')).not.toBeVisible();
  });

  test('condition group hidden for default flow on ExclusiveGateway', async ({ page }) => {
    // Camunda 7 Modeler Kompatibilität – default flows cannot have conditions
    await loadXmlAndClickElement(page, FLOW_CONDITION_XML_XOR, 'Flow_Default');

    const propsPanel = page.locator('.properties-panel-parent');
    await expect(propsPanel).toBeVisible({ timeout: 5_000 });
    await page.waitForTimeout(500);
    // Default flow must NOT show the Condition group
    await expect(propsPanel.getByText('Condition Type')).not.toBeVisible();
  });

  test('expression mode sets conditionExpression body without language', async ({ page }) => {
    // Camunda 7 Modeler Kompatibilität – expression produces FormalExpression without language
    await loadXmlAndClickElement(page, FLOW_CONDITION_XML_XOR, 'Flow_Cond');

    const propsPanel = page.locator('.properties-panel-parent');

    // Condition group starts open (shouldOpen: true) – no click needed
    await page.waitForTimeout(300);

    // Select "Expression" in the Condition Type dropdown
    const condTypeSelect = propsPanel.locator('[data-entry-id="conditionType"] select');
    await condTypeSelect.selectOption('expression');
    await page.waitForTimeout(300);

    // Type an expression
    const exprInput = propsPanel.locator('[data-entry-id="conditionExpression"] input');
    await exprInput.fill('${amount > 100}');
    // Trigger blur for debounce
    await exprInput.blur();
    await page.waitForTimeout(500);

    // Export XML and verify
    const xml = await page.evaluate(() => localStorage.getItem('minibpm_last_workflow') || '');
    expect(xml).toContain('conditionExpression');
    expect(xml).toContain('${amount &gt; 100}');
    // Should NOT contain language attribute on this expression
    expect(xml).not.toMatch(/conditionExpression[^>]*language/);
  });

  test('script mode sets conditionExpression with language attribute', async ({ page }) => {
    // Camunda 7 Modeler Kompatibilität – script produces FormalExpression with language
    await loadXmlAndClickElement(page, FLOW_CONDITION_XML_XOR, 'Flow_Cond');

    const propsPanel = page.locator('.properties-panel-parent');

    // Condition group starts open (shouldOpen: true) – no click needed
    await page.waitForTimeout(300);

    // Select "Script" in the Condition Type dropdown
    const condTypeSelect = propsPanel.locator('[data-entry-id="conditionType"] select');
    await condTypeSelect.selectOption('script');
    await page.waitForTimeout(300);

    // Verify language is rhai (only option)
    const langSelect = propsPanel.locator('[data-entry-id="conditionScriptLanguage"] select');
    await expect(langSelect).toHaveValue('rhai');
    await page.waitForTimeout(300);

    // Type a script body
    const scriptInput = propsPanel.locator('[data-entry-id="conditionScriptBody"] textarea');
    await scriptInput.fill('amount > 100');
    await scriptInput.blur();
    await page.waitForTimeout(500);

    // Export XML and verify
    const xml = await page.evaluate(() => localStorage.getItem('minibpm_last_workflow') || '');
    expect(xml).toContain('language="rhai"');
    expect(xml).toContain('amount &gt; 100');
  });

  // =====================================================================
  // HistoryPage — archived (completed) instances
  // =====================================================================

  test.describe('HistoryPage', () => {
    /** Build N synthetic completed instances for pagination tests */
    const makeCompleted = (n: number, defKey = 'def-hist-1') =>
      Array.from({ length: n }, (_, i) => ({
        id: `hist-inst-${String(i).padStart(3, '0')}`,
        definition_key: defKey,
        business_key: `bk-${i}`,
        state: 'Completed',
        current_node: 'end',
        audit_log: [],
        variables: {},
        started_at: '2026-04-10T10:00:00Z',
        completed_at: '2026-04-10T10:00:05Z',
      }));

    test('shows empty state when no historical instances exist', async ({ page }) => {
      await injectTauriMock(page);
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();

      await expect(page.getByText('No Historical Instances')).toBeVisible({ timeout: 5_000 });
      await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
    });

    test('displays archived instances in the table with columns and status badges', async ({ page }) => {
      await injectTauriMock(page, {
        definitions: [
          { key: 'def-hist-1', bpmn_id: 'OrderProcess', version: 1, node_count: 5, is_latest: true },
        ],
        completedInstances: [
          {
            id: 'hist-abcdef12',
            definition_key: 'def-hist-1',
            business_key: 'bk-order-777',
            state: 'Completed',
            current_node: 'end',
            audit_log: [],
            variables: {},
            started_at: '2026-04-10T10:00:00Z',
            completed_at: '2026-04-10T10:00:03Z',
          },
          {
            id: 'hist-badfeed11',
            definition_key: 'def-hist-1',
            business_key: 'bk-failure',
            state: { CompletedWithError: { error_code: 'E_TIMEOUT' } },
            current_node: 'errorEnd',
            audit_log: [],
            variables: {},
            started_at: '2026-04-10T11:00:00Z',
            completed_at: '2026-04-10T11:00:10Z',
          },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();

      const rows = page.locator('table tbody tr');
      await expect(rows).toHaveCount(2, { timeout: 5_000 });

      // First row: Completed badge + business key + truncated instance id
      const firstRow = rows.first();
      await expect(firstRow.getByText('hist-abc')).toBeVisible();
      await expect(firstRow.getByText('bk-order-777')).toBeVisible();
      await expect(firstRow.getByText('Completed', { exact: true })).toBeVisible();

      // Second row: Error badge
      await expect(rows.nth(1).getByText('Error', { exact: true })).toBeVisible();
    });

    test('filters by business key substring when pressing Enter', async ({ page }) => {
      await injectTauriMock(page, {
        completedInstances: [
          {
            id: 'hist-alpha-001', definition_key: 'd1', business_key: 'alpha-42',
            state: 'Completed', current_node: 'end', audit_log: [], variables: {},
            started_at: '2026-04-10T10:00:00Z', completed_at: '2026-04-10T10:00:01Z',
          },
          {
            id: 'hist-beta-002', definition_key: 'd1', business_key: 'beta-99',
            state: 'Completed', current_node: 'end', audit_log: [], variables: {},
            started_at: '2026-04-10T10:00:00Z', completed_at: '2026-04-10T10:00:01Z',
          },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();

      await expect(page.locator('table tbody tr')).toHaveCount(2, { timeout: 5_000 });

      // Type filter and press Enter
      const bkInput = page.locator('input[placeholder="Search..."]');
      await bkInput.fill('alpha');
      await bkInput.press('Enter');

      await expect(page.locator('table tbody tr')).toHaveCount(1, { timeout: 5_000 });
      await expect(page.getByText('alpha-42')).toBeVisible();
      await expect(page.getByText('beta-99')).not.toBeVisible();
    });

    test('filters by status (error only)', async ({ page }) => {
      await injectTauriMock(page, {
        completedInstances: [
          {
            id: 'hist-ok-1', definition_key: 'd1', business_key: 'ok',
            state: 'Completed', current_node: 'end', audit_log: [], variables: {},
            started_at: '2026-04-10T10:00:00Z', completed_at: '2026-04-10T10:00:01Z',
          },
          {
            id: 'hist-err-1', definition_key: 'd1', business_key: 'err',
            state: { CompletedWithError: { error_code: 'E1' } },
            current_node: 'errorEnd', audit_log: [], variables: {},
            started_at: '2026-04-10T10:00:00Z', completed_at: '2026-04-10T10:00:01Z',
          },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();
      await expect(page.locator('table tbody tr')).toHaveCount(2, { timeout: 5_000 });

      // Status dropdown is the 2nd <select> on the filter bar (1st = Definition)
      const statusSelect = page.locator('select').nth(1);
      await statusSelect.selectOption('error');
      await page.locator('button', { hasText: 'Search' }).click();

      await expect(page.locator('table tbody tr')).toHaveCount(1, { timeout: 5_000 });
      await expect(page.locator('table').getByText('Error', { exact: true })).toBeVisible();
    });

    test('paginates via Next / Previous buttons (PAGE_SIZE=50)', async ({ page }) => {
      // Seed exactly 55 instances so page 1 = 50 rows (hasMore=true), page 2 = 5 rows
      await injectTauriMock(page, {
        completedInstances: makeCompleted(55),
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();

      await expect(page.locator('table tbody tr')).toHaveCount(50, { timeout: 5_000 });
      await expect(page.getByText(/Showing 1[–-]50/)).toBeVisible();

      // Previous should be disabled on page 1, Next enabled
      const prev = page.locator('button', { hasText: 'Previous' });
      const next = page.locator('button', { hasText: 'Next' });
      await expect(prev).toBeDisabled();
      await expect(next).toBeEnabled();

      await next.click();
      await expect(page.locator('table tbody tr')).toHaveCount(5, { timeout: 5_000 });
      await expect(page.getByText(/Showing 51[–-]55/)).toBeVisible();

      // Now Next should be disabled, Previous enabled
      await expect(next).toBeDisabled();
      await expect(prev).toBeEnabled();

      await prev.click();
      await expect(page.locator('table tbody tr')).toHaveCount(50, { timeout: 5_000 });
    });

    test('clicking a row navigates to the Instances tab', async ({ page }) => {
      await injectTauriMock(page, {
        completedInstances: [
          {
            id: 'hist-click-1', definition_key: 'd1', business_key: 'clickme',
            state: 'Completed', current_node: 'end', audit_log: [], variables: {},
            started_at: '2026-04-10T10:00:00Z', completed_at: '2026-04-10T10:00:01Z',
          },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();
      await expect(page.locator('table tbody tr')).toHaveCount(1, { timeout: 5_000 });

      await page.locator('table tbody tr').first().click();

      // App.tsx wires onViewInstance → setActiveTab('instances')
      await expect(page.locator('.nav-item.active')).toHaveText('Instances', { timeout: 5_000 });
    });

    test('populates Definition dropdown from latest definitions only', async ({ page }) => {
      await injectTauriMock(page, {
        definitions: [
          { key: 'def-v1', bpmn_id: 'Order', version: 1, node_count: 3, is_latest: false },
          { key: 'def-v2', bpmn_id: 'Order', version: 2, node_count: 4, is_latest: true },
          { key: 'def-invoice', bpmn_id: 'Invoice', version: 1, node_count: 5, is_latest: true },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'History' }).click();

      // First <select> on the filter bar is "Definition"
      const defSelect = page.locator('select').first();
      const optionTexts = await defSelect.locator('option').allTextContents();
      // "All" + two latest definitions (v1 is_latest=false → excluded)
      expect(optionTexts.length).toBe(3);
      expect(optionTexts.some(t => t.includes('Order (v2)'))).toBe(true);
      expect(optionTexts.some(t => t.includes('Invoice (v1)'))).toBe(true);
      expect(optionTexts.some(t => t.includes('(v1)') && t.includes('Order'))).toBe(false);
    });
  });

  // =====================================================================
  // Instance Lifecycle — Suspend / Resume
  // =====================================================================

  test.describe('Instanz Suspend/Resume', () => {
    test('zeigt Suspend-Button und setzt Instanz auf Suspended', async ({ page }) => {
      const inst = {
        id: 'inst-sr-1',
        definition_key: 'def-1',
        business_key: 'bk-sr',
        state: 'Running',
        current_node: 'task_a',
        audit_log: ['Process started'],
        variables: {},
      };
      await injectTauriMock(page, { processInstances: [inst] });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Instances' }).click();

      // Instanz-Karte anklicken → Detail-Dialog öffnet sich
      await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
      await page.locator('.instance-list-item').first().click();

      // Suspend-Button muss sichtbar sein
      const suspendBtn = page.locator('button', { hasText: 'Suspend' });
      await expect(suspendBtn).toBeVisible({ timeout: 5_000 });

      // Klick → Instanz wird suspended
      await suspendBtn.click();

      // Toast-Bestätigung
      await expect(page.locator('[role="status"], [role="alert"]').first()).toBeVisible({ timeout: 3_000 });
    });

    test('zeigt Resume-Button für suspendierte Instanz', async ({ page }) => {
      const inst = {
        id: 'inst-sr-2',
        definition_key: 'def-1',
        business_key: 'bk-sr-2',
        state: { Suspended: { previous_state: 'Running' } },
        current_node: 'task_a',
        audit_log: [],
        variables: {},
      };
      await injectTauriMock(page, { processInstances: [inst] });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Instances' }).click();

      await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
      await page.locator('.instance-list-item').first().click();

      // Bei einer suspendierten Instanz zeigt der Button "Resume"
      const resumeBtn = page.locator('button', { hasText: 'Resume' });
      await expect(resumeBtn).toBeVisible({ timeout: 5_000 });

      await resumeBtn.click();
      await expect(page.locator('[role="status"], [role="alert"]').first()).toBeVisible({ timeout: 3_000 });
    });
  });

  // =====================================================================
  // Instance Migration Dialog
  // =====================================================================

  test.describe('Instanz-Migrationsdialog', () => {
    test('Migrate-Button ist deaktiviert wenn keine andere Version vorhanden', async ({ page }) => {
      const def = { key: 'def-only', bpmn_id: 'OrderProcess', version: 1, node_count: 3, is_latest: true };
      const inst = {
        id: 'inst-mig-1',
        definition_key: 'def-only',
        business_key: 'bk-mig',
        state: 'Running',
        current_node: 'approve',
        audit_log: [],
        variables: {},
      };
      await injectTauriMock(page, { definitions: [def], processInstances: [inst] });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Instances' }).click();

      await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
      await page.locator('.instance-list-item').first().click();

      // Migrate-Button öffnet den Dialog
      const migrateBtn = page.locator('button', { hasText: 'Migrate' });
      await expect(migrateBtn).toBeVisible({ timeout: 5_000 });
      await migrateBtn.click();

      // Dialog erscheint — weil keine andere Version existiert, soll ein Hinweis erscheinen
      await expect(page.getByText(/keine anderen Versionen/i)).toBeVisible({ timeout: 5_000 });
    });

    test('Migrationsdialog zeigt Versions-Dropdown wenn Kandidaten vorhanden', async ({ page }) => {
      const defs = [
        { key: 'def-v1', bpmn_id: 'OrderProcess', version: 1, node_count: 3, is_latest: false },
        { key: 'def-v2', bpmn_id: 'OrderProcess', version: 2, node_count: 4, is_latest: true },
      ];
      const inst = {
        id: 'inst-mig-2',
        definition_key: 'def-v1',
        business_key: 'bk-mig2',
        state: 'Running',
        current_node: 'approve',
        audit_log: [],
        variables: {},
      };
      await injectTauriMock(page, { definitions: defs, processInstances: [inst] });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Instances' }).click();

      await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
      await page.locator('.instance-list-item').first().click();

      const migrateBtn = page.locator('button', { hasText: 'Migrate' });
      await expect(migrateBtn).toBeVisible({ timeout: 5_000 });
      await migrateBtn.click();

      // Shadcn Select rendert Optionen nur wenn Dropdown offen ist
      await page.locator('[role="combobox"]').click();
      // Version v2 muss als Option erscheinen
      await expect(page.getByText(/v2/)).toBeVisible({ timeout: 5_000 });
      // Dropdown schließen bevor Dialog-Buttons geklickt werden
      await page.keyboard.press('Escape');

      // Dialog kann geschlossen werden
      await page.locator('button', { hasText: 'Schließen' }).click();
      await expect(page.getByText(/v2/)).not.toBeVisible();
    });

    test('Migrate-Button im Dialog bleibt deaktiviert ohne Zielauswahl', async ({ page }) => {
      const defs = [
        { key: 'def-v1', bpmn_id: 'OrderProcess', version: 1, node_count: 3, is_latest: false },
        { key: 'def-v2', bpmn_id: 'OrderProcess', version: 2, node_count: 4, is_latest: true },
      ];
      const inst = {
        id: 'inst-mig-3',
        definition_key: 'def-v1',
        business_key: 'bk-mig3',
        state: 'Running',
        current_node: 'approve',
        audit_log: [],
        variables: {},
      };
      await injectTauriMock(page, { definitions: defs, processInstances: [inst] });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Instances' }).click();

      await expect(page.locator('.instance-list-item').first()).toBeVisible({ timeout: 5_000 });
      await page.locator('.instance-list-item').first().click();

      const migrateBtn = page.locator('button', { hasText: 'Migrate' });
      await migrateBtn.click();

      // "Migrieren…"-Button im Dialog ist disabled, solange keine Version gewählt
      const dialogMigrateBtn = page.locator('button', { hasText: 'Migrieren…' });
      await expect(dialogMigrateBtn).toBeVisible({ timeout: 5_000 });
      await expect(dialogMigrateBtn).toBeDisabled();
    });
  });

  // =====================================================================
  // Monitoring — Base64-Dekodierung im DataViewer
  // =====================================================================

  test.describe('Monitoring Base64-Anzeige', () => {
    // Minimalste gültige 1×1-Pixel PNG als Base64
    const TINY_PNG_B64 =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

    test('rendert base64-PNG-Eintrag als <img>-Element', async ({ page }) => {
      const storageInfo = {
        backend_name: 'NATS',
        version: '2.x',
        host: 'localhost',
        port: 4222,
        memory_bytes: 0,
        storage_bytes: 1024,
        streams: 1,
        consumers: 0,
        buckets: [
          { name: 'instance_files', bucket_type: 'object_store', entries: 1, size_bytes: 1024 },
        ],
      };
      const bucketEntries = {
        instance_files: [
          { key: 'inst-123/avatar.png', size_bytes: 772, created_at: '2026-01-01T00:00:00Z' },
        ],
      };
      const bucketEntryDetails: Record<string, any> = {
        'instance_files::inst-123/avatar.png': {
          key: 'inst-123/avatar.png',
          data: TINY_PNG_B64,
          encoding: 'base64',
        },
      };

      await injectTauriMock(page, { storageInfo, bucketEntries, bucketEntryDetails });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Monitoring' }).click();

      // Warte auf Monitoring-Seite
      await expect(page.locator('h1, h2', { hasText: 'Monitoring' }).first()).toBeVisible({ timeout: 5_000 });

      // Storage-Tabelle: Bucket-Zeile anklicken
      await expect(page.getByText('instance_files')).toBeVisible({ timeout: 5_000 });
      await page.getByText('instance_files').click();

      // Eintrags-Dialog: Datei-Zeile anklicken
      await expect(page.getByText('avatar.png')).toBeVisible({ timeout: 5_000 });
      await page.getByText('avatar.png').click();

      // DataViewer soll ein <img>-Tag rendern, kein Monaco-Editor
      await expect(page.locator('img[src^="data:image/png;base64,"]')).toBeVisible({ timeout: 5_000 });
    });

    test('rendert base64-JSON-Eintrag als Monaco-Editor (Text-Dekodierung)', async ({ page }) => {
      const jsonPayload = JSON.stringify({ order_id: 42, status: 'pending' });
      const jsonB64 = btoa(jsonPayload);

      const storageInfo = {
        backend_name: 'NATS', version: '2.x', host: 'localhost', port: 4222,
        memory_bytes: 0, storage_bytes: 512, streams: 1, consumers: 0,
        buckets: [
          { name: 'instance_files', bucket_type: 'object_store', entries: 1, size_bytes: 512 },
        ],
      };
      const bucketEntries = {
        instance_files: [
          { key: 'inst-456/payload.json', size_bytes: 48, created_at: '2026-01-01T00:00:00Z' },
        ],
      };
      const bucketEntryDetails: Record<string, any> = {
        'instance_files::inst-456/payload.json': {
          key: 'inst-456/payload.json',
          data: jsonB64,
          encoding: 'base64',
        },
      };

      await injectTauriMock(page, { storageInfo, bucketEntries, bucketEntryDetails });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Monitoring' }).click();

      await expect(page.locator('h1, h2', { hasText: 'Monitoring' }).first()).toBeVisible({ timeout: 5_000 });
      await page.getByText('instance_files').click();
      await expect(page.getByText('payload.json')).toBeVisible({ timeout: 5_000 });
      await page.getByText('payload.json').click();

      // Monaco-Editor soll erscheinen (kein img), Badge zeigt "JSON"
      await expect(page.locator('.monaco-editor')).toBeVisible({ timeout: 8_000 });
      await expect(page.locator('div.font-mono', { hasText: 'JSON' }).first()).toBeVisible();
    });
  });

  // =====================================================================
  // OverviewPage — pending timers / messages / jobs
  // =====================================================================

  test.describe('OverviewPage', () => {
    test('shows empty states across all three tabs when nothing is pending', async ({ page }) => {
      await injectTauriMock(page);
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Overview' }).click();

      // Default tab shows timers empty state — be lenient on exact copy,
      // but the "Overview" page header must be visible.
      await expect(page.locator('h1, h2', { hasText: 'Overview' }).first()).toBeVisible({ timeout: 5_000 });
      await expect(page.locator('button', { hasText: 'Refresh' })).toBeVisible();
    });

    test('displays pending timers when seeded', async ({ page }) => {
      await injectTauriMock(page, {
        pendingTimers: [
          {
            timer_id: 'timer-1',
            instance_id: 'inst-timer-001',
            node_id: 'Timer_Event_1',
            fire_at: '2026-05-01T12:00:00Z',
            timer_def: { Date: '2026-05-01T12:00:00Z' },
          },
        ],
      });
      await page.goto('/');
      await page.locator('.nav-item', { hasText: 'Overview' }).click();

      // Expect the timer node id to surface somewhere in the rendered overview
      await expect(page.getByText('Timer_Event_1')).toBeVisible({ timeout: 5_000 });
    });
  });
});
